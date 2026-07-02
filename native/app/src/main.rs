#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::{borrow::Cow, collections::HashMap, error::Error, sync::Arc, time::{Duration, Instant}};

#[cfg(target_os = "windows")]
use std::ffi::CString;

use anyhow::{Context, Result};
use core_types::{AppColor, AppConfig, CollisionShape, ObjectState, ObjectVisualKind, RectF, Vector2};
use native_shell::{
    configure_overlay_window, overlay_window_attributes, pick_model_file, set_overlay_input_mode, show_error_dialog,
    sync_window_to_monitor,
    GlobalImportKeys, GlobalInputPoller, OverlayInputMode, TrayAction, TrayController,
};
use renderer::{GpuVertex, HudState, OverlayPanel, PanelLine, RenderScene, SceneRenderer};
use scene_logic::{DragController, FrameClock, HitTester, SceneController};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};
#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::{BOOL, HINSTANCE, HWND, HMODULE, LPARAM, LRESULT, WPARAM},
    Graphics::{
        Direct3D::{
            Fxc::D3DCompile, ID3DBlob, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D_DRIVER_TYPE_HARDWARE,
            D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL,
        },
        Direct3D11::{
            D3D11CreateDevice, ID3D11BlendState, ID3D11Buffer, ID3D11DepthStencilState, ID3D11DepthStencilView,
            ID3D11Device, ID3D11DeviceContext, ID3D11InputLayout, ID3D11PixelShader, ID3D11RasterizerState,
            ID3D11RenderTargetView, ID3D11Texture2D, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER,
            D3D11_BIND_DEPTH_STENCIL, D3D11_BIND_VERTEX_BUFFER, D3D11_BLEND_DESC, D3D11_BLEND_INV_SRC_ALPHA,
            D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD, D3D11_BUFFER_DESC, D3D11_CLEAR_DEPTH,
            D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_COMPARISON_LESS_EQUAL, D3D11_CPU_ACCESS_WRITE,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_SINGLETHREADED, D3D11_CULL_NONE,
            D3D11_DEPTH_STENCIL_DESC, D3D11_DEPTH_WRITE_MASK_ALL, D3D11_FILL_SOLID, D3D11_INPUT_ELEMENT_DESC,
            D3D11_INPUT_PER_VERTEX_DATA, D3D11_MAP_WRITE_DISCARD, D3D11_MAPPED_SUBRESOURCE,
            D3D11_RASTERIZER_DESC, D3D11_RENDER_TARGET_BLEND_DESC, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
            D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC, D3D11_VIEWPORT,
        },
        DirectComposition::{DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual},
        Dxgi::{
            CreateDXGIFactory1, IDXGIDevice, IDXGIFactory2, IDXGISwapChain1, DXGI_SWAP_CHAIN_DESC1,
            DXGI_SCALING_STRETCH, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
        },
        Dxgi::Common::{
            DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_D32_FLOAT,
            DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_R32G32B32_FLOAT, DXGI_SAMPLE_DESC,
        },
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, GetWindowLongPtrW, RegisterClassW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
        GWL_EXSTYLE, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_SHOWNA,
        WINDOW_EX_STYLE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_EX_TRANSPARENT, WS_POPUP,
    },
};
#[cfg(target_os = "windows")]
use windows::core::{w, ComInterface, HRESULT, PCSTR};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct RenderUniforms {
    viewport_size: [f32; 2],
    camera_distance: f32,
    _padding: f32,
}

impl RenderUniforms {
    fn new(width: u32, height: u32) -> Self {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        Self {
            viewport_size: [width, height],
            camera_distance: width.max(height) * 1.25,
            _padding: 0.0,
        }
    }
}

#[derive(Debug)]
struct FpsCounter {
    sample_started_at: Instant,
    frames: u32,
    fps: f32,
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self {
            sample_started_at: Instant::now(),
            frames: 0,
            fps: 0.0,
        }
    }
}

impl FpsCounter {
    fn record_frame(&mut self, now: Instant) {
        self.frames = self.frames.saturating_add(1);
        let elapsed = now.saturating_duration_since(self.sample_started_at);
        if elapsed >= Duration::from_millis(250) {
            self.fps = self.frames as f32 / elapsed.as_secs_f32().max(0.001);
            self.frames = 0;
            self.sample_started_at = now;
        }
    }

    fn fps(&self) -> f32 {
        self.fps
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = NativeApp::default();
    event_loop.run_app(&mut app).map_err(Into::into)
}

struct NativeApp {
    window: Option<Arc<Window>>,
    window_id: Option<WindowId>,
    gpu: Option<GpuState>,
    #[cfg(target_os = "windows")]
    d3d_renderer: Option<WindowsD3dRenderer>,
    renderer: SceneRenderer,
    scene: SceneController,
    frame_clock: FrameClock,
    hit_tester: HitTester,
    drag_controller: DragController,
    input_poller: Option<GlobalInputPoller>,
    tray: Option<TrayController>,
    bounds: RectF,
    overlay_mode: OverlayInputMode,
    pending_mode: OverlayInputMode,
    mode_candidate_since_seconds: f64,
    cursor_local: Vector2,
    selected_id: Option<u64>,
    debug_visible: bool,
    debug_hit_primary_cursor: bool,
    debug_left_down: bool,
    debug_right_down: bool,
    was_left_down: bool,
    was_right_down: bool,
    was_stress_spawn_down: bool,
    was_slingshot_toggle_down: bool,
    was_robot_buddy_down: bool,
    force_interactive_for_debug: bool,
    is_rotation_dragging: bool,
    last_drag_attempt: String,
    last_rotation_cursor: Vector2,
    hud: HudState,
    status_message: Option<String>,
    next_input_retry_seconds: f64,
    settings_panel: SettingsPanel,
    import_panel: Option<ImportPanel>,
    slingshot_game: SlingshotGame,
    robot_carries: HashMap<u64, RobotCarry>,
    robot_drop_cooldowns: HashMap<u64, RobotDropCooldown>,
    robot_bin_ids: Vec<u64>,
    fallback_left_down: bool,
    fallback_right_down: bool,
    previous_global_import_keys: GlobalImportKeys,
    target_frame_duration: Duration,
    next_frame_at: Instant,
    fps_counter: FpsCounter,
}

const FLOOR_MARGIN_PIXELS: f32 = 18.0;
const STRESS_SPAWN_COUNT: usize = 25;
const ROBOT_STACK_DROP_COOLDOWN_SECONDS: f64 = 1.8;
const ROBOT_THROW_HOLD_SECONDS: f64 = 0.55;
const ROBOT_BIN_WIDTH: f32 = 118.0;
const ROBOT_BIN_HEIGHT: f32 = 96.0;
const ROBOT_BIN_WALL: f32 = 12.0;

impl Default for NativeApp {
    fn default() -> Self {
        let config = AppConfig::default();
        let now = Instant::now();
        Self {
            window: None,
            window_id: None,
            gpu: None,
            #[cfg(target_os = "windows")]
            d3d_renderer: None,
            renderer: SceneRenderer::new(),
            scene: SceneController::new(config),
            frame_clock: FrameClock::default(),
            hit_tester: HitTester::default(),
            drag_controller: DragController::new(10),
            input_poller: None,
            tray: None,
            bounds: RectF::new(0.0, 0.0, 1280.0, 720.0),
            overlay_mode: OverlayInputMode::Interactive,
            pending_mode: OverlayInputMode::Interactive,
            mode_candidate_since_seconds: 0.0,
            cursor_local: Vector2::ZERO,
            selected_id: None,
            debug_visible: false,
            debug_hit_primary_cursor: false,
            debug_left_down: false,
            debug_right_down: false,
            was_left_down: false,
            was_right_down: false,
            was_stress_spawn_down: false,
            was_slingshot_toggle_down: false,
            was_robot_buddy_down: false,
            force_interactive_for_debug: false,
            is_rotation_dragging: false,
            last_drag_attempt: "none".to_string(),
            last_rotation_cursor: Vector2::ZERO,
            hud: HudState::default(),
            status_message: None,
            next_input_retry_seconds: 0.0,
            settings_panel: SettingsPanel::default(),
            import_panel: None,
            slingshot_game: SlingshotGame::default(),
            robot_carries: HashMap::new(),
            robot_drop_cooldowns: HashMap::new(),
            robot_bin_ids: Vec::new(),
            fallback_left_down: false,
            fallback_right_down: false,
            previous_global_import_keys: GlobalImportKeys::default(),
            target_frame_duration: Duration::ZERO,
            next_frame_at: now,
            fps_counter: FpsCounter::default(),
        }
    }
}

impl NativeApp {
    fn scene_bounds(&self) -> RectF {
        RectF::new(
            0.0,
            0.0,
            self.bounds.width.max(1.0),
            (self.bounds.height - FLOOR_MARGIN_PIXELS).max(1.0),
        )
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            self.input_poller = Some(
                GlobalInputPoller::try_new_with_prompt().map_err(|error| {
                    anyhow::anyhow!(
                        "{error} Grant access in System Settings > Privacy & Security > Accessibility, then relaunch the app."
                    )
                })?,
            );
        }

        #[cfg(not(target_os = "macos"))]
        {
            self.input_poller = match GlobalInputPoller::try_new_with_prompt() {
                Ok(poller) => Some(poller),
                Err(error) => {
                    self.status_message = Some(format!(
                        "{error} Using local input only until permission is granted."
                    ));
                    self.next_input_retry_seconds = 1.0;
                    None
                },
            };
        }

        let initial_bounds = monitor_bounds(event_loop).unwrap_or(self.bounds);
        self.bounds = initial_bounds;

        let window = Arc::new(
            event_loop
                .create_window(overlay_window_attributes("ScreenOverlayPhysics Native", self.bounds))
                .context("Failed to create native overlay window")?,
        );
        configure_overlay_window(&window)?;
        self.bounds = sync_window_to_monitor(&window);
        self.target_frame_duration = Duration::ZERO;
        self.overlay_mode = if self.scene.config().start_in_pass_through {
            OverlayInputMode::PassThrough
        } else {
            OverlayInputMode::Interactive
        };
        self.pending_mode = self.overlay_mode;
        let _ = set_overlay_input_mode(&window, self.overlay_mode);

        #[cfg(target_os = "windows")]
        {
            let renderer = WindowsD3dRenderer::new(self.bounds)?;
            renderer.set_input_mode(self.overlay_mode)?;
            self.d3d_renderer = Some(renderer);
            window.set_visible(false);
        }

        #[cfg(not(target_os = "windows"))]
        {
            let gpu_width = self.bounds.width.max(1.0).round() as u32;
            let gpu_height = self.bounds.height.max(1.0).round() as u32;
            let gpu = pollster::block_on(GpuState::new(window.clone(), gpu_width, gpu_height))
                .context("Failed to create GPU renderer")?;
            self.gpu = Some(gpu);
        }

        self.window_id = Some(window.id());
        self.window = Some(window);
        self.scene.initialize(self.scene_bounds());
        self.selected_id = self.scene.objects().last().map(|object| object.id);
        self.settings_panel = SettingsPanel::from_config(*self.scene.config());
        self.tray = TrayController::new().ok();
        if self.tray.is_none() {
            self.status_message = Some("Tray icon unavailable on this host.".to_string());
        }
        #[cfg(target_os = "windows")]
        self.push_status_message("Renderer backend: Direct3D 11 + DirectComposition".to_string());
        #[cfg(not(target_os = "windows"))]
        if let Some(gpu) = &self.gpu {
            self.push_status_message(format!("Renderer backend: {}", gpu.backend_label));
        }
        self.push_status_message("Frame pacing: uncapped".to_string());
        Ok(())
    }

    fn update(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };

