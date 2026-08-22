#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod local_relay;
mod room;
mod room_bridge;
mod twitch;

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

use room::{RoomFeatureFilters, RoomService, RoomSnapshot};
use room_bridge::RoomBridge;
use serde::Serialize;
use tauri::{Manager, PhysicalPosition};
use twitch::{TwitchService, TwitchSnapshot};

const DEFAULT_ENGINE_IPC_ADDR: &str = "127.0.0.1:47731";
const DEFAULT_WINDOW_IPC_ADDR: &str = "127.0.0.1:47732";
fn engine_ipc_addr() -> String {
    env::var("SCREEN_OVERLAY_ENGINE_IPC_ADDR")
        .unwrap_or_else(|_| DEFAULT_ENGINE_IPC_ADDR.to_string())
}
fn window_ipc_addr() -> String {
    env::var("SCREEN_OVERLAY_WINDOW_IPC_ADDR")
        .unwrap_or_else(|_| DEFAULT_WINDOW_IPC_ADDR.to_string())
}
const RENDERER_SIDECAR_BASE: &str = "screen-overlay-renderer";
const RENDERER_SIDECAR_EXE: &str = "screen-overlay-renderer-x86_64-pc-windows-msvc.exe";
const DEFAULT_TWITCH_CLIENT_ID: &str = "vxukm6l5twy8m4pyena0pyfuim1sw2";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub(crate) fn room_trace(message: impl AsRef<str>) {
    let Ok(path) = env::var("SCREEN_OVERLAY_ROOM_TRACE_PATH") else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "ui[{}] {}", std::process::id(), message.as_ref());
    }
}

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
    obs_output_enabled: bool,
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
            obs_output_enabled: false,
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
    obs_output_enabled: bool,
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DisplayInfo {
    id: String,
    label: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    primary: bool,
}

#[tauri::command]
fn display_layout(app: tauri::AppHandle) -> Result<Vec<DisplayInfo>, String> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(Vec::new());
    };
    let primary = window
        .primary_monitor()
        .map_err(|error| error.to_string())?;
    let primary_position = primary.as_ref().map(|monitor| monitor.position());
    let primary_size = primary.as_ref().map(|monitor| monitor.size());
    let monitors = window
        .available_monitors()
        .map_err(|error| error.to_string())?;
    Ok(monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let position = monitor.position();
            let size = monitor.size();
            let is_primary = primary_position == Some(position) && primary_size == Some(size);
            let name = monitor
                .name()
                .filter(|name| !name.trim().is_empty())
                .cloned();
            DisplayInfo {
                id: format!(
                    "{}:{}:{}:{}",
                    position.x, position.y, size.width, size.height
                ),
                label: name.unwrap_or_else(|| format!("Display {}", index + 1)),
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
                primary: is_primary,
            }
        })
        .collect())
}

