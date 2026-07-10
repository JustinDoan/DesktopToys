#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::{
    env,
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::Duration,
};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use serde::Serialize;
use tauri::{Manager, PhysicalPosition};

const ENGINE_IPC_ADDR: &str = "127.0.0.1:47731";
const WINDOW_IPC_ADDR: &str = "127.0.0.1:47732";
const RENDERER_SIDECAR_BASE: &str = "screen-overlay-renderer";
const RENDERER_SIDECAR_EXE: &str = "screen-overlay-renderer-x86_64-pc-windows-msvc.exe";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Default)]
struct ControlState {
    model: Mutex<ControlModel>,
}

#[derive(Debug)]
struct ControlModel {
    command_log: Vec<CommandEntry>,
    active_preset: String,
    scene_mode: String,
    settings: RuntimeSettings,
    physics_paused: bool,
    shatter_gun_equipped: bool,
    click_through: bool,
    overlay_interactive: bool,
    object_count: u32,
    engine_connected: bool,
    transport: String,
}

impl Default for ControlModel {
    fn default() -> Self {
        Self {
            command_log: Vec::new(),
            active_preset: "Desk orbit".to_string(),
            scene_mode: "Play".to_string(),
            settings: RuntimeSettings::default(),
            physics_paused: false,
            shatter_gun_equipped: false,
            click_through: true,
            overlay_interactive: true,
            object_count: 7,
            engine_connected: false,
            transport: "native ipc idle".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineSnapshot {
    connected: bool,
    transport: String,
    renderer_backend: String,
    overlay_mode: String,
    active_preset: String,
    scene_mode: String,
    settings: RuntimeSettings,
    physics_paused: bool,
    shatter_gun_equipped: bool,
    click_through: bool,
    overlay_interactive: bool,
    fps: u32,
    object_count: u32,
    queued_commands: usize,
    last_command: Option<String>,
    command_log: Vec<CommandEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandEntry {
    id: u64,
    label: String,
    payload: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSettings {
    gravity_y: f32,
    throw_sensitivity: f32,
    max_throw_speed: f32,
    restitution: f32,
    linear_damping: f32,
    sleep_threshold: f32,
    floor_snap_threshold: f32,
    interaction_debounce_ms: u32,
    start_in_pass_through: bool,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            gravity_y: 1800.0,
            throw_sensitivity: 1.1,
            max_throw_speed: 2600.0,
            restitution: 0.75,
            linear_damping: 0.992,
            sleep_threshold: 24.0,
            floor_snap_threshold: 3.0,
            interaction_debounce_ms: 80,
            start_in_pass_through: true,
        }
    }
}

#[tauri::command]
fn engine_snapshot(state: tauri::State<'_, ControlState>) -> Result<EngineSnapshot, String> {
    let model = state
        .model
        .lock()
        .map_err(|_| "Control model lock was poisoned.".to_string())?;
    Ok(snapshot_from_model(&model))
}

#[tauri::command]
fn dispatch_engine_command(
    command: String,
    payload: serde_json::Value,
    state: tauri::State<'_, ControlState>,
) -> Result<EngineSnapshot, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("Command cannot be empty.".to_string());
    }

    if command == "clear_command_log" {
        let mut model = state
            .model
            .lock()
            .map_err(|_| "Control model lock was poisoned.".to_string())?;
        model.command_log.clear();
        return Ok(snapshot_from_model(&model));
    }

    let delivery = send_to_native_engine(command, &payload);

    let mut model = state
        .model
        .lock()
        .map_err(|_| "Control model lock was poisoned.".to_string())?;

    apply_command(&mut model, command, &payload);
    match &delivery {
        Ok(()) => {
            model.engine_connected = true;
            model.transport = "native ipc connected".to_string();
        },
        Err(_) => {
            model.engine_connected = false;
            model.transport = "native ipc offline".to_string();
        },
    }

    let next_id = model
        .command_log
        .first()
        .map(|entry| entry.id + 1)
        .unwrap_or(1);
    model.command_log.insert(
        0,
        CommandEntry {
            id: next_id,
            label: command.to_string(),
            payload: serde_json::json!({
                "payload": payload,
                "delivered": delivery.is_ok(),
                "error": delivery.as_ref().err()
            })
            .to_string(),
        },
    );
    model.command_log.truncate(8);

    Ok(snapshot_from_model(&model))
}

#[tauri::command]
fn set_overlay_interactive(
    interactive: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, ControlState>,
) -> Result<EngineSnapshot, String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_ignore_cursor_events(!interactive)
            .map_err(|error| error.to_string())?;
    }

    let mut model = state
        .model
        .lock()
        .map_err(|_| "Control model lock was poisoned.".to_string())?;
    model.overlay_interactive = interactive;
    let next_id = model
        .command_log
        .first()
        .map(|entry| entry.id + 1)
        .unwrap_or(1);
    model.command_log.insert(
        0,
        CommandEntry {
            id: next_id,
            label: "set_overlay_interactive".to_string(),
            payload: serde_json::json!({ "interactive": interactive }).to_string(),
        },
    );
    model.command_log.truncate(8);

    Ok(snapshot_from_model(&model))
}