        self.frame_clock.tick();
        let dt = self.frame_clock.delta_time_seconds;
        let now = self.frame_clock.elapsed_seconds;

        if self.input_poller.is_none() && now >= self.next_input_retry_seconds {
            match GlobalInputPoller::try_new() {
                Ok(poller) => {
                    self.input_poller = Some(poller);
                    self.force_interactive_for_debug = false;
                    self.push_status_message("Global drag input enabled.".to_string());
                },
                Err(_) => {
                    self.next_input_retry_seconds = now + 2.0;
                },
            }
        }

        self.scene.set_gravity(self.scene.config().gravity_y);
        let pointer = if let Some(poller) = &self.input_poller {
            match poller.poll(self.bounds) {
                Ok(pointer) => {
                    self.cursor_local = pointer.local_position;
                    pointer
                },
                Err(error) => {
                    self.input_poller = None;
                    self.status_message = Some(format!(
                        "{error}. Falling back to local input only."
                    ));
                    self.force_interactive_for_debug = true;
                    self.next_input_retry_seconds = now + 2.0;
                    native_shell::GlobalPointerState {
                        screen_position: (0, 0),
                        local_position: self.cursor_local,
                        left_down: self.fallback_left_down,
                        right_down: self.fallback_right_down,
                        spawn_stress_down: false,
                        slingshot_toggle_down: false,
                        robot_buddy_down: false,
                        import_keys: GlobalImportKeys::default(),
                    }
                },
            }
        } else {
            native_shell::GlobalPointerState {
                screen_position: (0, 0),
                local_position: self.cursor_local,
                left_down: self.fallback_left_down,
                right_down: self.fallback_right_down,
                spawn_stress_down: false,
                slingshot_toggle_down: false,
                robot_buddy_down: false,
                import_keys: GlobalImportKeys::default(),
            }
        };
        self.debug_left_down = pointer.left_down;
        self.debug_right_down = pointer.right_down;
        self.debug_hit_primary_cursor = self
            .hit_tester
            .is_point_over_any_object(self.scene.objects(), self.cursor_local);

        self.update_click_through_mode(now, &window);

        if self.drag_controller.is_dragging() {
            self.drag_controller
                .update_drag(self.scene.objects_mut(), self.cursor_local, now);
            self.update_held_object_rotation(pointer.right_down);
        }