#[tauri::command]
fn frontend_ready(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main control window is unavailable".to_string())?;
    window.show().map_err(|error| error.to_string())?;
    let _ = window.set_focus();
    Ok(())
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
fn twitch_snapshot(state: tauri::State<'_, TwitchService>) -> Result<TwitchSnapshot, String> {
    Ok(state.snapshot())
}

#[tauri::command]
fn twitch_connect(state: tauri::State<'_, TwitchService>) -> Result<TwitchSnapshot, String> {
    state.connect()
}

#[tauri::command]
fn twitch_disconnect(state: tauri::State<'_, TwitchService>) -> Result<TwitchSnapshot, String> {
    state.disconnect()
}

#[tauri::command]
fn twitch_set_features(
    chat_enabled: bool,
    bits_enabled: bool,
    state: tauri::State<'_, TwitchService>,
) -> Result<TwitchSnapshot, String> {
    state.set_features(chat_enabled, bits_enabled)
}

#[tauri::command]
fn room_snapshot(
    state: tauri::State<'_, RoomService>,
    bridge: tauri::State<'_, RoomBridge>,
) -> Result<RoomSnapshot, String> {
    state.pump_transport(&bridge);
    Ok(state.snapshot())
}

#[tauri::command]
fn room_host(
    display_name: String,
    target_display_id: Option<String>,
    allow_guest_interaction: bool,
    feature_filters: RoomFeatureFilters,
    app: tauri::AppHandle,
    state: tauri::State<'_, RoomService>,
    bridge: tauri::State<'_, RoomBridge>,
) -> Result<RoomSnapshot, String> {
    let (target_display_id, target_display) = resolve_room_display(&app, target_display_id, false)?;
    let snapshot = state.host(
        display_name,
        target_display_id,
        allow_guest_interaction,
        feature_filters,
    )?;
    let payload = serde_json::json!({
        "allowInteraction": allow_guest_interaction,
        "targetDisplay": target_display,
    });
    let _ = send_to_native_engine("network_enter_host", &payload);
    bridge.send("enterHost", payload);
    room::emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn room_join(
    invite_url: String,
    display_name: String,
    target_display_id: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<'_, RoomService>,
    bridge: tauri::State<'_, RoomBridge>,
) -> Result<RoomSnapshot, String> {
    let (target_display_id, target_display) = resolve_room_display(&app, target_display_id, true)?;
    let snapshot = state.join(invite_url, display_name, target_display_id)?;
    let payload = serde_json::json!({ "targetDisplay": target_display });
    let _ = send_to_native_engine("network_enter_guest", &payload);
    bridge.send("enterGuest", payload);
    room::emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

fn resolve_room_display(
    app: &tauri::AppHandle,
    requested_id: Option<String>,
    prefer_secondary: bool,
) -> Result<(Option<String>, Option<DisplayInfo>), String> {
    let displays = display_layout(app.clone())?;
    let selected = requested_id
        .as_deref()
        .and_then(|id| displays.iter().find(|display| display.id == id))
        .or_else(|| {
            prefer_secondary
                .then(|| displays.iter().find(|display| !display.primary))
                .flatten()
        })
        .or_else(|| displays.iter().find(|display| display.primary))
        .or_else(|| displays.first())
        .cloned();
    Ok((
        selected.as_ref().map(|display| display.id.clone()),
        selected,
    ))
}

#[tauri::command]
fn room_leave(
    app: tauri::AppHandle,
    state: tauri::State<'_, RoomService>,
    bridge: tauri::State<'_, RoomBridge>,
) -> Result<RoomSnapshot, String> {
    let snapshot = state.leave()?;
    let _ = send_to_native_engine("network_leave", &serde_json::json!({}));
    bridge.send("leaveRoom", serde_json::json!({}));
    room::emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn room_end(
    app: tauri::AppHandle,
    state: tauri::State<'_, RoomService>,
    bridge: tauri::State<'_, RoomBridge>,
) -> Result<RoomSnapshot, String> {
    room_leave(app, state, bridge)
}

#[tauri::command]
fn room_set_interaction(
    enabled: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, RoomService>,
    bridge: tauri::State<'_, RoomBridge>,
) -> Result<RoomSnapshot, String> {
    let snapshot = state.set_interaction(enabled)?;
    let _ = send_to_native_engine(
        "network_set_interaction",
        &serde_json::json!({ "enabled": enabled }),
    );
    bridge.send("setInteraction", serde_json::json!({ "enabled": enabled }));
    room::emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn room_set_participant_interaction(
    participant_id: String,
    enabled: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, RoomService>,
) -> Result<RoomSnapshot, String> {
    let snapshot = state.set_participant_interaction(participant_id, enabled)?;
    room::emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn room_remove_participant(
    participant_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, RoomService>,
) -> Result<RoomSnapshot, String> {
    let snapshot = state.remove_participant(participant_id)?;
    room::emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn room_set_features(
    feature_filters: RoomFeatureFilters,
    app: tauri::AppHandle,
    state: tauri::State<'_, RoomService>,
) -> Result<RoomSnapshot, String> {
    let snapshot = state.set_features(feature_filters)?;
    room::emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn room_regenerate_invite(
    app: tauri::AppHandle,
    state: tauri::State<'_, RoomService>,
) -> Result<RoomSnapshot, String> {
    let snapshot = state.regenerate_invite()?;
    room::emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn room_reconnect(
    app: tauri::AppHandle,
    state: tauri::State<'_, RoomService>,
) -> Result<RoomSnapshot, String> {
    let snapshot = state.reconnect()?;
    room::emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn open_room_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("room") {
        let _ = window.unminimize();
        window.show().map_err(|error| error.to_string())?;
        let _ = window.set_focus();
        return Ok(());
    }

    let window =
        tauri::WebviewWindowBuilder::new(&app, "room", tauri::WebviewUrl::App("room.html".into()))
            .title("ScreenOverlayPhysics Shared Room")
            .inner_size(480.0, 640.0)
            .min_inner_size(420.0, 520.0)
            .resizable(true)
            .decorations(true)
            .transparent(false)
            .always_on_top(true)
            .skip_taskbar(false)
            .shadow(false)
            .build()
            .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    let _ = window.center();
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
fn hide_room_window(app: tauri::AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("room") else {
        return Ok(());
    };
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
fn minimize_room_window(app: tauri::AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("room") else {
        return Ok(());
    };
    window.minimize().map_err(|error| error.to_string())
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
        }
        Err(_) => {
            model.engine_connected = false;
            model.transport = "native ipc offline".to_string();
        }
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

/// Asks the engine for its published case state. Returns `None` when the engine
/// is not running, which the page shows as offline rather than as an error.
#[tauri::command]
fn get_case_status() -> Option<serde_json::Value> {
    let response = query_native_engine("query_case", &serde_json::json!({})).ok()?;
    response.get("case").cloned()
}

fn send_to_native_engine(command: &str, payload: &serde_json::Value) -> Result<(), String> {
    query_native_engine(command, payload).map(|_| ())
}

/// Sends one command and returns the engine's acknowledgement, which carries a
/// payload for queries and is a bare `{"ok":true}` for everything else.
fn query_native_engine(
    command: &str,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let addr: SocketAddr = engine_ipc_addr()
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
        Ok(response)
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
        "set_obs_output" => {
            model.obs_output_enabled = payload
                .get("enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(model.obs_output_enabled);
        }
        "apply_runtime_settings" => {
            apply_runtime_settings(&mut model.settings, payload);
            if let Some(click_through) = payload
                .get("clickThrough")
                .and_then(serde_json::Value::as_bool)
            {
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
        }
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
        obs_output_enabled: model.obs_output_enabled,
        fps: 0,
        object_count: model.object_count,
        queued_commands: model.command_log.len(),
        last_command: model.command_log.first().map(|entry| entry.label.clone()),
        command_log: model.command_log.clone(),
    }
}

fn apply_runtime_settings(settings: &mut RuntimeSettings, payload: &serde_json::Value) {
    settings.gravity_y = payload_f32(payload, "gravityY", settings.gravity_y, 0.0, 4000.0);
    settings.throw_sensitivity = payload_f32(
        payload,
        "throwSensitivity",
        settings.throw_sensitivity,
        0.1,
        4.0,
    );
    settings.max_throw_speed = payload_f32(
        payload,
        "maxThrowSpeed",
        settings.max_throw_speed,
        100.0,
        6000.0,
    );
    settings.restitution = payload_f32(payload, "restitution", settings.restitution, 0.05, 1.2);
    settings.linear_damping = payload_f32(
        payload,
        "linearDamping",
        settings.linear_damping,
        0.9,
        0.999,
    );
    settings.sleep_threshold = payload_f32(
        payload,
        "sleepThreshold",
        settings.sleep_threshold,
        1.0,
        120.0,
    );
    settings.floor_snap_threshold = payload_f32(
        payload,
        "floorSnapThreshold",
        settings.floor_snap_threshold,
        0.0,
        24.0,
    );
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

fn claim_control_instance() -> Option<TcpListener> {
    let address = window_ipc_addr();
    claim_control_instance_at(&address)
}

fn claim_control_instance_at(address: &str) -> Option<TcpListener> {
    match TcpListener::bind(&address) {
        Ok(listener) => Some(listener),
        Err(_) => {
            if let Ok(mut existing) = TcpStream::connect(&address) {
                let _ = existing.write_all(b"show\n");
                let _ = existing.flush();
            }
            None
        }
    }
}

#[cfg(test)]
mod instance_tests {
    use super::*;

    #[test]
    fn a_second_control_instance_cannot_claim_the_same_port() {
        let probe = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let address = probe.local_addr().expect("read ephemeral address");
        drop(probe);

        let first = claim_control_instance_at(&address.to_string())
            .expect("first instance should claim the address");
        let second = claim_control_instance_at(&address.to_string());

        assert!(second.is_none());
        drop(first);
    }
}

fn start_window_ipc(app: tauri::AppHandle, listener: TcpListener) {
    thread::Builder::new()
        .name("control-window-ipc".to_string())
        .spawn(move || {
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
    let Ok(addr) = engine_ipc_addr().parse::<SocketAddr>() else {
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
            let base = if subdir.is_empty() {
                root.clone()
            } else {
                root.join(subdir)
            };
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

fn twitch_client_id() -> Option<String> {
    env::var("SCREEN_OVERLAY_TWITCH_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            option_env!("SCREEN_OVERLAY_TWITCH_CLIENT_ID")
                .map(str::to_string)
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            option_env!("TWITCH_CLIENT_ID")
                .map(str::to_string)
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| Some(DEFAULT_TWITCH_CLIENT_ID.to_string()))
}

fn main() {
    let Some(instance_listener) = claim_control_instance() else {
        return;
    };
    let twitch_service = TwitchService::start(twitch_client_id(), send_to_native_engine);
    let room_service = RoomService::default();
    let room_bridge = RoomBridge::start_from_environment();
    room_service.start_transport_pump(room_bridge.clone());

    tauri::Builder::default()
        .setup(move |app| {
            start_window_ipc(app.handle().clone(), instance_listener);
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
                        let x = work_area.position.x + work_area.size.width as i32
                            - size.width as i32
                            - margin;
                        let y = work_area.position.y + work_area.size.height as i32
                            - size.height as i32
                            - margin;
                        let _ = window.set_position(PhysicalPosition::new(
                            x.max(work_area.position.x),
                            y.max(work_area.position.y),
                        ));
                    }
                }
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
        .manage(room_service)
        .manage(room_bridge)
        .manage(twitch_service)
        .invoke_handler(tauri::generate_handler![
            engine_snapshot,
            twitch_snapshot,
            twitch_connect,
            twitch_disconnect,
            twitch_set_features,
            room_snapshot,
            room_host,
            room_join,
            room_leave,
            room_end,
            room_set_interaction,
            room_set_participant_interaction,
            room_remove_participant,
            room_set_features,
            room_regenerate_invite,
            room_reconnect,
            open_room_window,
            hide_room_window,
            minimize_room_window,
            display_layout,
            frontend_ready,
            dispatch_engine_command,
            get_case_status,
            set_overlay_interactive,
            hide_overlay,
            minimize_overlay,
            exit_overlay
        ])
        .run(tauri::generate_context!())
        .expect("error while running ScreenOverlayPhysics Control");
}