#[tauri::command]
fn hide_overlay(app: tauri::AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
fn minimize_overlay(app: tauri::AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    window.minimize().map_err(|error| error.to_string())
}

#[tauri::command]
fn exit_overlay(app: tauri::AppHandle) -> Result<(), String> {
    hide_overlay(app)
}

fn send_to_native_engine(command: &str, payload: &serde_json::Value) -> Result<(), String> {
    let addr: SocketAddr = ENGINE_IPC_ADDR
        .parse()
        .map_err(|error| format!("bad endpoint: {error}"))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(220))
        .map_err(|error| format!("connect failed: {error}"))?;
    let _ = stream.set_write_timeout(Some(Duration::from_millis(300)));
    let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));

    let message = serde_json::json!({
        "command": command,
        "payload": payload,
    });
    writeln!(stream, "{message}").map_err(|error| format!("send failed: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("flush failed: {error}"))?;

    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .map_err(|error| format!("ack failed: {error}"))?;
    let response: serde_json::Value =
        serde_json::from_str(response.trim()).map_err(|error| format!("bad ack: {error}"))?;
    if response
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err("native rejected command".to_string())
    }
}

fn apply_command(model: &mut ControlModel, command: &str, payload: &serde_json::Value) {
    match command {
        "pause_physics" => model.physics_paused = true,
        "resume_physics" => model.physics_paused = false,
        "toggle_click_through" => {
            model.click_through = payload
                .get("enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(model.click_through);
        }
        "apply_runtime_settings" => {
            apply_runtime_settings(&mut model.settings, payload);
            if let Some(click_through) = payload.get("clickThrough").and_then(serde_json::Value::as_bool) {
                model.click_through = click_through;
            }
        }
        "apply_preset" => {
            if let Some(preset) = payload.get("preset").and_then(serde_json::Value::as_str) {
                model.active_preset = preset.to_string();
            }
        }
        "set_scene_mode" => {
            if let Some(mode) = payload.get("mode").and_then(serde_json::Value::as_str) {
                model.scene_mode = mode.to_string();
            }
        }
        "reset_scene" => {
            model.object_count = 7;
            model.shatter_gun_equipped = false;
        },
        "toggle_shatter_gun" => model.shatter_gun_equipped = !model.shatter_gun_equipped,
        "shatter_screen" => model.object_count = 118,
        "spawn_stress_batch" => model.object_count = model.object_count.saturating_add(25),
        command if command.starts_with("spawn_") => {
            model.object_count = model.object_count.saturating_add(1);
        }
        _ => {}
    }
}

fn snapshot_from_model(model: &ControlModel) -> EngineSnapshot {
    EngineSnapshot {
        connected: model.engine_connected,
        transport: model.transport.clone(),
        renderer_backend: "native overlay".to_string(),
        overlay_mode: "pass-through ready".to_string(),
        active_preset: model.active_preset.clone(),
        scene_mode: model.scene_mode.clone(),
        settings: model.settings,
        physics_paused: model.physics_paused,
        shatter_gun_equipped: model.shatter_gun_equipped,
        click_through: model.click_through,
        overlay_interactive: model.overlay_interactive,
        fps: 0,
        object_count: model.object_count,
        queued_commands: model.command_log.len(),
        last_command: model.command_log.first().map(|entry| entry.label.clone()),
        command_log: model.command_log.clone(),
    }
}

fn apply_runtime_settings(settings: &mut RuntimeSettings, payload: &serde_json::Value) {
    settings.gravity_y = payload_f32(payload, "gravityY", settings.gravity_y, 0.0, 4000.0);
    settings.throw_sensitivity = payload_f32(payload, "throwSensitivity", settings.throw_sensitivity, 0.1, 4.0);
    settings.max_throw_speed = payload_f32(payload, "maxThrowSpeed", settings.max_throw_speed, 100.0, 6000.0);
    settings.restitution = payload_f32(payload, "restitution", settings.restitution, 0.05, 1.2);
    settings.linear_damping = payload_f32(payload, "linearDamping", settings.linear_damping, 0.9, 0.999);
    settings.sleep_threshold = payload_f32(payload, "sleepThreshold", settings.sleep_threshold, 1.0, 120.0);
    settings.floor_snap_threshold = payload_f32(payload, "floorSnapThreshold", settings.floor_snap_threshold, 0.0, 24.0);
    settings.interaction_debounce_ms = payload
        .get("interactionDebounceMs")
        .and_then(serde_json::Value::as_u64)
        .map(|value| value.clamp(0, 300) as u32)
        .unwrap_or(settings.interaction_debounce_ms);
    settings.start_in_pass_through = payload
        .get("startInPassThrough")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(settings.start_in_pass_through);
}

fn payload_f32(payload: &serde_json::Value, key: &str, fallback: f32, min: f32, max: f32) -> f32 {
    payload
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .map(|value| (value as f32).clamp(min, max))
        .unwrap_or(fallback)
}

fn start_window_ipc(app: tauri::AppHandle) {
    thread::Builder::new()
        .name("control-window-ipc".to_string())
        .spawn(move || {
            let Ok(listener) = TcpListener::bind(WINDOW_IPC_ADDR) else {
                return;
            };

            for stream in listener.incoming().flatten() {
                handle_window_ipc_stream(stream, &app);
            }
        })
        .expect("failed to spawn Control UI window IPC thread");
}

fn handle_window_ipc_stream(stream: TcpStream, app: &tauri::AppHandle) {
    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    if reader.read_line(&mut line).is_err() {
        return;
    }

    let command = line.trim();
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    match command {
        "show" => {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        "hide" => {
            let _ = window.hide();
        }
        "minimize" => {
            let _ = window.minimize();
        }
        _ => {}
    }
}

fn start_native_renderer_sidecar(app: &tauri::AppHandle) {
    if native_engine_is_online() {
        return;
    }

    let Some(renderer_path) = find_native_renderer_sidecar(app) else {
        return;
    };

    let mut command = Command::new(renderer_path);
    configure_hidden_process(&mut command);
    let _ = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn native_engine_is_online() -> bool {
    let Ok(addr) = ENGINE_IPC_ADDR.parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(80)).is_ok()
}

fn find_native_renderer_sidecar(app: &tauri::AppHandle) -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        roots.push(resource_dir);
    }
    if let Ok(current_exe) = env::current_exe() {
        for ancestor in current_exe.ancestors() {
            roots.push(ancestor.to_path_buf());
        }
    }

    let names = [
        RENDERER_SIDECAR_EXE,
        concat!("screen-overlay-renderer", ".exe"),
        RENDERER_SIDECAR_BASE,
    ];
    let subdirs = ["", "binaries", "resources"];

    for root in roots {
        for subdir in subdirs {
            let base = if subdir.is_empty() { root.clone() } else { root.join(subdir) };
            for name in names {
                let candidate = base.join(name);
                if candidate.is_file() && is_not_current_exe(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn is_not_current_exe(candidate: &Path) -> bool {
    env::current_exe()
        .map(|current| current != candidate)
        .unwrap_or(true)
}

fn configure_hidden_process(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            start_window_ipc(app.handle().clone());
            start_native_renderer_sidecar(app.handle());
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_always_on_top(true);
                let _ = window.set_skip_taskbar(true);
                let _ = window.set_shadow(false);
                let _ = window.set_ignore_cursor_events(false);
                let monitor = window
                    .current_monitor()
                    .ok()
                    .flatten()
                    .or_else(|| window.primary_monitor().ok().flatten());
                if let Some(monitor) = monitor {
                    if let Ok(size) = window.outer_size() {
                        let work_area = monitor.work_area();
                        let margin = 18;
                        let x = work_area.position.x + work_area.size.width as i32 - size.width as i32 - margin;
                        let y = work_area.position.y + work_area.size.height as i32 - size.height as i32 - margin;
                        let _ = window.set_position(PhysicalPosition::new(x.max(work_area.position.x), y.max(work_area.position.y)));
                    }
                }
                let _ = window.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .manage(ControlState::default())
        .invoke_handler(tauri::generate_handler![
            engine_snapshot,
            dispatch_engine_command,
            set_overlay_interactive,
            hide_overlay,
            minimize_overlay,
            exit_overlay
        ])
        .run(tauri::generate_context!())
        .expect("error while running ScreenOverlayPhysics Control");
}