        self.handle_global_mouse_buttons(now, pointer.left_down, pointer.right_down);
        self.handle_global_stress_spawn(pointer.spawn_stress_down);
        self.handle_global_slingshot_toggle(pointer.slingshot_toggle_down);
        self.handle_global_robot_buddy(pointer.robot_buddy_down);
        self.handle_global_import_keys(pointer.import_keys);
        self.update_slingshot_game();
        self.update_robot_buddies(dt);
        self.scene.step(dt, self.scene_bounds());
        self.collect_robot_bin_cubes();
        self.stabilize_robot_buddies();
        self.sync_panels();
        window.request_redraw();
    }

    fn update_click_through_mode(&mut self, now_seconds: f64, window: &Window) {
        let should_be_interactive = self.force_interactive_for_debug
            || self.input_poller.is_none()
            || self.drag_controller.is_dragging()
            || self.debug_hit_primary_cursor
            || self.settings_panel.visible
            || self.import_panel.is_some();
        let desired_mode = if should_be_interactive {
            OverlayInputMode::Interactive
        } else {
            OverlayInputMode::PassThrough
        };

        if desired_mode != self.pending_mode {
            self.pending_mode = desired_mode;
            self.mode_candidate_since_seconds = now_seconds;
        }

        let mut debounce_seconds = self.scene.config().interaction_debounce_ms as f64 / 1000.0;
        if desired_mode == OverlayInputMode::Interactive {
            debounce_seconds = debounce_seconds.min(0.02);
        }

        if now_seconds - self.mode_candidate_since_seconds >= debounce_seconds && desired_mode != self.overlay_mode {
            let mut applied_mode = set_overlay_input_mode(window, desired_mode).is_ok();
            #[cfg(target_os = "windows")]
            {
                if let Some(renderer) = &self.d3d_renderer {
                    match renderer.set_input_mode(desired_mode) {
                        Ok(()) => applied_mode = true,
                        Err(error) => {
                            self.status_message = Some(format!("Overlay input mode update failed: {error:#}"));
                        },
                    }
                }
            }

            if applied_mode {
                self.overlay_mode = desired_mode;
            }
        }
    }

    fn handle_global_mouse_buttons(&mut self, now_seconds: f64, is_left_down: bool, is_right_down: bool) {
        if self.settings_panel.visible || self.import_panel.is_some() {
            self.was_left_down = is_left_down;
            self.was_right_down = is_right_down;
            self.is_rotation_dragging = false;
            return;
        }

        if self.slingshot_game.active {
            self.handle_slingshot_mouse(is_left_down);
            self.was_left_down = is_left_down;
            self.was_right_down = is_right_down;
            self.is_rotation_dragging = false;
            return;
        }

        if is_left_down && !self.was_left_down {
            let began = self.drag_controller.begin_drag(
                self.scene.objects_mut(),
                self.cursor_local,
                now_seconds,
                &self.hit_tester,
            );
            self.last_drag_attempt = if let Some(id) = began {
                self.selected_id = Some(id);
                self.last_rotation_cursor = self.cursor_local;
                "begin:primary".to_string()
            } else {
                "miss:primary".to_string()
            };
        } else if !is_left_down && self.was_left_down && self.drag_controller.is_dragging() {
            self.end_drag_and_apply_spin(now_seconds);
            self.last_drag_attempt = "release".to_string();
        }

        if !is_right_down && self.was_right_down {
            self.is_rotation_dragging = false;
        }

        self.was_left_down = is_left_down;
        self.was_right_down = is_right_down;
    }

    fn handle_global_stress_spawn(&mut self, is_down: bool) {
        if is_down && !self.was_stress_spawn_down {
            self.spawn_stress_cubes();
        }
        self.was_stress_spawn_down = is_down;
    }

    fn handle_global_slingshot_toggle(&mut self, is_down: bool) {
        if is_down && !self.was_slingshot_toggle_down {
            self.toggle_slingshot_game();
        }
        self.was_slingshot_toggle_down = is_down;
    }

    fn handle_global_robot_buddy(&mut self, is_down: bool) {
        if is_down && !self.was_robot_buddy_down {
            self.spawn_robot_buddy();
        }
        self.was_robot_buddy_down = is_down;
    }

    fn handle_global_import_keys(&mut self, keys: GlobalImportKeys) {
        if self.import_panel.is_none() {
            self.previous_global_import_keys = keys;
            return;
        }

        let previous = self.previous_global_import_keys;
        if keys.up && !previous.up {
            self.handle_import_key(KeyCode::ArrowUp);
        }
        if keys.down && !previous.down {
            self.handle_import_key(KeyCode::ArrowDown);
        }
        if keys.left && !previous.left {
            self.handle_import_key(KeyCode::ArrowLeft);
        }
        if keys.right && !previous.right {
            self.handle_import_key(KeyCode::ArrowRight);
        }
        if keys.enter && !previous.enter {
            self.handle_import_key(KeyCode::Enter);
        }
        if keys.escape && !previous.escape {
            self.handle_import_key(KeyCode::Escape);
        }
        self.previous_global_import_keys = keys;
    }

    fn spawn_stress_cubes(&mut self) {
        let id = self
            .scene
            .spawn_small_cube_batch(self.default_spawn_position(), STRESS_SPAWN_COUNT);
        self.selected_id = id;
        self.push_status_message(format!("Spawned {STRESS_SPAWN_COUNT} stress cubes."));
    }

    fn spawn_robot_buddy(&mut self) {
        let mut position = self.default_spawn_position();
        position.y = (position.y + 120.0).min(self.scene_bounds().bottom() - 140.0);
        let id = self.scene.spawn_object(
            position,
            Some(AppColor::from_rgb(150, 220, 245)),
            ObjectVisualKind::RobotBuddy,
        );
        self.selected_id = Some(id);
        self.push_status_message("Robot buddy joined the desktop.".to_string());
    }

    fn update_robot_buddies(&mut self, _dt: f32) {
        if self.slingshot_game.active {
            return;
        }
        let objects = self.scene.objects().to_vec();
        let robot_ids: Vec<u64> = objects
            .iter()
            .filter(|object| object.visual_kind == ObjectVisualKind::RobotBuddy)
            .map(|object| object.id)
            .collect();
        if !robot_ids.is_empty() {
            self.ensure_robot_bin();
        }
        self.robot_carries
            .retain(|robot_id, carry| robot_ids.contains(robot_id) && objects.iter().any(|object| object.id == carry.object_id));
        self.robot_drop_cooldowns.retain(|robot_id, cooldown| {
            robot_ids.contains(robot_id) && self.frame_clock.elapsed_seconds - cooldown.dropped_at < ROBOT_STACK_DROP_COOLDOWN_SECONDS
        });

        for robot_id in robot_ids {
            let Some(robot_snapshot) = objects.iter().find(|object| object.id == robot_id) else {
                continue;
            };
            if self.drag_controller.dragged_id() == Some(robot_id) || robot_snapshot.is_dragging {
                self.drop_robot_carry(robot_id, 0.0);
                if let Some(robot) = self.scene.objects_mut().iter_mut().find(|object| object.id == robot_id) {
                    robot.body.motor_enabled = false;
                    robot.body.motor_velocity_x = 0.0;
                }
                continue;
            }

            let robot_center = Vector2::new(
                robot_snapshot.body.position.x + robot_snapshot.body.width * 0.5,
                robot_snapshot.body.position.y + robot_snapshot.body.height * 0.5,
            );
            let facing = if robot_snapshot.body.motor_velocity_x < -1.0 || robot_snapshot.body.velocity.x < -1.0 {
                -1.0
            } else {
                1.0
            };

            if let Some(carry) = self.robot_carries.get(&robot_id).copied() {
                let held_seconds = self.frame_clock.elapsed_seconds - carry.picked_up_at;
                let near_edge = robot_center.x < 80.0 || robot_center.x > self.scene_bounds().right() - 80.0;
                if near_edge || held_seconds > 7.0 {
                    self.drop_robot_carry(robot_id, facing * 70.0);
                } else if held_seconds >= ROBOT_THROW_HOLD_SECONDS {
                    self.position_robot_carry(robot_snapshot, carry.object_id);
                    self.throw_robot_carry_to_bin(robot_id, carry.object_id);
                } else {
                    self.position_robot_carry(robot_snapshot, carry.object_id);
                    self.drive_robot(robot_id, 0.0, 0.0);
                    continue;
                }
            }

            let is_carrying = self.robot_carries.contains_key(&robot_id);
            let nearest = if is_carrying {
                None
            } else {
                objects
                    .iter()
                    .filter(|object| self.is_robot_carry_candidate(robot_id, object))
                    .map(|object| {
                        let center = Vector2::new(
                            object.body.position.x + object.body.width * 0.5,
                            object.body.position.y + object.body.height * 0.5,
                        );
                        (object.id, center, (center - robot_center).length_squared())
                    })
                    .filter(|(_, _, distance)| *distance < 540.0 * 540.0)
                    .min_by(|(_, _, left), (_, _, right)| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal))
            };

            if let Some((object_id, object_center, distance)) = nearest {
                if distance < 95.0 * 95.0 {
                    self.robot_carries.insert(
                        robot_id,
                        RobotCarry {
                            object_id,
                            picked_up_at: self.frame_clock.elapsed_seconds,
                        },
                    );
                    self.position_robot_carry(robot_snapshot, object_id);
                }
                let direction = (object_center.x - robot_center.x).clamp(-1.0, 1.0);
                self.drive_robot(robot_id, direction, 108.0);
                continue;
            }

            let patrol = ((self.frame_clock.elapsed_seconds * 0.32 + robot_id as f64 * 0.17).sin() as f32).signum();
            let target_x = self.robot_bin_rect().right() + 210.0 + patrol * 180.0;
            let direction = (target_x - robot_center.x).clamp(-1.0, 1.0);
            let speed = if is_carrying { 72.0 } else { 64.0 };
            self.drive_robot(robot_id, direction, speed);
        }
    }

    fn is_robot_carry_candidate(&self, robot_id: u64, object: &ObjectState) -> bool {
        let bounds = self.scene_bounds();
        let bin_rect = self.robot_bin_rect();
        let center_x = object.body.position.x + object.body.width * 0.5;
        let bottom_y = object.body.position.y + object.body.height;
        let in_bin_zone = center_x > bin_rect.x - object.body.width && center_x < bin_rect.right() + object.body.width;
        object.id != robot_id
            && object.body.collidable
            && !object.is_dragging
            && !object.body.is_dragging
            && object.visual_kind == ObjectVisualKind::Cube
            && object.body.width <= 120.0
            && object.body.height <= 120.0
            && bottom_y > bounds.bottom() - 210.0
            && !in_bin_zone
            && !self.robot_carries.values().any(|carry| carry.object_id == object.id)
            && !self
                .robot_drop_cooldowns
                .get(&robot_id)
                .is_some_and(|cooldown| cooldown.object_id == object.id)
            && !matches!(
                object.visual_kind,
                ObjectVisualKind::RobotBuddy
                    | ObjectVisualKind::FoxBuddy
                    | ObjectVisualKind::DvdLogo
                    | ObjectVisualKind::ImportedModel
                    | ObjectVisualKind::GamePlank
                    | ObjectVisualKind::GameTarget
            )
    }

    fn drive_robot(&mut self, robot_id: u64, direction: f32, speed: f32) {
        let motor_velocity = if direction.abs() < 0.08 { 0.0 } else { direction.signum() * speed };
        if let Some(robot) = self.scene.objects_mut().iter_mut().find(|object| object.id == robot_id) {
            robot.body.is_dragging = false;
            robot.is_dragging = false;
            robot.body.is_sleeping = false;
            robot.body.gravity_scale = 1.0;
            robot.body.motor_enabled = motor_velocity != 0.0;
            robot.body.motor_velocity_x = motor_velocity;
            robot.body.velocity.x = motor_velocity;
            robot.rotation_x *= 0.65;
            robot.rotation_y *= 0.65;
            robot.rotation_z = 0.0;
            robot.angular_velocity_x = 0.0;
            robot.angular_velocity_y = 0.0;
            robot.angular_velocity_z = 0.0;
        }
    }

    fn position_robot_carry(&mut self, robot: &ObjectState, object_id: u64) {
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == object_id) {
            object.is_dragging = true;
            object.body.is_dragging = true;
            object.body.is_sleeping = false;
            object.body.velocity = Vector2::ZERO;
            object.rotation_z *= 0.92;
            let robot_center = Vector2::new(
                robot.body.position.x + robot.body.width * 0.5,
                robot.body.position.y + robot.body.height * 0.5,
            );
            let cargo_center_x = robot_center.x;
            let cargo_center_y = robot.body.position.y - object.body.height * 0.5 - 4.0;
            object.body.position = Vector2::new(
                cargo_center_x - object.body.width * 0.5,
                cargo_center_y - object.body.height * 0.5,
            );
        }
    }

    fn drop_robot_carry(&mut self, robot_id: u64, toss_x: f32) {
        let Some(carry) = self.robot_carries.remove(&robot_id) else {
            return;
        };
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == carry.object_id) {
            object.is_dragging = false;
            object.body.is_dragging = false;
            object.body.is_sleeping = false;
            object.body.velocity = Vector2::new(toss_x, -65.0);
        }
        self.robot_drop_cooldowns.insert(
            robot_id,
            RobotDropCooldown {
                object_id: carry.object_id,
                dropped_at: self.frame_clock.elapsed_seconds,
            },
        );
    }

    fn throw_robot_carry_to_bin(&mut self, robot_id: u64, object_id: u64) {
        let Some(carry) = self.robot_carries.remove(&robot_id) else {
            return;
        };
        let Some(throw_velocity) = self.robot_bin_throw_velocity(object_id) else {
            self.robot_carries.insert(robot_id, carry);
            return;
        };
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == object_id) {
            object.is_dragging = false;
            object.body.is_dragging = false;
            object.body.is_sleeping = false;
            object.body.velocity = throw_velocity;
            object.body.friction = 0.95;
            object.body.restitution = 0.18;
            object.body.linear_damping = 0.992;
            object.body.lock_rotation = false;
            object.rotation_x = 0.0;
            object.rotation_y = 0.0;
            object.rotation_z = 0.0;
            object.angular_velocity_x = 0.0;
            object.angular_velocity_y = 0.0;
            object.angular_velocity_z = 0.0;
        }
        self.robot_drop_cooldowns.insert(
            robot_id,
            RobotDropCooldown {
                object_id,
                dropped_at: self.frame_clock.elapsed_seconds,
            },
        );
    }

    fn robot_bin_throw_velocity(&self, object_id: u64) -> Option<Vector2> {
        let object = self.scene.objects().iter().find(|object| object.id == object_id)?;
        let mut target = self.robot_bin_target();
        let start = Vector2::new(
            object.body.position.x + object.body.width * 0.5,
            object.body.position.y + object.body.height * 0.5,
        );
        target.x += (target.x - start.x).signum() * ROBOT_BIN_WIDTH * 0.18;
        let distance_x = (target.x - start.x).abs();
        let travel_time = (distance_x / 520.0).clamp(1.05, 2.05);
        let gravity = self.scene.config().gravity_y.max(240.0);
        Some(Vector2::new(
            (target.x - start.x) / travel_time * 1.08,
            (target.y - start.y - 0.5 * gravity * travel_time * travel_time) / travel_time,
        ))
    }

    fn robot_bin_rect(&self) -> RectF {
        let bottom = self.scene_bounds().bottom();
        RectF::new(42.0, bottom - ROBOT_BIN_HEIGHT - 4.0, ROBOT_BIN_WIDTH, ROBOT_BIN_HEIGHT)
    }

    fn robot_bin_target(&self) -> Vector2 {
        let bin = self.robot_bin_rect();
        Vector2::new(bin.x + bin.width * 0.5, bin.y + bin.height * 0.34)
    }

    fn ensure_robot_bin(&mut self) {
        let existing_ids: std::collections::HashSet<u64> = self.scene.objects().iter().map(|object| object.id).collect();
        if self.robot_bin_ids.len() == 3 && self.robot_bin_ids.iter().all(|id| existing_ids.contains(id)) {
            return;
        }
        self.robot_bin_ids.clear();
        let bin = self.robot_bin_rect();
        let color = AppColor::from_rgb(70, 96, 108);
        let rim = AppColor::from_rgb(98, 142, 158);
        let floor_id = self.spawn_static_game_object(
            Vector2::new(bin.x, bin.y + bin.height - ROBOT_BIN_WALL),
            Vector2::new(bin.width, ROBOT_BIN_WALL),
            color,
            ObjectVisualKind::GamePlank,
        );
        let left_id = self.spawn_static_game_object(
            Vector2::new(bin.x, bin.y),
            Vector2::new(ROBOT_BIN_WALL, bin.height),
            rim,
            ObjectVisualKind::GamePlank,
        );
        let right_id = self.spawn_static_game_object(
            Vector2::new(bin.x + bin.width - ROBOT_BIN_WALL, bin.y),
            Vector2::new(ROBOT_BIN_WALL, bin.height),
            rim,
            ObjectVisualKind::GamePlank,
        );
        self.robot_bin_ids.extend([floor_id, left_id, right_id]);
    }

    fn collect_robot_bin_cubes(&mut self) {
        if self.robot_bin_ids.is_empty() {
            return;
        }
        let bin = self.robot_bin_rect();
        let collected: Vec<u64> = self
            .scene
            .objects()
            .iter()
            .filter(|object| object.visual_kind == ObjectVisualKind::Cube)
            .filter(|object| !object.body.is_dragging && !object.is_dragging)
            .filter(|object| {
                let center = Vector2::new(
                    object.body.position.x + object.body.width * 0.5,
                    object.body.position.y + object.body.height * 0.5,
                );
                center.x > bin.x + ROBOT_BIN_WALL
                    && center.x < bin.right() - ROBOT_BIN_WALL
                    && center.y > bin.y
                    && center.y < bin.bottom() - ROBOT_BIN_WALL * 0.5
            })
            .map(|object| object.id)
            .collect();
        if collected.is_empty() {
            return;
        }
        for id in &collected {
            let _ = self.scene.remove_object(*id);
        }
        self.robot_carries.retain(|_, carry| !collected.contains(&carry.object_id));
        self.robot_drop_cooldowns
            .retain(|_, cooldown| !collected.contains(&cooldown.object_id));
    }

    fn stabilize_robot_buddies(&mut self) {
        for object in self
            .scene
            .objects_mut()
            .iter_mut()
            .filter(|object| object.visual_kind == ObjectVisualKind::RobotBuddy)
        {
            object.rotation_x *= 0.35;
            object.rotation_y *= 0.35;
            object.rotation_z *= 0.2;
            if object.rotation_x.abs() < 1.0 {
                object.rotation_x = 0.0;
            }
            if object.rotation_y.abs() < 1.0 {
                object.rotation_y = 0.0;
            }
            if object.rotation_z.abs() < 1.0 {
                object.rotation_z = 0.0;
            }
            object.angular_velocity_x = 0.0;
            object.angular_velocity_y = 0.0;
            object.angular_velocity_z = 0.0;
            object.body.lock_rotation = true;
            object.body.is_sleeping = false;
        }
    }

    fn toggle_slingshot_game(&mut self) {
        if self.slingshot_game.active {
            self.slingshot_game = SlingshotGame::default();
            self.scene.reset(self.scene_bounds());
            self.selected_id = self.scene.objects().last().map(|object| object.id);
            self.push_status_message("Slingshot game off.".to_string());
        } else {
            self.start_slingshot_level();
        }
    }

    fn start_slingshot_level(&mut self) {
        let bounds = self.scene_bounds();
        self.scene.clear_objects();
        self.drag_controller.cancel_drag(self.scene.objects_mut());
        self.selected_id = None;
        self.slingshot_game = SlingshotGame::new(bounds);
        self.build_slingshot_level(bounds);
        self.push_status_message("Slingshot game: pull the orb back, release to fire. F3 resets, F10 exits.".to_string());
    }

    fn build_slingshot_level(&mut self, bounds: RectF) {
        let anchor = self.slingshot_game.anchor;
        self.spawn_static_game_object(
            Vector2::new(bounds.width * 0.5 - 220.0, bounds.bottom() - 28.0),
            Vector2::new(440.0, 28.0),
            AppColor::from_rgb(76, 124, 64),
            ObjectVisualKind::GamePlank,
        );
        self.spawn_static_game_object(
            Vector2::new((bounds.width * 0.66).max(anchor.x + 340.0) - 28.0, bounds.bottom() - 34.0),
            Vector2::new(500.0, 34.0),
            AppColor::from_rgb(84, 130, 68),
            ObjectVisualKind::GamePlank,
        );
        self.spawn_pinned_game_object(
            Vector2::new(anchor.x - 42.0, anchor.y + 16.0),
            Vector2::new(22.0, 104.0),
            AppColor::from_rgb(116, 74, 46),
            ObjectVisualKind::GamePlank,
        );
        self.spawn_pinned_game_object(
            Vector2::new(anchor.x + 18.0, anchor.y + 16.0),
            Vector2::new(22.0, 104.0),
            AppColor::from_rgb(116, 74, 46),
            ObjectVisualKind::GamePlank,
        );
        self.spawn_pinned_game_object(
            Vector2::new(anchor.x - 50.0, anchor.y + 104.0),
            Vector2::new(100.0, 20.0),
            AppColor::from_rgb(96, 62, 42),
            ObjectVisualKind::GamePlank,
        );
        let left_band = self.spawn_pinned_game_object(
            Vector2::new(anchor.x - 40.0, anchor.y - 4.0),
            Vector2::new(8.0, 8.0),
            AppColor::from_rgb(56, 35, 40),
            ObjectVisualKind::GamePlank,
        );
        let right_band = self.spawn_pinned_game_object(
            Vector2::new(anchor.x + 32.0, anchor.y - 4.0),
            Vector2::new(8.0, 8.0),
            AppColor::from_rgb(56, 35, 40),
            ObjectVisualKind::GamePlank,
        );
        self.slingshot_game.band_ids = [Some(left_band), Some(right_band)];
        let projectile = self.scene.spawn_custom_object(
            Vector2::new(anchor.x - 19.0, anchor.y - 19.0),
            Vector2::new(38.0, 38.0),
            AppColor::from_rgb(245, 72, 78),
            ObjectVisualKind::Ball,
            CollisionShape::Circle,
        );
        self.slingshot_game.projectile_id = Some(projectile);
        self.selected_id = Some(projectile);
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == projectile) {
            object.body.is_dragging = true;
            object.is_dragging = true;
            object.body.restitution = 0.42;
            object.body.friction = 0.58;
            object.body.linear_damping = 0.996;
        }
        self.update_slingshot_bands();

        let floor = bounds.bottom() - 34.0;
        let base_x = (bounds.width * 0.66).max(anchor.x + 340.0);
        let block = AppColor::from_rgb(165, 116, 75);
        let glass = AppColor::from_rgb(105, 218, 236);
        let target = AppColor::from_rgb(115, 220, 105);

        for tower in 0..2 {
            let x = base_x + tower as f32 * 175.0;
            for row in 0..3 {
                let y = floor - 62.0 - row as f32 * 86.0;
                self.spawn_dynamic_game_object(
                    Vector2::new(x, y),
                    Vector2::new(26.0, 62.0),
                    block,
                    ObjectVisualKind::GamePlank,
                    CollisionShape::Box,
                    1.15,
                    0.12,
                );
                self.spawn_dynamic_game_object(
                    Vector2::new(x + 92.0, y),
                    Vector2::new(26.0, 62.0),
                    block,
                    ObjectVisualKind::GamePlank,
                    CollisionShape::Box,
                    1.15,
                    0.12,
                );
                self.spawn_dynamic_game_object(
                    Vector2::new(x + 8.0, y - 24.0),
                    Vector2::new(102.0, 20.0),
                    glass,
                    ObjectVisualKind::GamePlank,
                    CollisionShape::Box,
                    0.86,
                    0.08,
                );
            }

            let target_id = self.spawn_dynamic_game_object(
                Vector2::new(x + 39.0, floor - 104.0),
                Vector2::new(42.0, 42.0),
                target,
                ObjectVisualKind::GameTarget,
                CollisionShape::Circle,
                0.92,
                0.18,
            );
            self.slingshot_game.targets.push(TargetMarker {
                id: target_id,
                start_position: Vector2::new(x + 39.0, floor - 104.0),
            });

            self.spawn_dynamic_game_object(
                Vector2::new(x + 21.0, floor - 284.0),
                Vector2::new(78.0, 24.0),
                AppColor::from_rgb(248, 211, 84),
                ObjectVisualKind::GamePlank,
                CollisionShape::Box,
                1.05,
                0.10,
            );
        }
    }

    fn spawn_dynamic_game_object(
        &mut self,
        position: Vector2,
        size: Vector2,
        color: AppColor,
        visual_kind: ObjectVisualKind,
        shape: CollisionShape,
        friction: f32,
        restitution: f32,
    ) -> u64 {
        let id = self.scene.spawn_custom_object(position, size, color, visual_kind, shape);
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == id) {
            object.body.friction = friction;
            object.body.restitution = restitution;
            object.body.linear_damping = 0.988;
            object.body.mass = if shape == CollisionShape::Circle { 1.15 } else { 1.6 };
        }
        id
    }

    fn spawn_pinned_game_object(
        &mut self,
        position: Vector2,
        size: Vector2,
        color: AppColor,
        visual_kind: ObjectVisualKind,
    ) -> u64 {
        let id = self
            .scene
            .spawn_custom_object(position, size, color, visual_kind, CollisionShape::Box);
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == id) {
            object.body.is_dragging = true;
            object.is_dragging = true;
            object.body.gravity_scale = 0.0;
            object.body.velocity = Vector2::ZERO;
            object.body.collidable = false;
        }
        id
    }

    fn spawn_static_game_object(
        &mut self,
        position: Vector2,
        size: Vector2,
        color: AppColor,
        visual_kind: ObjectVisualKind,
    ) -> u64 {
        let id = self
            .scene
            .spawn_custom_object(position, size, color, visual_kind, CollisionShape::Box);
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == id) {
            object.body.is_dragging = true;
            object.is_dragging = true;
            object.body.gravity_scale = 0.0;
            object.body.velocity = Vector2::ZERO;
            object.body.restitution = 0.18;
            object.body.friction = 1.35;
            object.body.linear_damping = 1.0;
        }
        id
    }

    fn handle_slingshot_mouse(&mut self, is_left_down: bool) {
        let Some(projectile_id) = self.slingshot_game.projectile_id else {
            return;
        };

        if is_left_down && !self.was_left_down {
            if self.slingshot_game.ready && self.cursor_is_over_projectile(projectile_id) {
                self.slingshot_game.aiming = true;
            }
        }

        if is_left_down && self.slingshot_game.aiming {
            self.aim_slingshot_projectile(projectile_id);
        }

        if !is_left_down && self.was_left_down && self.slingshot_game.aiming {
            self.fire_slingshot_projectile(projectile_id);
        }
    }

    fn cursor_is_over_projectile(&self, projectile_id: u64) -> bool {
        self.scene
            .objects()
            .iter()
            .find(|object| object.id == projectile_id)
            .map(|object| {
                let center = Vector2::new(
                    object.body.position.x + object.body.width * 0.5,
                    object.body.position.y + object.body.height * 0.5,
                );
                (self.cursor_local - center).length_squared() <= 72.0 * 72.0
            })
            .unwrap_or(false)
    }

    fn aim_slingshot_projectile(&mut self, projectile_id: u64) {
        let anchor = self.slingshot_game.anchor;
        let pull = clamp_vector(self.cursor_local - anchor, 142.0);
        let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == projectile_id) else {
            return;
        };
        object.body.position = anchor + pull - Vector2::new(object.body.width * 0.5, object.body.height * 0.5);
        object.body.velocity = Vector2::ZERO;
        object.body.is_dragging = true;
        object.is_dragging = true;
        object.body.is_sleeping = false;
        object.angular_velocity_x = 0.0;
        object.angular_velocity_y = 0.0;
        object.angular_velocity_z = 0.0;
        self.slingshot_game.pull = pull;
        self.update_slingshot_bands();
    }

    fn fire_slingshot_projectile(&mut self, projectile_id: u64) {
        let pull = self.slingshot_game.pull;
        let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == projectile_id) else {
            return;
        };
        object.body.is_dragging = false;
        object.is_dragging = false;
        object.body.velocity = Vector2::new(-pull.x * 11.5, -pull.y * 11.5);
        object.body.restitution = 0.42;
        object.body.friction = 0.58;
        object.body.linear_damping = 0.996;
        object.angular_velocity_z = -pull.x as f64 * 0.22;
        self.slingshot_game.aiming = false;
        self.slingshot_game.ready = false;
        self.slingshot_game.shots += 1;
        self.update_slingshot_bands();
    }

    fn update_slingshot_game(&mut self) {
        if !self.slingshot_game.active {
            return;
        }

        self.slingshot_game.targets_remaining = self
            .slingshot_game
            .targets
            .iter()
            .filter(|target| self.target_still_standing(target))
            .count();

        if self.slingshot_game.targets_remaining == 0 {
            self.slingshot_game.won = true;
        }

        if !self.slingshot_game.ready && !self.slingshot_game.won && self.projectile_should_reload() {
            self.reload_slingshot_projectile();
        }
    }

    fn target_still_standing(&self, target: &TargetMarker) -> bool {
        self.scene
            .objects()
            .iter()
            .find(|object| object.id == target.id)
            .map(|object| {
                let moved = (object.body.position - target.start_position).length_squared();
                moved < 55.0 * 55.0 && object.body.position.y < self.scene_bounds().bottom() - 75.0
            })
            .unwrap_or(false)
    }

    fn projectile_should_reload(&self) -> bool {
        let Some(projectile_id) = self.slingshot_game.projectile_id else {
            return false;
        };
        let Some(object) = self.scene.objects().iter().find(|object| object.id == projectile_id) else {
            return false;
        };
        object.body.is_sleeping
            || object.body.position.x > self.scene_bounds().right() + 120.0
            || object.body.position.y > self.scene_bounds().bottom() + 120.0
            || object.body.position.x < -180.0
    }

    fn reload_slingshot_projectile(&mut self) {
        let Some(projectile_id) = self.slingshot_game.projectile_id else {
            return;
        };
        let anchor = self.slingshot_game.anchor;
        let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == projectile_id) else {
            return;
        };
        object.body.position = Vector2::new(anchor.x - object.body.width * 0.5, anchor.y - object.body.height * 0.5);
        object.body.velocity = Vector2::ZERO;
        object.body.is_dragging = true;
        object.is_dragging = true;
        object.body.is_sleeping = false;
        self.slingshot_game.ready = true;
        self.slingshot_game.pull = Vector2::ZERO;
        self.update_slingshot_bands();
    }

    fn update_slingshot_bands(&mut self) {
        let anchor = self.slingshot_game.anchor;
        let projectile_center = if self.slingshot_game.aiming || self.slingshot_game.ready {
            self
            .slingshot_game
            .projectile_id
            .and_then(|id| self.scene.objects().iter().find(|object| object.id == id))
            .map(|object| {
                Vector2::new(
                    object.body.position.x + object.body.width * 0.5,
                    object.body.position.y + object.body.height * 0.5,
                )
            })
            .unwrap_or(anchor)
        } else {
            anchor
        };
        let forks = [Vector2::new(anchor.x - 31.0, anchor.y + 8.0), Vector2::new(anchor.x + 31.0, anchor.y + 8.0)];
        for (index, band_id) in self.slingshot_game.band_ids.iter().flatten().copied().enumerate() {
            let from = forks[index.min(1)];
            let delta = projectile_center - from;
            let length = delta.length_squared().sqrt().max(8.0);
            let angle = delta.y.atan2(delta.x).to_degrees() as f64;
            if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == band_id) {
                object.body.position = Vector2::new(from.x + delta.x * 0.5 - length * 0.5, from.y + delta.y * 0.5 - 3.0);
                object.body.width = length;
                object.body.height = 6.0;
                object.rotation_z = angle;
            }
        }
    }

    fn end_drag_and_apply_spin(&mut self, now_seconds: f64) {
        let rotated_while_held = self.is_rotation_dragging;
        let throw_sensitivity = self.scene.config().throw_sensitivity;
        let max_throw_speed = self.scene.config().max_throw_speed;
        let throw_velocity = self.drag_controller.end_drag(
            self.scene.objects_mut(),
            now_seconds,
            throw_sensitivity,
            max_throw_speed,
        );

        let Some(selected_id) = self.selected_id else {
            return;
        };
        let Some(object) = self
            .scene
            .objects_mut()
            .iter_mut()
            .find(|object| object.id == selected_id)
        else {
            return;
        };

        if rotated_while_held {
            object.angular_velocity_y += throw_velocity.x as f64 * 0.18;
            object.angular_velocity_x += throw_velocity.y as f64 * 0.12;
            object.angular_velocity_z += throw_velocity.x as f64 * 0.06;
        } else {
            object.angular_velocity_x = 0.0;
            object.angular_velocity_y = 0.0;
            object.angular_velocity_z = 0.0;
        }
        self.is_rotation_dragging = false;
    }

    fn update_held_object_rotation(&mut self, is_right_down: bool) {
        let Some(selected_id) = self.selected_id else {
            self.is_rotation_dragging = false;
            return;
        };

        let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == selected_id) else {
            self.is_rotation_dragging = false;
            return;
        };

        if self.drag_controller.dragged_id() != Some(selected_id) || !is_right_down {
            object.angular_velocity_x = 0.0;
            object.angular_velocity_y = 0.0;
            object.angular_velocity_z = 0.0;
            self.is_rotation_dragging = false;
            return;
        }

        if !self.is_rotation_dragging {
            self.is_rotation_dragging = true;
            self.last_rotation_cursor = self.cursor_local;
            return;
        }

        let delta = self.cursor_local - self.last_rotation_cursor;
        self.last_rotation_cursor = self.cursor_local;
        if delta.length_squared() <= 0.01 {
            return;
        }

        let size = object.body.width.max(object.body.height).max(1.0) as f64;
        let center = Vector2::new(
            object.body.position.x + object.body.width * 0.5,
            object.body.position.y + object.body.height * 0.5,
        );
        let grab = self.cursor_local - center;
        let torque = ((grab.x * delta.y) - (grab.y * delta.x)) as f64 / size;
        let tumble = 1.18;
        let roll = 0.32;

        object.rotation_y += (delta.x as f64 / size) * tumble;
        object.rotation_x += (delta.y as f64 / size) * tumble;
        object.rotation_z += torque * roll;
        object.angular_velocity_y = (delta.x as f64 / size) * tumble * 48.0;
        object.angular_velocity_x = (delta.y as f64 / size) * tumble * 48.0;
        object.angular_velocity_z = torque * roll * 56.0;
        self.last_drag_attempt = format!("rotate:{:.1},{:.1}", delta.x, delta.y);
    }

    fn sync_panels(&mut self) {
        let mut panels = Vec::new();
        panels.push(OverlayPanel {
            title: "Perf".to_string(),
            lines: vec![
                PanelLine {
                    text: format!("FPS: {:.0}", self.fps_counter.fps()),
                    selected: false,
                },
                PanelLine {
                    text: format!("Objects: {}", self.scene.objects().len()),
                    selected: false,
                },
                PanelLine {
                    text: format!("F9: +{} cubes", STRESS_SPAWN_COUNT),
                    selected: false,
                },
                PanelLine {
                    text: "F11: robot buddy".to_string(),
                    selected: false,
                },
            ],
            footer: Vec::new(),
        });

        if self.debug_visible {
            let lines = vec![
                PanelLine {
                    text: format!("ForceInteractive(F5): {}", if self.force_interactive_for_debug { "ON" } else { "off" }),
                    selected: false,
                },
                PanelLine {
                    text: format!(
                        "Cursor: ({:.1},{:.1}) hit={}",
                        self.cursor_local.x,
                        self.cursor_local.y,
                        if self.debug_hit_primary_cursor { "yes" } else { "no" }
                    ),
                    selected: false,
                },
                PanelLine {
                    text: format!("LMB: {} WasDown: {}", if self.debug_left_down { "down" } else { "up" }, self.was_left_down),
                    selected: false,
                },
                PanelLine {
                    text: format!("RMB: {} Rotating: {}", if self.debug_right_down { "down" } else { "up" }, self.is_rotation_dragging),
                    selected: false,
                },
                PanelLine {
                    text: format!("DragAttempt: {}", self.last_drag_attempt),
                    selected: false,
                },
                PanelLine {
                    text: format!("PendingMode: {:?}", self.pending_mode),
                    selected: false,
                },
            ];
            panels.push(OverlayPanel {
                title: "Debug".to_string(),
                lines,
                footer: Vec::new(),
            });
        }

        if self.slingshot_game.active {
            panels.push(OverlayPanel {
                title: "Slingshot".to_string(),
                lines: vec![
                    PanelLine {
                        text: format!("Shots: {}", self.slingshot_game.shots),
                        selected: false,
                    },
                    PanelLine {
                        text: format!("Targets: {}", self.slingshot_game.targets_remaining),
                        selected: false,
                    },
                    PanelLine {
                        text: if self.slingshot_game.won {
                            "Cleared! F3 resets.".to_string()
                        } else if self.slingshot_game.ready {
                            "Pull orb back, release.".to_string()
                        } else {
                            "Shot in flight.".to_string()
                        },
                        selected: self.slingshot_game.aiming,
                    },
                ],
                footer: vec!["F3 reset. F10 exit.".to_string()],
            });
        }

        if self.settings_panel.visible {
            panels.push(self.settings_panel.to_panel(self.scene.config()));
        }

        if let Some(import_panel) = &self.import_panel {
            panels.push(import_panel.to_panel());
        }

        self.hud = HudState {
            status_message: self.status_message.clone(),
            panels,
        };
    }

    fn render(&mut self) -> Result<()> {
        let scene_bounds = self.scene_bounds();
        let size = self
            .window
            .as_ref()
            .map(|window| window.inner_size())
            .unwrap_or(winit::dpi::PhysicalSize::new(self.bounds.width as u32, self.bounds.height as u32));
        let scene = RenderScene {
            bounds: scene_bounds,
            elapsed_seconds: self.frame_clock.elapsed_seconds,
            objects: self.scene.objects(),
            cursor: self.cursor_local,
            hud: &self.hud,
        };
        let vertices = self.renderer.build_vertices(size.width, size.height, &scene)?;

        #[cfg(target_os = "windows")]
        {
            let Some(d3d_renderer) = &mut self.d3d_renderer else {
                return Ok(());
            };
            return d3d_renderer.render(&vertices);
        }

        #[cfg(not(target_os = "windows"))]
        {
            let Some(gpu) = &mut self.gpu else {
                return Ok(());
            };
            gpu.render(&vertices)
        }
    }

    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        self.bounds.width = size.width as f32;
        self.bounds.height = size.height as f32;
        if let Some(gpu) = &mut self.gpu {
            gpu.resize(size.width, size.height);
        }
        #[cfg(target_os = "windows")]
        if let Some(d3d_renderer) = &mut self.d3d_renderer {
            d3d_renderer.resize(size.width, size.height);
        }
    }

    fn push_status_message(&mut self, message: String) {
        self.status_message = Some(match self.status_message.take() {
            Some(existing) => format!("{existing} {message}"),
            None => message,
        });
    }

    fn handle_action(&mut self, action: AppAction, event_loop: &ActiveEventLoop) {
        match action {
            AppAction::ToggleDebug => {
                self.debug_visible = !self.debug_visible;
            },
            AppAction::SpawnObject => {
                let id = self.scene.spawn_next_object(self.default_spawn_position());
                self.selected_id = Some(id);
            },
            AppAction::SpawnCrystal => {
                let id = self.scene.spawn_random_crystal(self.default_spawn_position());
                self.selected_id = Some(id);
            },
            AppAction::SpawnDvdLogo => {
                let id = self.scene.spawn_random_dvd_logo(self.default_spawn_position());
                self.selected_id = Some(id);
            },
            AppAction::SpawnStressCubes => {
                self.spawn_stress_cubes();
            },
            AppAction::SpawnRobotBuddy => {
                self.spawn_robot_buddy();
            },
            AppAction::ToggleSlingshotGame => {
                self.toggle_slingshot_game();
            },
            AppAction::Reset => {
                if self.slingshot_game.active {
                    self.start_slingshot_level();
                } else {
                    self.scene.reset(self.scene_bounds());
                    self.selected_id = self.scene.objects().last().map(|object| object.id);
                }
            },
            AppAction::ToggleSettings => {
                self.settings_panel.visible = !self.settings_panel.visible;
                if self.settings_panel.visible {
                    self.import_panel = None;
                }
            },
            AppAction::ToggleForceInteractive => {
                self.force_interactive_for_debug = !self.force_interactive_for_debug;
                self.last_drag_attempt = if self.force_interactive_for_debug {
                    "force-input:on".to_string()
                } else {
                    "force-input:off".to_string()
                };
            },
            AppAction::RequestImport => {
                if let Some(path) = pick_model_file() {
                    self.import_panel = Some(ImportPanel::new(path.display().to_string()));
                    self.settings_panel.visible = false;
                }
            },
            AppAction::Exit => event_loop.exit(),
        }
    }

    fn handle_keyboard(&mut self, key_code: KeyCode, state: ElementState, event_loop: &ActiveEventLoop) {
        if state != ElementState::Pressed {
            return;
        }

        if self.import_panel.is_some() && self.handle_import_key(key_code) {
            return;
        }

        if self.settings_panel.visible {
            if self.settings_panel.handle_key(key_code, self.scene.config_mut()) {
                self.scene.apply_runtime_physics_config();
                return;
            }
            if key_code == KeyCode::Escape || key_code == KeyCode::Enter {
                self.settings_panel.visible = false;
                return;
            }
        }

        match key_code {
            KeyCode::F1 => self.handle_action(AppAction::ToggleDebug, event_loop),
            KeyCode::F2 => self.handle_action(AppAction::SpawnObject, event_loop),
            KeyCode::F3 => self.handle_action(AppAction::Reset, event_loop),
            KeyCode::F4 => self.handle_action(AppAction::ToggleSettings, event_loop),
            KeyCode::F5 => self.handle_action(AppAction::ToggleForceInteractive, event_loop),
            KeyCode::F6 => self.handle_action(AppAction::RequestImport, event_loop),
            KeyCode::F7 => self.handle_action(AppAction::SpawnCrystal, event_loop),
            KeyCode::F8 => self.handle_action(AppAction::SpawnDvdLogo, event_loop),
            KeyCode::F9 => self.handle_action(AppAction::SpawnStressCubes, event_loop),
            KeyCode::F10 => self.handle_action(AppAction::ToggleSlingshotGame, event_loop),
            KeyCode::F11 => self.handle_action(AppAction::SpawnRobotBuddy, event_loop),
            KeyCode::Escape => self.handle_action(AppAction::Exit, event_loop),
            _ => {},
        }
    }

    fn handle_import_key(&mut self, key_code: KeyCode) -> bool {
        if let Some(import_panel) = &mut self.import_panel {
            if import_panel.handle_key(key_code) {
                return true;
            }
        }

        match key_code {
            KeyCode::Enter => {
                let default_spawn = self.default_spawn_position();
                let panel = self.import_panel.take().expect("import panel should exist");
                let id = self.scene.spawn_imported_model(
                    default_spawn,
                    panel.path.clone(),
                    panel.scale_multiplier,
                    AppColor::from_rgb(panel.tint_r, panel.tint_g, panel.tint_b),
                );
                self.selected_id = Some(id);
                self.status_message = Some(format!("Imported model: {}", panel.path));
                true
            },
            KeyCode::Escape => {
                self.import_panel = None;
                true
            },
            _ => false,
        }
    }

    fn default_spawn_position(&self) -> Vector2 {
        let bounds = self.scene_bounds();
        Vector2::new(bounds.width * 0.5, 40.0)
    }
}

fn monitor_bounds(event_loop: &ActiveEventLoop) -> Option<RectF> {
    let monitor = event_loop.primary_monitor()?;
    let position = monitor.position();
    let size = monitor.size();
    Some(RectF::new(
        position.x as f32,
        position.y as f32,
        size.width as f32,
        size.height as f32,
    ))
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
struct GpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    backend_label: String,
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
impl GpuState {
    async fn new(window: Arc<Window>, width: u32, height: u32) -> Result<Self> {
        let backends = if cfg!(target_os = "macos") {
            wgpu::Backends::METAL
        } else if cfg!(target_os = "windows") {
            wgpu::Backends::VULKAN
        } else {
            wgpu::Backends::VULKAN | wgpu::Backends::GL
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let surface = instance.create_surface(window).context("Failed to create WGPU surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .context("Failed to find a GPU adapter")?;

        let adapter_info = adapter.get_info();
        if cfg!(target_os = "macos") && adapter_info.backend != wgpu::Backend::Metal {
            anyhow::bail!("Expected Metal backend on macOS, got {:?}", adapter_info.backend);
        }
        if cfg!(target_os = "windows") && adapter_info.backend != wgpu::Backend::Vulkan {
            anyhow::bail!("Expected Vulkan backend on Windows, got {:?}", adapter_info.backend);
        }

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("screen-overlay-physics-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .context("Failed to request WGPU device")?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        let alpha_mode = if cfg!(target_os = "windows") && caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            caps.alpha_modes[0]
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);
        let (depth_texture, depth_view) = create_depth_buffer(&device, config.width, config.height);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("screen-overlay-physics-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                r#"
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct RenderUniforms {
    viewport_size: vec2<f32>,
    camera_distance: f32,
    padding: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: RenderUniforms;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let center = uniforms.viewport_size * 0.5;
    var screen = input.position.xy;
    var clip_z = 0.0;
    if (input.position.z < 800.0) {
        let denominator = max(uniforms.camera_distance - input.position.z, 1.0);
        let perspective = uniforms.camera_distance / denominator;
        screen = center + ((input.position.xy - center) * perspective);
        clip_z = clamp(0.5 - (input.position.z / (uniforms.camera_distance * 2.0)), 0.0, 1.0);
    }
    let clip_xy = vec2<f32>(
        (screen.x / uniforms.viewport_size.x) * 2.0 - 1.0,
        1.0 - ((screen.y / uniforms.viewport_size.y) * 2.0)
    );
    out.position = vec4<f32>(clip_xy, clip_z, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#,
            )),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen-overlay-physics-uniforms"),
            size: std::mem::size_of::<RenderUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("screen-overlay-physics-uniform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen-overlay-physics-uniform-bind-group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("screen-overlay-physics-pipeline-layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("screen-overlay-physics-pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let vertex_capacity = 4096usize;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen-overlay-physics-vertices"),
            size: (vertex_capacity * std::mem::size_of::<GpuVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            depth_texture,
            depth_view,
            uniform_buffer,
            uniform_bind_group,
            vertex_buffer,
            vertex_capacity,
            backend_label: format!("{:?}", adapter_info.backend),
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        (self.depth_texture, self.depth_view) =
            create_depth_buffer(&self.device, self.config.width, self.config.height);
    }

    fn render(&mut self, vertices: &[GpuVertex]) -> Result<()> {
        if self.config.width == 0 || self.config.height == 0 {
            return Ok(());
        }

        self.ensure_vertex_capacity(vertices.len());
        let uniforms = RenderUniforms::new(self.config.width, self.config.height);
        self.queue
            .write_buffer(&self.uniform_buffer, 0, render_uniforms_as_bytes(&uniforms));
        if !vertices.is_empty() {
            self.queue
                .write_buffer(&self.vertex_buffer, 0, gpu_vertices_as_bytes(vertices));
        }

        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface
                    .get_current_texture()
                    .context("Failed to reacquire transparent surface")?
            },
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(wgpu::SurfaceError::OutOfMemory) => anyhow::bail!("GPU surface ran out of memory"),
        };

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("screen-overlay-physics-encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("screen-overlay-physics-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(overlay_clear_color()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            if !vertices.is_empty() {
                render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                render_pass.draw(0..vertices.len() as u32, 0..1);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    fn ensure_vertex_capacity(&mut self, needed_vertices: usize) {
        if needed_vertices <= self.vertex_capacity {
            return;
        }

        self.vertex_capacity = needed_vertices.next_power_of_two().max(4096);
        self.vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen-overlay-physics-vertices"),
            size: (self.vertex_capacity * std::mem::size_of::<GpuVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
fn create_depth_buffer(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screen-overlay-physics-depth"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
    (depth_texture, depth_view)
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
fn overlay_clear_color() -> wgpu::Color {
    wgpu::Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: if cfg!(target_os = "windows") { 1.0 } else { 0.0 },
    }
}

#[cfg(target_os = "windows")]
struct WindowsD3dRenderer {
    hwnd: HWND,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    dxgi_device: IDXGIDevice,
    factory: IDXGIFactory2,
    dcomp_device: IDCompositionDevice,
    dcomp_target: Option<IDCompositionTarget>,
    dcomp_visual: Option<IDCompositionVisual>,
    swap_chain: Option<IDXGISwapChain1>,
    render_target: Option<ID3D11RenderTargetView>,
    depth_view: Option<ID3D11DepthStencilView>,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    input_layout: ID3D11InputLayout,
    blend_state: ID3D11BlendState,
    depth_stencil_state: ID3D11DepthStencilState,
    rasterizer_state: ID3D11RasterizerState,
    uniform_buffer: ID3D11Buffer,
    vertex_buffer: Option<ID3D11Buffer>,
    vertex_capacity: usize,
    width: u32,
    height: u32,
}

#[cfg(target_os = "windows")]
impl WindowsD3dRenderer {
    fn new(bounds: RectF) -> Result<Self> {
        let hwnd = create_dcomp_overlay_window(bounds).context("Failed to create DirectComposition overlay window")?;
        let (device, context) = create_d3d_device().context("Failed to create Direct3D 11 device")?;
        let dxgi_device: IDXGIDevice = device.cast().context("Failed to query IDXGIDevice")?;
        let factory: IDXGIFactory2 = unsafe { CreateDXGIFactory1().context("Failed to create DXGI factory")? };
        let dcomp_device: IDCompositionDevice =
            unsafe { DCompositionCreateDevice(&dxgi_device).context("Failed to create DirectComposition device")? };
        let (vertex_shader, pixel_shader, input_layout, blend_state, depth_stencil_state, rasterizer_state) =
            create_d3d_pipeline(&device).context("Failed to create Direct3D pipeline")?;
        let uniform_buffer = create_d3d_uniform_buffer(&device).context("Failed to create Direct3D uniform buffer")?;

        let mut renderer = Self {
            hwnd,
            device,
            context,
            dxgi_device,
            factory,
            dcomp_device,
            dcomp_target: None,
            dcomp_visual: None,
            swap_chain: None,
            render_target: None,
            depth_view: None,
            vertex_shader,
            pixel_shader,
            input_layout,
            blend_state,
            depth_stencil_state,
            rasterizer_state,
            uniform_buffer,
            vertex_buffer: None,
            vertex_capacity: 0,
            width: 0,
            height: 0,
        };
        renderer.resize(
            bounds.width.max(1.0).round() as u32,
            bounds.height.max(1.0).round() as u32,
        );
        Ok(renderer)
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        if let Err(error) = self.ensure_target(width, height) {
            self.swap_chain = None;
            self.render_target = None;
            self.depth_view = None;
            self.width = 0;
            self.height = 0;
            eprintln!("Direct3D target resize failed: {error:#}");
        }
    }

    fn render(&mut self, vertices: &[GpuVertex]) -> Result<()> {
        self.ensure_target(self.width.max(1), self.height.max(1))?;
        self.ensure_vertex_buffer(vertices.len())?;
        self.upload_uniforms()?;
        self.upload_vertices(vertices)?;
        self.bind_pipeline();

        let clear = [0.0f32, 0.0, 0.0, 0.0];
        unsafe {
            self.context
                .ClearRenderTargetView(self.render_target.as_ref().context("Missing D3D render target")?, &clear);
            self.context.ClearDepthStencilView(
                self.depth_view.as_ref().context("Missing D3D depth view")?,
                D3D11_CLEAR_DEPTH.0 as u32,
                1.0,
                0,
            );
            if !vertices.is_empty() {
                self.context.Draw(vertices.len() as u32, 0);
            }
        }

        let Some(swap_chain) = &self.swap_chain else {
            anyhow::bail!("Missing D3D swap chain");
        };
        let present = unsafe { swap_chain.Present(0, 0) };
        if present == HRESULT(0x087A0001u32 as i32) {
            return Ok(());
        }
        present.ok().context("Direct3D swap chain present failed")
    }

    fn set_input_mode(&self, mode: OverlayInputMode) -> Result<()> {
        let transparent_bit = WS_EX_TRANSPARENT.0 as isize;
        unsafe {
            let ex_style = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let next_ex_style = if mode == OverlayInputMode::PassThrough {
                ex_style | transparent_bit
            } else {
                ex_style & !transparent_bit
            };

            if next_ex_style != ex_style {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, next_ex_style);
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                )
                .context("Failed to update DirectComposition overlay input mode")?;
            }
        }

        Ok(())
    }

    fn ensure_target(&mut self, width: u32, height: u32) -> Result<()> {
        if self.swap_chain.is_none() {
            self.create_swap_chain(width, height)?;
            return Ok(());
        }

        if self.width == width && self.height == height && self.render_target.is_some() {
            return Ok(());
        }

        self.render_target = None;
        self.depth_view = None;
        let swap_chain = self.swap_chain.as_ref().context("Missing D3D swap chain")?;
        unsafe {
            swap_chain.ResizeBuffers(0, width, height, DXGI_FORMAT(0), 0)?;
        }
        self.create_render_target(width, height)
    }

    fn create_swap_chain(&mut self, width: u32, height: u32) -> Result<()> {
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            Stereo: BOOL(0),
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            Flags: 0,
        };

        let swap_chain = unsafe {
            self.factory
                .CreateSwapChainForComposition(&self.dxgi_device, &desc, None)
                .context("Failed to create DirectComposition swap chain")?
        };
        let target = unsafe {
            self.dcomp_device
                .CreateTargetForHwnd(self.hwnd, true)
                .context("Failed to create DirectComposition target")?
        };
        let visual = unsafe {
            self.dcomp_device
                .CreateVisual()
                .context("Failed to create DirectComposition visual")?
        };
        unsafe {
            visual.SetContent(&swap_chain)?;
            target.SetRoot(&visual)?;
            self.dcomp_device.Commit()?;
        }

        self.swap_chain = Some(swap_chain);
        self.dcomp_target = Some(target);
        self.dcomp_visual = Some(visual);
        self.create_render_target(width, height)
    }

    fn create_render_target(&mut self, width: u32, height: u32) -> Result<()> {
        let swap_chain = self.swap_chain.as_ref().context("Missing D3D swap chain")?;
        let back_buffer: ID3D11Texture2D = unsafe { swap_chain.GetBuffer(0).context("Failed to get swap chain buffer")? };
        let mut render_target = None;
        unsafe {
            self.device
                .CreateRenderTargetView(&back_buffer, None, Some(&mut render_target))
                .context("Failed to create render target view")?;
        }
        self.render_target = Some(render_target.context("CreateRenderTargetView returned no target")?);
        self.depth_view = Some(create_d3d_depth_view(&self.device, width, height)?);
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn ensure_vertex_buffer(&mut self, needed_vertices: usize) -> Result<()> {
        if needed_vertices <= self.vertex_capacity && self.vertex_buffer.is_some() {
            return Ok(());
        }

        let mut capacity = self.vertex_capacity.max(4096);
        while capacity < needed_vertices {
            capacity *= 2;
        }
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: (capacity * std::mem::size_of::<GpuVertex>()) as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            ..Default::default()
        };
        let mut buffer = None;
        unsafe {
            self.device
                .CreateBuffer(&desc, None, Some(&mut buffer))
                .context("Failed to create D3D vertex buffer")?;
        }
        self.vertex_buffer = Some(buffer.context("CreateBuffer returned no vertex buffer")?);
        self.vertex_capacity = capacity;
        Ok(())
    }

    fn upload_uniforms(&self) -> Result<()> {
        let uniforms = RenderUniforms::new(self.width, self.height);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(&self.uniform_buffer, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))
                .context("Failed to map D3D uniform buffer")?;
            std::ptr::copy_nonoverlapping(
                (&uniforms as *const RenderUniforms).cast::<u8>(),
                mapped.pData.cast::<u8>(),
                std::mem::size_of::<RenderUniforms>(),
            );
            self.context.Unmap(&self.uniform_buffer, 0);
        }
        Ok(())
    }

    fn upload_vertices(&self, vertices: &[GpuVertex]) -> Result<()> {
        if vertices.is_empty() {
            return Ok(());
        }

        let buffer = self.vertex_buffer.as_ref().context("Missing D3D vertex buffer")?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            self.context
                .Map(buffer, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))
                .context("Failed to map D3D vertex buffer")?;
            std::ptr::copy_nonoverlapping(
                vertices.as_ptr(),
                mapped.pData.cast::<GpuVertex>(),
                vertices.len(),
            );
            self.context.Unmap(buffer, 0);
        }
        Ok(())
    }

    fn bind_pipeline(&self) {
        let stride = std::mem::size_of::<GpuVertex>() as u32;
        let offset = 0u32;
        let viewport = D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: self.width as f32,
            Height: self.height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        let blend_factor = [0.0f32, 0.0, 0.0, 0.0];
        let render_target = self.render_target.clone();
        let depth_view = self.depth_view.clone();
        let vertex_buffers = [self.vertex_buffer.clone()];
        let strides = [stride];
        let offsets = [offset];
        let constant_buffers = [Some(self.uniform_buffer.clone())];

        unsafe {
            self.context.OMSetRenderTargets(Some(&[render_target]), depth_view.as_ref());
            self.context.RSSetViewports(Some(&[viewport]));
            self.context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.IASetInputLayout(&self.input_layout);
            self.context.IASetVertexBuffers(
                0,
                vertex_buffers.len() as u32,
                Some(vertex_buffers.as_ptr()),
                Some(strides.as_ptr()),
                Some(offsets.as_ptr()),
            );
            self.context.VSSetShader(&self.vertex_shader, None);
            self.context.VSSetConstantBuffers(0, Some(&constant_buffers));
            self.context.PSSetShader(&self.pixel_shader, None);
            self.context
                .OMSetBlendState(&self.blend_state, Some(&blend_factor), u32::MAX);
            self.context
                .OMSetDepthStencilState(&self.depth_stencil_state, 0);
            self.context.RSSetState(&self.rasterizer_state);
        }
    }
}

#[cfg(target_os = "windows")]
fn create_dcomp_overlay_window(bounds: RectF) -> Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None).context("GetModuleHandleW failed")?;
        let class_name = w!("ScreenOverlayPhysicsDcompOverlay");
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(dcomp_overlay_wnd_proc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&window_class);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(
                WS_EX_LAYERED.0
                    | WS_EX_NOACTIVATE.0
                    | WS_EX_TRANSPARENT.0
                    | WS_EX_TOPMOST.0
                    | WS_EX_TOOLWINDOW.0,
            ),
            class_name,
            w!("ScreenOverlayPhysics DirectComposition"),
            WS_POPUP,
            bounds.x.round() as i32,
            bounds.y.round() as i32,
            bounds.width.max(1.0).round() as i32,
            bounds.height.max(1.0).round() as i32,
            None,
            None,
            instance,
            None,
        );
        if hwnd.0 == 0 {
            anyhow::bail!("CreateWindowExW failed for DirectComposition overlay");
        }
        ShowWindow(hwnd, SW_SHOWNA);
        Ok(hwnd)
    }
}

#[cfg(target_os = "windows")]
extern "system" fn dcomp_overlay_wnd_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

#[cfg(target_os = "windows")]
fn create_d3d_device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    unsafe {
        let mut device = None;
        let mut context = None;
        let mut feature_level = D3D_FEATURE_LEVEL::default();
        let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_SINGLETHREADED;
        let hardware = D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE(0),
            flags,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut feature_level),
            Some(&mut context),
        );
        if hardware.is_err() {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                HMODULE(0),
                flags,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut feature_level),
                Some(&mut context),
            )?;
        }
        Ok((
            device.context("D3D11CreateDevice returned no device")?,
            context.context("D3D11CreateDevice returned no context")?,
        ))
    }
}

#[cfg(target_os = "windows")]
fn create_d3d_uniform_buffer(device: &ID3D11Device) -> Result<ID3D11Buffer> {
    let desc = D3D11_BUFFER_DESC {
        ByteWidth: std::mem::size_of::<RenderUniforms>() as u32,
        Usage: D3D11_USAGE_DYNAMIC,
        BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
        CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
        ..Default::default()
    };
    let mut buffer = None;
    unsafe {
        device
            .CreateBuffer(&desc, None, Some(&mut buffer))
            .context("Failed to create D3D uniform buffer")?;
    }
    buffer.context("CreateBuffer returned no uniform buffer")
}

#[cfg(target_os = "windows")]
fn create_d3d_depth_view(device: &ID3D11Device, width: u32, height: u32) -> Result<ID3D11DepthStencilView> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width.max(1),
        Height: height.max(1),
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_D32_FLOAT,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_DEPTH_STENCIL.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut texture))
            .context("Failed to create D3D depth texture")?;
    }
    let texture = texture.context("CreateTexture2D returned no depth texture")?;
    let mut view = None;
    unsafe {
        device
            .CreateDepthStencilView(&texture, None, Some(&mut view))
            .context("Failed to create D3D depth view")?;
    }
    view.context("CreateDepthStencilView returned no depth view")
}

#[cfg(target_os = "windows")]
fn create_d3d_pipeline(
    device: &ID3D11Device,
) -> Result<(
    ID3D11VertexShader,
    ID3D11PixelShader,
    ID3D11InputLayout,
    ID3D11BlendState,
    ID3D11DepthStencilState,
    ID3D11RasterizerState,
)> {
    let shader_source = br#"
cbuffer RenderUniforms : register(b0) {
    float2 viewport_size;
    float camera_distance;
    float padding;
};

struct VSInput {
    float3 position : POSITION;
    float4 color : COLOR0;
};
struct PSInput {
    float4 position : SV_POSITION;
    float4 color : COLOR0;
};
PSInput vs_main(VSInput input) {
    PSInput output;
    float2 center = viewport_size * 0.5f;
    float2 screen = input.position.xy;
    float clip_z = 0.0f;
    if (input.position.z < 800.0f) {
        float denominator = max(camera_distance - input.position.z, 1.0f);
        float perspective = camera_distance / denominator;
        screen = center + ((input.position.xy - center) * perspective);
        clip_z = saturate(0.5f - (input.position.z / (camera_distance * 2.0f)));
    }
    float2 clip_xy = float2(
        (screen.x / viewport_size.x) * 2.0f - 1.0f,
        1.0f - ((screen.y / viewport_size.y) * 2.0f)
    );
    output.position = float4(clip_xy, clip_z, 1.0f);
    output.color = input.color;
    return output;
}
float4 ps_main(PSInput input) : SV_TARGET {
    return float4(input.color.rgb * input.color.a, input.color.a);
}
"#;
    let vertex_blob = compile_shader(shader_source, "vs_main", "vs_4_0")?;
    let pixel_blob = compile_shader(shader_source, "ps_main", "ps_4_0")?;

    unsafe {
        let vertex_bytecode = std::slice::from_raw_parts(
            vertex_blob.GetBufferPointer().cast::<u8>(),
            vertex_blob.GetBufferSize(),
        );
        let pixel_bytecode = std::slice::from_raw_parts(
            pixel_blob.GetBufferPointer().cast::<u8>(),
            pixel_blob.GetBufferSize(),
        );

        let mut vertex_shader = None;
        device.CreateVertexShader(vertex_bytecode, None, Some(&mut vertex_shader))?;
        let mut pixel_shader = None;
        device.CreatePixelShader(pixel_bytecode, None, Some(&mut pixel_shader))?;

        let position_name = CString::new("POSITION")?;
        let color_name = CString::new("COLOR")?;
        let elements = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: PCSTR(position_name.as_ptr().cast()),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: PCSTR(color_name.as_ptr().cast()),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 12,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];
        let mut input_layout = None;
        device.CreateInputLayout(&elements, vertex_bytecode, Some(&mut input_layout))?;

        let blend_desc = D3D11_BLEND_DESC {
            AlphaToCoverageEnable: BOOL(0),
            IndependentBlendEnable: BOOL(0),
            RenderTarget: [
                D3D11_RENDER_TARGET_BLEND_DESC {
                    BlendEnable: BOOL(1),
                    SrcBlend: D3D11_BLEND_ONE,
                    DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                    BlendOp: D3D11_BLEND_OP_ADD,
                    SrcBlendAlpha: D3D11_BLEND_ONE,
                    DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                    BlendOpAlpha: D3D11_BLEND_OP_ADD,
                    RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
                },
                D3D11_RENDER_TARGET_BLEND_DESC::default(),
                D3D11_RENDER_TARGET_BLEND_DESC::default(),
                D3D11_RENDER_TARGET_BLEND_DESC::default(),
                D3D11_RENDER_TARGET_BLEND_DESC::default(),
                D3D11_RENDER_TARGET_BLEND_DESC::default(),
                D3D11_RENDER_TARGET_BLEND_DESC::default(),
                D3D11_RENDER_TARGET_BLEND_DESC::default(),
            ],
        };
        let mut blend_state = None;
        device.CreateBlendState(&blend_desc, Some(&mut blend_state))?;

        let depth_desc = D3D11_DEPTH_STENCIL_DESC {
            DepthEnable: BOOL(1),
            DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ALL,
            DepthFunc: D3D11_COMPARISON_LESS_EQUAL,
            StencilEnable: BOOL(0),
            ..Default::default()
        };
        let mut depth_stencil_state = None;
        device.CreateDepthStencilState(&depth_desc, Some(&mut depth_stencil_state))?;

        let rasterizer_desc = D3D11_RASTERIZER_DESC {
            FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_NONE,
            FrontCounterClockwise: BOOL(0),
            DepthBias: 0,
            DepthBiasClamp: 0.0,
            SlopeScaledDepthBias: 0.0,
            DepthClipEnable: BOOL(1),
            ScissorEnable: BOOL(0),
            MultisampleEnable: BOOL(0),
            AntialiasedLineEnable: BOOL(0),
        };
        let mut rasterizer_state = None;
        device.CreateRasterizerState(&rasterizer_desc, Some(&mut rasterizer_state))?;

        Ok((
            vertex_shader.context("CreateVertexShader returned no shader")?,
            pixel_shader.context("CreatePixelShader returned no shader")?,
            input_layout.context("CreateInputLayout returned no layout")?,
            blend_state.context("CreateBlendState returned no state")?,
            depth_stencil_state.context("CreateDepthStencilState returned no state")?,
            rasterizer_state.context("CreateRasterizerState returned no state")?,
        ))
    }
}

#[cfg(target_os = "windows")]
fn compile_shader(source: &[u8], entry: &str, target: &str) -> Result<ID3DBlob> {
    let entry = CString::new(entry)?;
    let target = CString::new(target)?;
    let mut blob = None;
    let mut errors = None;
    let result = unsafe {
        D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            PCSTR::null(),
            None,
            None,
            PCSTR(entry.as_ptr().cast()),
            PCSTR(target.as_ptr().cast()),
            0,
            0,
            &mut blob,
            Some(&mut errors),
        )
    };
    if let Err(error) = result {
        if let Some(errors) = errors {
            let message = unsafe {
                let bytes = std::slice::from_raw_parts(errors.GetBufferPointer().cast::<u8>(), errors.GetBufferSize());
                String::from_utf8_lossy(bytes).into_owned()
            };
            anyhow::bail!("Shader compile failed: {message}");
        }
        return Err(error).context("Shader compile failed");
    }
    blob.context("D3DCompile returned no bytecode")
}


#[cfg_attr(target_os = "windows", allow(dead_code))]
fn gpu_vertices_as_bytes(vertices: &[GpuVertex]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            vertices.as_ptr() as *const u8,
            std::mem::size_of_val(vertices),
        )
    }
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
fn render_uniforms_as_bytes(uniforms: &RenderUniforms) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            (uniforms as *const RenderUniforms).cast::<u8>(),
            std::mem::size_of::<RenderUniforms>(),
        )
    }
}

impl ApplicationHandler for NativeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.target_frame_duration.is_zero() {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
        }
        if self.window.is_none() {
            if let Err(error) = self.create_window(event_loop) {
                show_error_dialog("Startup Error", &format!("{error:#}"));
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if self.window_id != Some(window_id) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size),
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.render() {
                    show_error_dialog("Render Error", &format!("{error:#}"));
                    event_loop.exit();
                }
            },
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.handle_keyboard(code, event.state, event_loop);
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_local = Vector2::new(position.x as f32, position.y as f32);
            },
            WindowEvent::MouseInput { state, button, .. } => {
                let is_down = state == ElementState::Pressed;
                match button {
                    MouseButton::Left => self.fallback_left_down = is_down,
                    MouseButton::Right => self.fallback_right_down = is_down,
                    _ => {},
                }
            },
            _ => {},
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if !self.target_frame_duration.is_zero() && now < self.next_frame_at {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
            return;
        }

        self.fps_counter.record_frame(now);

        if !self.target_frame_duration.is_zero() {
            self.next_frame_at = self
                .next_frame_at
                .checked_add(self.target_frame_duration)
                .unwrap_or(now + self.target_frame_duration);
            if self.next_frame_at <= now {
                self.next_frame_at = now + self.target_frame_duration;
            }
        }

        if let Some(tray) = &self.tray {
            if let Some(action) = tray.poll_action() {
                let mapped = match action {
                    TrayAction::ToggleDebug => AppAction::ToggleDebug,
                    TrayAction::SpawnObject => AppAction::SpawnObject,
                    TrayAction::SpawnCrystal => AppAction::SpawnCrystal,
                    TrayAction::SpawnDvdLogo => AppAction::SpawnDvdLogo,
                    TrayAction::SpawnStressCubes => AppAction::SpawnStressCubes,
                    TrayAction::Reset => AppAction::Reset,
                    TrayAction::ToggleSettings => AppAction::ToggleSettings,
                    TrayAction::ImportModel => AppAction::RequestImport,
                    TrayAction::Exit => AppAction::Exit,
                };
                self.handle_action(mapped, event_loop);
            }
        }

        self.update();
        #[cfg(target_os = "windows")]
        if let Err(error) = self.render() {
            show_error_dialog("Render Error", &format!("{error:#}"));
            event_loop.exit();
        }
        if self.target_frame_duration.is_zero() {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
        }
    }
}

#[derive(Clone, Copy)]
enum AppAction {
    ToggleDebug,
    SpawnObject,
    SpawnCrystal,
    SpawnDvdLogo,
    SpawnStressCubes,
    SpawnRobotBuddy,
    ToggleSlingshotGame,
    Reset,
    ToggleSettings,
    ToggleForceInteractive,
    RequestImport,
    Exit,
}

fn clamp_vector(vector: Vector2, max_length: f32) -> Vector2 {
    let length_squared = vector.length_squared();
    let max_squared = max_length * max_length;
    if length_squared <= max_squared || length_squared <= f32::EPSILON {
        return vector;
    }
    let scale = max_length / length_squared.sqrt();
    vector * scale
}

#[derive(Clone, Copy)]
struct TargetMarker {
    id: u64,
    start_position: Vector2,
}

#[derive(Clone, Copy)]
struct RobotCarry {
    object_id: u64,
    picked_up_at: f64,
}

#[derive(Clone, Copy)]
struct RobotDropCooldown {
    object_id: u64,
    dropped_at: f64,
}

#[derive(Default)]
struct SlingshotGame {
    active: bool,
    aiming: bool,
    ready: bool,
    won: bool,
    shots: u32,
    targets_remaining: usize,
    anchor: Vector2,
    pull: Vector2,
    projectile_id: Option<u64>,
    band_ids: [Option<u64>; 2],
    targets: Vec<TargetMarker>,
}

impl SlingshotGame {
    fn new(bounds: RectF) -> Self {
        Self {
            active: true,
            ready: true,
            targets_remaining: 2,
            anchor: Vector2::new((bounds.width * 0.18).clamp(95.0, 260.0), bounds.bottom() - 140.0),
            ..Self::default()
        }
    }
}

#[derive(Default)]
struct SettingsPanel {
    visible: bool,
    selected_index: usize,
}

impl SettingsPanel {
    fn from_config(_config: AppConfig) -> Self {
        Self::default()
    }

    fn to_panel(&self, config: &AppConfig) -> OverlayPanel {
        OverlayPanel {
            title: "Settings".to_string(),
            lines: vec![
                PanelLine {
                    text: format!("Gravity: {:.0}", config.gravity_y),
                    selected: self.selected_index == 0,
                },
                PanelLine {
                    text: format!("Throw Sensitivity: {:.2}", config.throw_sensitivity),
                    selected: self.selected_index == 1,
                },
                PanelLine {
                    text: format!("Max Throw Speed: {:.0}", config.max_throw_speed),
                    selected: self.selected_index == 2,
                },
                PanelLine {
                    text: format!("Restitution: {:.2}", config.restitution),
                    selected: self.selected_index == 3,
                },
                PanelLine {
                    text: format!("Linear Damping: {:.3}", config.linear_damping),
                    selected: self.selected_index == 4,
                },
                PanelLine {
                    text: format!("Sleep Threshold: {:.0}", config.sleep_threshold),
                    selected: self.selected_index == 5,
                },
                PanelLine {
                    text: format!("Floor Snap: {:.0}", config.floor_snap_threshold),
                    selected: self.selected_index == 6,
                },
                PanelLine {
                    text: format!("Debounce Ms: {}", config.interaction_debounce_ms),
                    selected: self.selected_index == 7,
                },
                PanelLine {
                    text: format!("Start Pass Through: {}", config.start_in_pass_through),
                    selected: self.selected_index == 8,
                },
            ],
            footer: vec![
                "Use Arrow keys to adjust values.".to_string(),
                "Enter/Esc closes the panel.".to_string(),
            ],
        }
    }

    fn handle_key(&mut self, key_code: KeyCode, config: &mut AppConfig) -> bool {
        const ENTRY_COUNT: usize = 9;
        match key_code {
            KeyCode::ArrowUp => {
                self.selected_index = self.selected_index.saturating_sub(1);
                true
            },
            KeyCode::ArrowDown => {
                self.selected_index = (self.selected_index + 1).min(ENTRY_COUNT - 1);
                true
            },
            KeyCode::ArrowLeft => {
                adjust_setting(config, self.selected_index, -1.0);
                true
            },
            KeyCode::ArrowRight => {
                adjust_setting(config, self.selected_index, 1.0);
                true
            },
            _ => false,
        }
    }
}

fn adjust_setting(config: &mut AppConfig, selected_index: usize, direction: f32) {
    match selected_index {
        0 => config.gravity_y = (config.gravity_y + direction * 50.0).clamp(0.0, 4000.0),
        1 => config.throw_sensitivity = (config.throw_sensitivity + direction * 0.05).clamp(0.1, 4.0),
        2 => config.max_throw_speed = (config.max_throw_speed + direction * 100.0).clamp(100.0, 6000.0),
        3 => config.restitution = (config.restitution + direction * 0.05).clamp(0.05, 1.2),
        4 => config.linear_damping = (config.linear_damping + direction * 0.002).clamp(0.900, 0.999),
        5 => config.sleep_threshold = (config.sleep_threshold + direction * 2.0).clamp(1.0, 120.0),
        6 => config.floor_snap_threshold = (config.floor_snap_threshold + direction).clamp(0.0, 24.0),
        7 => {
            let next = config.interaction_debounce_ms as i32 + (direction as i32 * 10);
            config.interaction_debounce_ms = next.clamp(0, 300) as u32;
        },
        8 => {
            if direction != 0.0 {
                config.start_in_pass_through = !config.start_in_pass_through;
            }
        },
        _ => {},
    }
}

struct ImportPanel {
    path: String,
    selected_index: usize,
    scale_multiplier: f32,
    tint_r: u8,
    tint_g: u8,
    tint_b: u8,
}

impl ImportPanel {
    fn new(path: String) -> Self {
        Self {
            path,
            selected_index: 0,
            scale_multiplier: 1.0,
            tint_r: 255,
            tint_g: 255,
            tint_b: 255,
        }
    }

    fn handle_key(&mut self, key_code: KeyCode) -> bool {
        match key_code {
            KeyCode::ArrowUp => {
                self.selected_index = self.selected_index.saturating_sub(1);
                true
            },
            KeyCode::ArrowDown => {
                self.selected_index = (self.selected_index + 1).min(3);
                true
            },
            KeyCode::ArrowLeft => {
                self.adjust(-1);
                true
            },
            KeyCode::ArrowRight => {
                self.adjust(1);
                true
            },
            _ => false,
        }
    }

    fn adjust(&mut self, direction: i32) {
        match self.selected_index {
            0 => self.scale_multiplier = (self.scale_multiplier + direction as f32 * 0.1).clamp(0.25, 3.0),
            1 => self.tint_r = (self.tint_r as i32 + direction * 8).clamp(0, 255) as u8,
            2 => self.tint_g = (self.tint_g as i32 + direction * 8).clamp(0, 255) as u8,
            3 => self.tint_b = (self.tint_b as i32 + direction * 8).clamp(0, 255) as u8,
            _ => {},
        }
    }

    fn to_panel(&self) -> OverlayPanel {
        OverlayPanel {
            title: "Import Model".to_string(),
            lines: vec![
                PanelLine {
                    text: format!("Path: {}", self.path),
                    selected: false,
                },
                PanelLine {
                    text: format!("Scale: {:.2}x", self.scale_multiplier),
                    selected: self.selected_index == 0,
                },
                PanelLine {
                    text: format!("Tint R: {}", self.tint_r),
                    selected: self.selected_index == 1,
                },
                PanelLine {
                    text: format!("Tint G: {}", self.tint_g),
                    selected: self.selected_index == 2,
                },
                PanelLine {
                    text: format!("Tint B: {}", self.tint_b),
                    selected: self.selected_index == 3,
                },
            ],
            footer: vec![
                "Arrow keys adjust options.".to_string(),
                "Enter imports. Esc cancels.".to_string(),
            ],
        }
    }
}
