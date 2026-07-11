#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::{
    borrow::Cow,
    collections::HashMap,
    env,
    error::Error,
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
use std::{ffi::CString, os::windows::process::CommandExt};

use anyhow::{Context, Result};
use core_types::{AppColor, AppConfig, CollisionShape, ObjectState, ObjectVisualKind, RectF, Vector2};
use native_shell::{
    configure_overlay_window, desktop_window_at_point, desktop_window_by_id, overlay_window_attributes,
    pick_model_file, set_overlay_input_mode, show_error_dialog, sync_window_to_bounds, DesktopWindowTarget,
    GlobalImportKeys, GlobalInputPoller, OverlayInputMode, TrayAction, TrayController,
};
use renderer::{
    hoop_geometry, CheerPortalVisual, GpuVertex, HudState, OverlayPanel, PanelLine, RenderScene, SandRenderCell,
    SceneRenderer, ScreenLabel,
};
use scene_logic::{DragController, FrameClock, HitTester, MouseTracker, SceneController};
use serde::Deserialize;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, ModifiersState, PhysicalKey},
    window::{CursorIcon, Window, WindowId},
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
            ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader,
            D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_DEPTH_STENCIL, D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_VERTEX_BUFFER,
            D3D11_BLEND_DESC, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD,
            D3D11_BUFFER_DESC, D3D11_CLEAR_DEPTH, D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_COMPARISON_LESS_EQUAL,
            D3D11_COMPARISON_NEVER,
            D3D11_CPU_ACCESS_WRITE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_SINGLETHREADED,
            D3D11_CULL_NONE, D3D11_DEPTH_STENCIL_DESC, D3D11_DEPTH_WRITE_MASK_ALL, D3D11_FILL_SOLID,
            D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA, D3D11_MAP_WRITE_DISCARD,
            D3D11_MAPPED_SUBRESOURCE, D3D11_RASTERIZER_DESC, D3D11_RENDER_TARGET_BLEND_DESC,
            D3D11_SAMPLER_DESC, D3D11_SDK_VERSION, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC,
            D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC, D3D11_USAGE_IMMUTABLE,
            D3D11_VIEWPORT,
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
        Gdi::{
            BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC,
            SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
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

#[derive(Clone, Debug)]
struct WindowCapture {
    window_id: isize,
    title: String,
    client_rect_screen: RectF,
    object_collidable: bool,
}

#[derive(Clone, Debug)]
struct CheerDropEffect {
    donor: String,
    bits: u32,
    center: Vector2,
    color: AppColor,
    started_at: f64,
    ends_at: f64,
    anonymous: bool,
}

#[derive(Clone, Debug)]
struct PendingCheerDrop {
    donor: String,
    bits: u32,
    center: Vector2,
    color: AppColor,
    count: usize,
    emitted: usize,
    size: f32,
    tier: u32,
    excitement: f32,
    mass_per_crystal: f32,
    base_value: u32,
    remainder: u32,
    started_at: f64,
    emission_seconds: f64,
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
    control_ui_process: Option<Child>,
    control_command_rx: Option<Receiver<ControlIpcCommand>>,
    physics_paused: bool,
    bounds: RectF,
    spawn_monitor_bounds: Option<RectF>,
    overlay_mode: OverlayInputMode,
    pending_mode: OverlayInputMode,
    mode_candidate_since_seconds: f64,
    cursor_local: Vector2,
    selected_id: Option<u64>,
    window_capture_candidate: Option<DesktopWindowTarget>,
    window_captures: HashMap<u64, WindowCapture>,
    cheer_drop_effects: Vec<CheerDropEffect>,
    pending_cheer_drops: Vec<PendingCheerDrop>,
    cheer_portals: Vec<CheerPortalVisual>,
    cheer_labels: Vec<ScreenLabel>,
    debug_visible: bool,
    debug_hit_primary_cursor: bool,
    debug_left_down: bool,
    debug_right_down: bool,
    was_left_down: bool,
    was_right_down: bool,
    was_spawn_object_down: bool,
    was_spawn_crystal_down: bool,
    was_reset_down: bool,
    was_weather_toggle_down: bool,
    was_sand_toggle_down: bool,
    was_stress_spawn_down: bool,
    was_slingshot_toggle_down: bool,
    was_robot_buddy_down: bool,
    was_basketball_toggle_down: bool,
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
    basketball_game: BasketballGame,
    basketball_tracker: MouseTracker,
    basketball_confetti: Vec<(u64, f64)>,
    weather_world: WeatherWorld,
    sand_world: SandWorld,
    measure_tool: MeasureTool,
    spotlight_tool: SpotlightTool,
    lasso_tool: LassoTool,
    portal_pair_tool: PortalPairTool,
    shatter_gun: ShatterGunTool,
    shatter_backdrop_active: bool,
    fan_yaws: HashMap<u64, f64>,
    robot_carries: HashMap<u64, RobotCarry>,
    robot_drop_cooldowns: HashMap<u64, RobotDropCooldown>,
    robot_bin_ids: Vec<u64>,
    drone_carries: HashMap<u64, DroneCarry>,
    drone_drop_cooldowns: HashMap<u64, DroneDropCooldown>,
    snail_death_until_seconds: f64,
    snail_respawn_grace_until_seconds: f64,
    fallback_left_down: bool,
    fallback_right_down: bool,
    previous_global_import_keys: GlobalImportKeys,
    keyboard_modifiers: ModifiersState,
    current_cursor_icon: CursorIcon,
    target_frame_duration: Duration,
    next_frame_at: Instant,
    fps_counter: FpsCounter,
}

const FLOOR_MARGIN_PIXELS: f32 = 0.0;
const STRESS_SPAWN_COUNT: usize = 25;
const ROBOT_STACK_DROP_COOLDOWN_SECONDS: f64 = 1.8;
const ROBOT_THROW_HOLD_SECONDS: f64 = 0.55;
const ROBOT_BIN_WIDTH: f32 = 118.0;
const ROBOT_BIN_HEIGHT: f32 = 96.0;
const ROBOT_BIN_WALL: f32 = 12.0;
const FAN_RANGE_PIXELS: f32 = 560.0;
const FAN_HALF_ANGLE_RADIANS: f32 = 0.58;
const FAN_PUSH_ACCELERATION: f32 = 1850.0;
const DRONE_SPEED_PIXELS_PER_SECOND: f32 = 230.0;
const DRONE_ACCELERATION_PIXELS_PER_SECOND_SQUARED: f32 = 680.0;
const DRONE_PICKUP_RADIUS_PIXELS: f32 = 58.0;
const DRONE_DROP_RADIUS_PIXELS: f32 = 52.0;
const DRONE_BIN_RELEASE_RADIUS_PIXELS: f32 = 8.0;
const DRONE_DROP_COOLDOWN_SECONDS: f64 = 1.25;
const PORTAL_COOLDOWN_SECONDS: f64 = 0.30;
const PORTAL_HALF_LENGTH_PIXELS: f32 = 86.0;
const PORTAL_EDGE_MARGIN_PIXELS: f32 = 14.0;
const PORTAL_EXIT_OFFSET_PIXELS: f32 = 12.0;
const SNAIL_SPEED_PIXELS_PER_SECOND: f32 = 62.0;
const SNAIL_EAT_RADIUS_PIXELS: f32 = 34.0;
const SNAIL_DEATH_MESSAGE_SECONDS: f64 = 1.35;
const SNAIL_RESPAWN_GRACE_SECONDS: f64 = 1.1;
const SAND_CELL_SIZE_PIXELS: i32 = 4;
const SAND_EMIT_RADIUS_PIXELS: i32 = 10;
const SAND_ERASE_RADIUS_PIXELS: i32 = 18;

const BASKETBALL_SIZE: f32 = 92.0;
const BASKETBALL_HOOP_WIDTH: f32 = 360.0;
const BASKETBALL_HOOP_HEIGHT: f32 = 420.0;
/// How far behind the screen plane the hoop object sits.
const BASKETBALL_HOOP_DEPTH: f32 = 640.0;
/// Flicks slower than this just drop the ball instead of counting as a shot.
const BASKETBALL_MIN_SHOT_UP_SPEED: f32 = 260.0;
/// Caps the upward flick so the ball never slams the top of the screen.
const BASKETBALL_MAX_UP_SPEED: f32 = 1750.0;
const BASKETBALL_MAX_SIDE_SPEED: f32 = 1200.0;
/// Depth speed is derived from the upward flick: faster flick = deeper shot.
const BASKETBALL_DEPTH_BASE_SPEED: f32 = 200.0;
const BASKETBALL_DEPTH_UP_FACTOR: f32 = 0.66;
const BASKETBALL_RELOAD_TIMEOUT_SECONDS: f64 = 6.0;
/// Collision sphere of the ball is slightly smaller than its rendered size.
const BASKETBALL_COLLISION_SCALE: f32 = 0.92;
/// Radius of the static spheres approximating the rim ring in the physics world.
const BASKETBALL_RIM_COLLIDER_RADIUS: f32 = 9.0;
const BASKETBALL_RIM_COLLIDER_COUNT: usize = 14;
const BASKETBALL_CONFETTI_LIFETIME_SECONDS: f64 = 2.4;

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
            control_ui_process: None,
            control_command_rx: None,
            physics_paused: false,
            bounds: RectF::new(0.0, 0.0, 1280.0, 720.0),
            spawn_monitor_bounds: None,
            overlay_mode: OverlayInputMode::Interactive,
            pending_mode: OverlayInputMode::Interactive,
            mode_candidate_since_seconds: 0.0,
            cursor_local: Vector2::ZERO,
            selected_id: None,
            window_capture_candidate: None,
            window_captures: HashMap::new(),
            cheer_drop_effects: Vec::new(),
            pending_cheer_drops: Vec::new(),
            cheer_portals: Vec::new(),
            cheer_labels: Vec::new(),
            debug_visible: false,
            debug_hit_primary_cursor: false,
            debug_left_down: false,
            debug_right_down: false,
            was_left_down: false,
            was_right_down: false,
            was_spawn_object_down: false,
            was_spawn_crystal_down: false,
            was_reset_down: false,
            was_weather_toggle_down: false,
            was_sand_toggle_down: false,
            was_stress_spawn_down: false,
            was_slingshot_toggle_down: false,
            was_robot_buddy_down: false,
            was_basketball_toggle_down: false,
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
            basketball_game: BasketballGame::default(),
            basketball_tracker: MouseTracker::new(12),
            basketball_confetti: Vec::new(),
            weather_world: WeatherWorld::default(),
            sand_world: SandWorld::default(),
            measure_tool: MeasureTool::default(),
            spotlight_tool: SpotlightTool::default(),
            lasso_tool: LassoTool::default(),
            portal_pair_tool: PortalPairTool::default(),
            shatter_gun: ShatterGunTool::default(),
            shatter_backdrop_active: false,
            fan_yaws: HashMap::new(),
            robot_carries: HashMap::new(),
            robot_drop_cooldowns: HashMap::new(),
            robot_bin_ids: Vec::new(),
            drone_carries: HashMap::new(),
            drone_drop_cooldowns: HashMap::new(),
            snail_death_until_seconds: 0.0,
            snail_respawn_grace_until_seconds: 0.0,
            fallback_left_down: false,
            fallback_right_down: false,
            previous_global_import_keys: GlobalImportKeys::default(),
            keyboard_modifiers: ModifiersState::default(),
            current_cursor_icon: CursorIcon::Default,
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

        let initial_bounds = desktop_bounds(event_loop).unwrap_or(self.bounds);
        self.bounds = initial_bounds;
        self.spawn_monitor_bounds = event_loop.primary_monitor().map(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            RectF::new(
                position.x as f32 - initial_bounds.x,
                position.y as f32 - initial_bounds.y,
                size.width as f32,
                size.height as f32,
            )
        });

        let window = Arc::new(
            event_loop
                .create_window(overlay_window_attributes("ScreenOverlayPhysics Native", self.bounds))
                .context("Failed to create native overlay window")?,
        );
        configure_overlay_window(&window)?;
        self.bounds = sync_window_to_bounds(&window, initial_bounds);
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
        match start_control_ipc_server() {
            Ok(receiver) => {
                self.control_command_rx = Some(receiver);
                self.push_status_message(format!("Control UI IPC listening on {CONTROL_IPC_ADDR}."));
            },
            Err(error) => {
                self.push_status_message(format!("Control UI IPC unavailable: {error:#}."));
            },
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
                        shift_down: self.keyboard_modifiers.shift_key(),
                        spawn_object_down: false,
                        spawn_crystal_down: false,
                        reset_down: false,
                        weather_toggle_down: false,
                        sand_toggle_down: false,
                        spawn_stress_down: false,
                        slingshot_toggle_down: false,
                        robot_buddy_down: false,
                        basketball_toggle_down: false,
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
                shift_down: self.keyboard_modifiers.shift_key(),
                spawn_object_down: false,
                spawn_crystal_down: false,
                reset_down: false,
                weather_toggle_down: false,
                sand_toggle_down: false,
                spawn_stress_down: false,
                slingshot_toggle_down: false,
                robot_buddy_down: false,
                basketball_toggle_down: false,
                import_keys: GlobalImportKeys::default(),
            }
        };
        self.debug_left_down = pointer.left_down;
        self.debug_right_down = pointer.right_down;
        self.debug_hit_primary_cursor = self
            .hit_tester
            .is_point_over_any_object(self.scene.objects(), self.cursor_local);

        self.drain_control_commands();
        self.update_click_through_mode(now, &window);
        self.update_cursor_icon(&window);

        if self.drag_controller.is_dragging() {
            self.drag_controller
                .update_drag(self.scene.objects_mut(), self.cursor_local, now);
            self.update_held_object_rotation(pointer.right_down);
            let capture_modifier_down = pointer.shift_down || self.keyboard_modifiers.shift_key();
            self.window_capture_candidate = capture_modifier_down
                .then(|| desktop_window_at_point(pointer.screen_position))
                .flatten();
        } else {
            self.window_capture_candidate = None;
        }

        self.handle_global_mouse_buttons(now, pointer.left_down, pointer.right_down);
        self.handle_global_spawn_object(pointer.spawn_object_down);
        self.handle_global_spawn_crystal(pointer.spawn_crystal_down);
        self.handle_global_reset(pointer.reset_down);
        self.handle_global_weather_toggle(pointer.weather_toggle_down);
        self.handle_global_sand_toggle(pointer.sand_toggle_down);
        self.update_sand(pointer.left_down, pointer.right_down);
        self.update_weather(dt);
        self.update_spotlight();
        self.update_lasso(dt);
        self.shatter_gun
            .rebuild_render_cells(self.cursor_local, self.frame_clock.elapsed_seconds);
        self.handle_global_stress_spawn(pointer.spawn_stress_down);
        self.handle_global_slingshot_toggle(pointer.slingshot_toggle_down);
        self.handle_global_robot_buddy(pointer.robot_buddy_down);
        self.handle_global_basketball_toggle(pointer.basketball_toggle_down);
        self.handle_global_import_keys(pointer.import_keys);
        self.update_slingshot_game();
        self.update_basketball_game(dt);
        self.update_robot_buddies(dt);
        self.update_fans(dt);
        self.update_quad_drones(dt);
        self.update_snails(dt, &window);
        self.update_portals(dt);
        self.update_cheer_drop_effects(now);
        if !self.physics_paused {
            self.scene.step(dt, self.scene_bounds());
        }
        self.enforce_window_captures();
        self.collect_robot_bin_cubes();
        self.stabilize_robot_buddies();
        self.stabilize_quad_drones();
        self.restore_fan_yaws();
        self.sync_panels();
        window.request_redraw();
    }

    fn update_click_through_mode(&mut self, now_seconds: f64, window: &Window) {
        let should_be_interactive = self.force_interactive_for_debug
            || self.input_poller.is_none()
            || self.drag_controller.is_dragging()
            || self.debug_hit_primary_cursor
            || self.sand_world.active
            || self.lasso_tool.needs_interactive()
            || self.portal_pair_tool.needs_interactive()
            || self.basketball_game.aiming
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

        if self.portal_pair_tool.needs_interactive() {
            self.drag_controller.cancel_drag(self.scene.objects_mut());
            if let Some(message) = self.portal_pair_tool.update_input(
                self.cursor_local,
                self.scene_bounds(),
                is_left_down,
                is_right_down,
            ) {
                self.push_status_message(message);
            }
            self.was_left_down = is_left_down;
            self.was_right_down = is_right_down;
            self.is_rotation_dragging = false;
            self.last_drag_attempt = "portal-pair:place".to_string();
            return;
        }

        if self.shatter_gun.active {
            self.drag_controller.cancel_drag(self.scene.objects_mut());
            if is_left_down && !self.was_left_down {
                let impact = self.cursor_local;
                self.shatter_gun.fire(impact, now_seconds);
                self.trigger_screen_shatter_at(impact);
                self.last_drag_attempt = "shatter-gun:fire".to_string();
            } else if is_right_down && !self.was_right_down {
                self.shatter_gun.deactivate();
                self.push_status_message("Shatter gun holstered.".to_string());
                self.last_drag_attempt = "shatter-gun:holster".to_string();
            } else {
                self.last_drag_attempt = "shatter-gun:aim".to_string();
            }
            self.was_left_down = is_left_down;
            self.was_right_down = is_right_down;
            self.is_rotation_dragging = false;
            return;
        }

        if self.lasso_tool.active {
            self.drag_controller.cancel_drag(self.scene.objects_mut());
            let outcome = self.lasso_tool.update_input(
                self.cursor_local,
                is_left_down,
                is_right_down,
                now_seconds,
                self.scene.objects(),
                &self.hit_tester,
            );
            if let Some(id) = outcome.primary_id {
                self.selected_id = Some(id);
            }
            if let Some(message) = outcome.status_message {
                self.push_status_message(message);
            }
            self.was_left_down = is_left_down;
            self.was_right_down = is_right_down;
            self.is_rotation_dragging = false;
            self.last_drag_attempt = if self.lasso_tool.drawing {
                "lasso:draw".to_string()
            } else if self.lasso_tool.has_capture() {
                "lasso:whip".to_string()
            } else {
                "lasso".to_string()
            };
            return;
        }

        if self.measure_tool.active {
            self.measure_tool
                .update(self.cursor_local, is_left_down, is_right_down);
            self.drag_controller.cancel_drag(self.scene.objects_mut());
            self.was_left_down = is_left_down;
            self.was_right_down = is_right_down;
            self.is_rotation_dragging = false;
            self.last_drag_attempt = if self.measure_tool.dragging {
                "measure:drag".to_string()
            } else {
                "measure".to_string()
            };
            return;
        }

        if self.slingshot_game.active {
            self.handle_slingshot_mouse(is_left_down);
            self.was_left_down = is_left_down;
            self.was_right_down = is_right_down;
            self.is_rotation_dragging = false;
            return;
        }

        if self.basketball_game.active {
            self.handle_basketball_mouse(is_left_down);
            self.was_left_down = is_left_down;
            self.was_right_down = is_right_down;
            self.is_rotation_dragging = false;
            return;
        }

        if is_left_down && !self.was_left_down {
            let began = if self.cursor_is_over_robot_bin() {
                None
            } else {
                self.drag_controller.begin_drag(
                    self.scene.objects_mut(),
                    self.cursor_local,
                    now_seconds,
                    &self.hit_tester,
                )
            };
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

    fn update_cursor_icon(&mut self, window: &Window) {
        let desired = if self.drag_controller.is_dragging()
            || self.slingshot_game.aiming
            || self.basketball_game.aiming
            || self.measure_tool.dragging
            || self.lasso_tool.drawing
            || self.lasso_tool.has_capture()
            || self.portal_pair_tool.needs_interactive()
        {
            CursorIcon::Grabbing
        } else if self.measure_tool.active
            || self.lasso_tool.active
            || self.portal_pair_tool.needs_interactive()
            || self.shatter_gun.active
            || self.debug_hit_primary_cursor
            || self.cursor_is_over_active_game_object()
        {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        };

        if desired != self.current_cursor_icon {
            window.set_cursor(desired);
            self.current_cursor_icon = desired;
        }
    }

    fn cursor_is_over_active_game_object(&self) -> bool {
        if self.slingshot_game.active && self.slingshot_game.ready {
            if let Some(projectile_id) = self.slingshot_game.projectile_id {
                return self.cursor_is_over_projectile(projectile_id);
            }
        }

        if self.basketball_game.active && self.basketball_game.ready {
            if let Some(ball_id) = self.basketball_game.ball_id {
                return self.cursor_is_over_basketball(ball_id);
            }
        }

        false
    }

    fn handle_global_stress_spawn(&mut self, is_down: bool) {
        if is_down && !self.was_stress_spawn_down {
            self.spawn_stress_cubes();
        }
        self.was_stress_spawn_down = is_down;
    }

    fn handle_global_spawn_object(&mut self, is_down: bool) {
        if is_down && !self.was_spawn_object_down {
            let id = self.scene.spawn_next_object(self.default_spawn_position());
            self.selected_id = Some(id);
        }
        self.was_spawn_object_down = is_down;
    }

    fn handle_global_spawn_crystal(&mut self, is_down: bool) {
        if is_down && !self.was_spawn_crystal_down {
            let id = self.scene.spawn_random_crystal(self.default_spawn_position());
            self.selected_id = Some(id);
        }
        self.was_spawn_crystal_down = is_down;
    }

    fn handle_global_reset(&mut self, is_down: bool) {
        if is_down && !self.was_reset_down {
            self.reset_everything();
        }
        self.was_reset_down = is_down;
    }

    fn handle_global_weather_toggle(&mut self, is_down: bool) {
        if is_down && !self.was_weather_toggle_down {
            self.toggle_weather_world();
        }
        self.was_weather_toggle_down = is_down;
    }

    fn toggle_weather_world(&mut self) {
        let active = self.weather_world.toggle(self.scene_bounds());
        self.push_status_message(if active {
            "Rain on. F5 toggles.".to_string()
        } else {
            "Rain off.".to_string()
        });
    }

    fn update_weather(&mut self, dt: f32) {
        if self.weather_world.active {
            self.weather_world.step(dt, self.scene_bounds());
        }
    }

    fn handle_global_sand_toggle(&mut self, is_down: bool) {
        if is_down && !self.was_sand_toggle_down {
            self.toggle_sand_world();
        }
        self.was_sand_toggle_down = is_down;
    }

    fn toggle_sand_world(&mut self) {
        let active = self.sand_world.toggle(self.scene_bounds());
        self.push_status_message(if active {
            "Sand on: hold left mouse to pour. F6 toggles.".to_string()
        } else {
            "Sand off.".to_string()
        });
    }

    fn toggle_measure_tool(&mut self) {
        let active = self.measure_tool.toggle();
        if active {
            self.lasso_tool.deactivate();
            self.portal_pair_tool.stop_placement();
            self.shatter_gun.deactivate();
            self.drag_controller.cancel_drag(self.scene.objects_mut());
        }
        self.push_status_message(if active {
            "Measure tool on: drag to measure, click-through stays on.".to_string()
        } else {
            "Measure tool off.".to_string()
        });
    }

    fn toggle_spotlight(&mut self) {
        let active = self.spotlight_tool.toggle(self.cursor_local, self.scene_bounds());
        self.push_status_message(if active {
            "Spotlight on, click-through stays on.".to_string()
        } else {
            "Spotlight off.".to_string()
        });
    }

    fn update_spotlight(&mut self) {
        self.spotlight_tool.update(self.cursor_local, self.scene_bounds());
    }

    fn toggle_lasso_tool(&mut self) {
        if self.lasso_tool.active {
            self.lasso_tool.deactivate();
            self.push_status_message("Rope lasso off.".to_string());
        } else {
            self.lasso_tool.activate(self.cursor_local);
            self.measure_tool.clear();
            self.portal_pair_tool.stop_placement();
            self.shatter_gun.deactivate();
            self.drag_controller.cancel_drag(self.scene.objects_mut());
            self.push_status_message(
                "Rope lasso on: drag a loop, release to snare, move mouse to whip. Right releases.".to_string(),
            );
        }
    }

    fn toggle_portal_pair_tool(&mut self) {
        if self.portal_pair_tool.has_portal_pair() || self.portal_pair_tool.needs_interactive() {
            self.portal_pair_tool.clear();
            self.push_status_message("Portal pair cleared.".to_string());
            return;
        }

        self.portal_pair_tool.start_placement();
        self.measure_tool.clear();
        self.lasso_tool.clear();
        self.shatter_gun.deactivate();
        self.drag_controller.cancel_drag(self.scene.objects_mut());
        self.was_left_down = true;
        self.was_right_down = true;
        self.push_status_message("Portal placement on: click two screen edges to link them. Right click cancels.".to_string());
    }

    fn toggle_shatter_gun(&mut self) {
        let active = self.shatter_gun.toggle(self.cursor_local, self.frame_clock.elapsed_seconds);
        self.drag_controller.cancel_drag(self.scene.objects_mut());
        if active {
            self.measure_tool.clear();
            self.lasso_tool.clear();
            self.portal_pair_tool.stop_placement();
            self.was_left_down = true;
            self.was_right_down = true;
            self.push_status_message("Shatter gun equipped: click anywhere to fire, B holsters.".to_string());
        } else {
            self.push_status_message("Shatter gun holstered.".to_string());
        }
    }

    fn trigger_screen_shatter(&mut self) {
        self.trigger_screen_shatter_at(self.cursor_local);
    }

    fn trigger_screen_shatter_at(&mut self, impact: Vector2) {
        #[cfg(target_os = "windows")]
        if let Some(d3d_renderer) = &mut self.d3d_renderer {
            if let Err(error) = d3d_renderer.capture_screen_texture(self.bounds) {
                self.push_status_message(format!("Screen capture failed; using fallback shard color: {error:#}."));
            }
        }
        #[cfg(not(target_os = "windows"))]
        self.push_status_message("Screen capture texture is only wired for the Windows Direct3D backend.".to_string());

        self.drag_controller.cancel_drag(self.scene.objects_mut());
        self.scene.clear_static_colliders();
        self.slingshot_game = SlingshotGame::default();
        self.basketball_game = BasketballGame::default();
        self.basketball_tracker.clear();
        self.basketball_confetti.clear();
        self.pending_cheer_drops.clear();
        self.cheer_drop_effects.clear();
        self.cheer_portals.clear();
        self.cheer_labels.clear();
        self.weather_world.clear();
        self.sand_world.clear();
        self.measure_tool.clear();
        self.spotlight_tool.clear();
        self.lasso_tool.clear();
        self.portal_pair_tool.clear();
        self.robot_carries.clear();
        self.robot_drop_cooldowns.clear();
        self.robot_bin_ids.clear();
        self.fan_yaws.clear();
        self.drone_carries.clear();
        self.drone_drop_cooldowns.clear();
        self.snail_death_until_seconds = 0.0;
        self.snail_respawn_grace_until_seconds = 0.0;
        self.import_panel = None;
        self.settings_panel.visible = false;
        self.is_rotation_dragging = false;
        self.last_rotation_cursor = Vector2::ZERO;

        let shard_count = self.scene.spawn_screen_shatter(self.scene_bounds(), impact);
        self.selected_id = None;
        self.shatter_backdrop_active = shard_count > 0;
        self.physics_paused = false;
        self.push_status_message(format!(
            "Screen shattered into {shard_count} pieces. {}",
            if self.shatter_gun.active {
                "Click to fire again, B holsters."
            } else {
                "Press B to equip the shatter gun, F3 to reset."
            }
        ));
    }

    fn update_lasso(&mut self, dt: f32) {
        if !self.lasso_tool.active {
            return;
        }
        let velocity_deltas = self
            .lasso_tool
            .compute_velocity_deltas(self.scene.objects(), self.cursor_local, dt);
        for (id, delta) in velocity_deltas {
            self.scene.add_object_velocity(id, delta);
        }
        self.lasso_tool
            .rebuild_render_cells(self.cursor_local, self.scene.objects(), self.frame_clock.elapsed_seconds);
    }

    fn update_sand(&mut self, is_left_down: bool, is_right_down: bool) {
        if !self.sand_world.active {
            return;
        }
        let bounds = self.scene_bounds();
        if !self.measure_tool.active
            && !self.lasso_tool.active
            && !self.settings_panel.visible
            && self.import_panel.is_none()
        {
            if is_left_down {
                self.sand_world.emit_at(self.cursor_local, bounds);
            }
            if is_right_down {
                self.sand_world.erase_at(self.cursor_local, bounds);
            }
        }
        self.sand_world.step(bounds);
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

    fn handle_global_basketball_toggle(&mut self, is_down: bool) {
        if is_down && !self.was_basketball_toggle_down {
            self.toggle_basketball_game();
        }
        self.was_basketball_toggle_down = is_down;
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

    fn spawn_control_visual_kind(&mut self, payload: Option<&serde_json::Value>) {
        let kind = payload
            .and_then(|payload| payload.get("kind"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Cube");
        let Some((visual_kind, color, label)) = control_visual_kind(kind) else {
            self.push_status_message(format!("Unknown Control UI spawn variant: {kind}."));
            return;
        };

        if visual_kind == ObjectVisualKind::Snail {
            self.spawn_snail();
            return;
        }
        if visual_kind == ObjectVisualKind::Fan {
            self.spawn_fan();
            return;
        }
        if visual_kind == ObjectVisualKind::QuadDrone {
            self.spawn_quad_drone();
            return;
        }

        let id = self.scene.spawn_object(self.default_spawn_position(), color, visual_kind);
        self.selected_id = Some(id);
        self.push_status_message(format!("Control UI spawned {label}."));
    }

    fn apply_control_runtime_settings(&mut self, payload: Option<&serde_json::Value>) {
        let Some(payload) = payload else {
            self.push_status_message("Control UI settings payload missing.".to_string());
            return;
        };

        let gravity_y = {
            let config = self.scene.config_mut();
            config.gravity_y = payload_f32(payload, "gravityY", config.gravity_y, 0.0, 4000.0);
            config.throw_sensitivity = payload_f32(payload, "throwSensitivity", config.throw_sensitivity, 0.1, 4.0);
            config.max_throw_speed = payload_f32(payload, "maxThrowSpeed", config.max_throw_speed, 100.0, 6000.0);
            config.restitution = payload_f32(payload, "restitution", config.restitution, 0.05, 1.2);
            config.linear_damping = payload_f32(payload, "linearDamping", config.linear_damping, 0.900, 0.999);
            config.sleep_threshold = payload_f32(payload, "sleepThreshold", config.sleep_threshold, 1.0, 120.0);
            config.floor_snap_threshold = payload_f32(payload, "floorSnapThreshold", config.floor_snap_threshold, 0.0, 24.0);
            if let Some(debounce_ms) = payload.get("interactionDebounceMs").and_then(serde_json::Value::as_u64) {
                config.interaction_debounce_ms = debounce_ms.clamp(0, 300) as u32;
            }
            if let Some(start_in_pass_through) = payload.get("startInPassThrough").and_then(serde_json::Value::as_bool) {
                config.start_in_pass_through = start_in_pass_through;
            }
            config.gravity_y
        };

        if let Some(click_through) = payload.get("clickThrough").and_then(serde_json::Value::as_bool) {
            self.force_interactive_for_debug = !click_through;
        }

        self.scene.set_gravity(gravity_y);
        self.scene.apply_runtime_physics_config();
        self.push_status_message("Control UI applied runtime settings.".to_string());
    }

    fn drain_control_commands(&mut self) {
        loop {
            let command = {
                let Some(receiver) = self.control_command_rx.as_ref() else {
                    return;
                };
                receiver.try_recv()
            };

            match command {
                Ok(command) => self.handle_control_command(command),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.control_command_rx = None;
                    self.push_status_message("Control UI IPC disconnected.".to_string());
                    return;
                },
            }
        }
    }

    fn handle_control_command(&mut self, command: ControlIpcCommand) {
        match command.command.as_str() {
            "toggle_debug" => {
                self.debug_visible = !self.debug_visible;
                self.push_status_message(if self.debug_visible {
                    "Control UI enabled debug HUD.".to_string()
                } else {
                    "Control UI disabled debug HUD.".to_string()
                });
            },
            "spawn_next_catalog" => {
                let id = self.scene.spawn_next_object(self.default_spawn_position());
                self.selected_id = Some(id);
                self.push_status_message("Control UI spawned next catalog object.".to_string());
            },
            "spawn_cube" => {
                let id = self
                    .scene
                    .spawn_object(self.default_spawn_position(), None, ObjectVisualKind::Cube);
                self.selected_id = Some(id);
                self.push_status_message("Control UI spawned cube.".to_string());
            },
            "spawn_crystal" => {
                let id = self.scene.spawn_random_crystal(self.default_spawn_position());
                self.selected_id = Some(id);
                self.push_status_message("Control UI spawned crystal.".to_string());
            },
            "spawn_dvd_logo" => {
                let id = self.scene.spawn_random_dvd_logo(self.default_spawn_position());
                self.selected_id = Some(id);
                self.push_status_message("Control UI spawned DVD logo.".to_string());
            },
            "spawn_target" => {
                let id = self.scene.spawn_object(
                    self.default_spawn_position(),
                    Some(AppColor::from_rgb(255, 224, 92)),
                    ObjectVisualKind::GameTarget,
                );
                self.selected_id = Some(id);
                self.push_status_message("Control UI spawned target.".to_string());
            },
            "spawn_visual_kind" => self.spawn_control_visual_kind(command.payload.as_ref()),
            "simulate_twitch_cheer" => self.simulate_twitch_cheer(command.payload.as_ref()),
            "spawn_robot_buddy" => self.spawn_robot_buddy(),
            "spawn_stress_batch" => self.spawn_stress_cubes(),
            "reset_scene" => self.reset_everything(),
            "pause_physics" => {
                self.physics_paused = true;
                self.push_status_message("Control UI paused physics.".to_string());
            },
            "resume_physics" => {
                self.physics_paused = false;
                self.push_status_message("Control UI resumed physics.".to_string());
            },
            "set_scene_mode" => {
                let mode = command
                    .payload
                    .as_ref()
                    .and_then(|payload| payload.get("mode"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("custom");
                self.push_status_message(format!("Control UI scene mode: {mode}."));
            },
            "apply_runtime_settings" => self.apply_control_runtime_settings(command.payload.as_ref()),
            "set_spawn_monitor" => self.set_spawn_monitor(command.payload.as_ref()),
            "toggle_weather" => self.toggle_weather_world(),
            "toggle_sand" => self.toggle_sand_world(),
            "toggle_measure_tool" => self.toggle_measure_tool(),
            "toggle_spotlight" => self.toggle_spotlight(),
            "toggle_lasso_tool" => self.toggle_lasso_tool(),
            "toggle_portal_pair_tool" => self.toggle_portal_pair_tool(),
            "toggle_shatter_gun" => self.toggle_shatter_gun(),
            "shatter_screen" => self.trigger_screen_shatter(),
            "toggle_slingshot_game" => self.toggle_slingshot_game(),
            "toggle_basketball_game" => self.toggle_basketball_game(),
            "release_lasso" => {
                self.lasso_tool.release_capture();
                self.push_status_message("Control UI released lasso.".to_string());
            },
            "open_overlay_settings" => {
                self.settings_panel.visible = !self.settings_panel.visible;
                if self.settings_panel.visible {
                    self.import_panel = None;
                }
            },
            "import_model" => {
                if let Some(path) = pick_model_file() {
                    self.import_panel = Some(ImportPanel::new(path.display().to_string()));
                    self.settings_panel.visible = false;
                }
            },
            _ => {
                self.push_status_message(format!("Unknown Control UI command: {}", command.command));
            },
        }
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

    fn spawn_fan(&mut self) {
        let mut position = self.default_spawn_position();
        position.y = (position.y + 160.0).min(self.scene_bounds().bottom() - 140.0);
        let id = self
            .scene
            .spawn_object(position, Some(AppColor::from_rgb(105, 230, 255)), ObjectVisualKind::Fan);
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == id) {
            object.rotation_z = 0.0;
            object.body.gravity_scale = 0.0;
            object.body.velocity = Vector2::ZERO;
        }
        self.fan_yaws.insert(id, 0.0);
        self.selected_id = Some(id);
        self.push_status_message("Fan spawned. Drag it around; right-drag while held aims the gust.".to_string());
    }

    fn spawn_quad_drone(&mut self) {
        let mut position = self.default_spawn_position();
        position.y = (position.y + 70.0).min(self.scene_bounds().bottom() - 220.0);
        let id = self.scene.spawn_object(
            position,
            Some(AppColor::from_rgb(248, 250, 252)),
            ObjectVisualKind::QuadDrone,
        );
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == id) {
            object.body.gravity_scale = 0.0;
            object.body.velocity = Vector2::ZERO;
            object.body.lock_rotation = true;
        }
        self.selected_id = Some(id);
        self.push_status_message("Quadcopter drone spawned. It will tidy loose desktop toys into the bin.".to_string());
    }

    fn spawn_snail(&mut self) {
        let id = self.scene.spawn_object(
            self.snail_spawn_position(),
            Some(AppColor::from_rgb(166, 214, 124)),
            ObjectVisualKind::Snail,
        );
        self.selected_id = Some(id);
        self.push_status_message("Snail spawned. It wants the mouse.".to_string());
    }

    fn snail_spawn_position(&self) -> Vector2 {
        let bounds = self.scene_bounds();
        let margin = 72.0;
        let snail_width = 124.0;
        let snail_height = 64.0;
        let candidates = [
            Vector2::new(margin, margin),
            Vector2::new((bounds.right() - snail_width - margin).max(margin), margin),
            Vector2::new(margin, (bounds.bottom() - snail_height - margin).max(margin)),
            Vector2::new(
                (bounds.right() - snail_width - margin).max(margin),
                (bounds.bottom() - snail_height - margin).max(margin),
            ),
        ];
        candidates
            .into_iter()
            .max_by(|left, right| {
                let left_center = *left + Vector2::new(snail_width * 0.5, snail_height * 0.5);
                let right_center = *right + Vector2::new(snail_width * 0.5, snail_height * 0.5);
                let left_distance = (left_center - self.cursor_local).length_squared();
                let right_distance = (right_center - self.cursor_local).length_squared();
                left_distance
                    .partial_cmp(&right_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or_else(|| self.default_spawn_position())
    }

    fn update_snails(&mut self, dt: f32, window: &Window) {
        let cursor = self.cursor_local;
        let now = self.frame_clock.elapsed_seconds;
        let in_respawn_grace = now < self.snail_respawn_grace_until_seconds;
        let mut eaten_by = None;

        for snail in self
            .scene
            .objects_mut()
            .iter_mut()
            .filter(|object| object.visual_kind == ObjectVisualKind::Snail)
        {
            if self.drag_controller.dragged_id() == Some(snail.id) {
                continue;
            }

            let center = object_center(snail);
            let delta = cursor - center;
            let distance = delta.length_squared().sqrt();
            let direction = if distance > 0.001 {
                delta / distance
            } else {
                Vector2::ZERO
            };
            let velocity = direction * SNAIL_SPEED_PIXELS_PER_SECOND;
            snail.body.position += velocity * dt;
            snail.body.velocity = velocity;
            snail.body.is_dragging = true;
            snail.is_dragging = false;
            snail.body.is_sleeping = false;
            snail.body.sleep_timer_seconds = 0.0;
            snail.body.gravity_scale = 0.0;
            snail.rotation_x *= 0.82;
            snail.rotation_y *= 0.82;
            snail.angular_velocity_x = 0.0;
            snail.angular_velocity_y = 0.0;
            snail.angular_velocity_z = 0.0;

            if !in_respawn_grace && distance <= SNAIL_EAT_RADIUS_PIXELS {
                eaten_by = Some(center);
            }
        }

        if let Some(snail_center) = eaten_by {
            self.handle_snail_death(window, snail_center);
        }
    }

    fn handle_snail_death(&mut self, window: &Window, snail_center: Vector2) {
        let now = self.frame_clock.elapsed_seconds;
        let respawn = self.snail_mouse_respawn_point(snail_center);
        self.snail_death_until_seconds = now + SNAIL_DEATH_MESSAGE_SECONDS;
        self.snail_respawn_grace_until_seconds = now + SNAIL_RESPAWN_GRACE_SECONDS;
        self.cursor_local = respawn;
        self.was_left_down = false;
        self.was_right_down = false;
        let _ = window.set_cursor_position(PhysicalPosition::new(respawn.x as f64, respawn.y as f64));
        self.push_status_message("The snail ate the mouse. Respawning.".to_string());
    }

    fn snail_mouse_respawn_point(&self, primary_snail_center: Vector2) -> Vector2 {
        let bounds = self.scene_bounds();
        let margin = 86.0;
        let candidates = [
            Vector2::new(margin, margin),
            Vector2::new((bounds.right() - margin).max(margin), margin),
            Vector2::new(margin, (bounds.bottom() - margin).max(margin)),
            Vector2::new((bounds.right() - margin).max(margin), (bounds.bottom() - margin).max(margin)),
            Vector2::new(bounds.width * 0.5, margin),
            Vector2::new(bounds.width * 0.5, (bounds.bottom() - margin).max(margin)),
        ];
        candidates
            .into_iter()
            .max_by(|left, right| {
                let left_distance = self.snail_respawn_safety_score(*left, primary_snail_center);
                let right_distance = self.snail_respawn_safety_score(*right, primary_snail_center);
                left_distance
                    .partial_cmp(&right_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(Vector2::new(bounds.width * 0.5, bounds.height * 0.5))
    }

    fn snail_respawn_safety_score(&self, candidate: Vector2, primary_snail_center: Vector2) -> f32 {
        self.scene
            .objects()
            .iter()
            .filter(|object| object.visual_kind == ObjectVisualKind::Snail)
            .map(object_center)
            .chain(std::iter::once(primary_snail_center))
            .map(|center| (candidate - center).length_squared())
            .fold(f32::MAX, f32::min)
    }

    fn update_robot_buddies(&mut self, _dt: f32) {
        if self.slingshot_game.active || self.basketball_game.active {
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
        RectF::new(42.0, bottom - ROBOT_BIN_HEIGHT, ROBOT_BIN_WIDTH, ROBOT_BIN_HEIGHT)
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

    fn cursor_is_over_robot_bin(&self) -> bool {
        self.hit_tester
            .hit_test_topmost(self.scene.objects(), self.cursor_local)
            .is_some_and(|object| self.robot_bin_ids.contains(&object.id))
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
            .filter(|object| object_is_cleanup_bin_collectable(object))
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
        self.drone_carries.retain(|_, carry| !collected.contains(&carry.object_id));
        self.drone_drop_cooldowns
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

    fn update_fans(&mut self, dt: f32) {
        let dragged_id = self.drag_controller.dragged_id();
        let mut fans = Vec::new();
        let mut fan_ids = Vec::new();
        for object in self
            .scene
            .objects_mut()
            .iter_mut()
            .filter(|object| object.visual_kind == ObjectVisualKind::Fan)
        {
            let yaw = if dragged_id == Some(object.id) || object.is_dragging || object.body.is_dragging {
                object.rotation_z
            } else {
                *self.fan_yaws.entry(object.id).or_insert(object.rotation_z)
            };
            self.fan_yaws.insert(object.id, yaw);
            object.rotation_z = yaw;
            object.rotation_x *= 0.5;
            object.rotation_y *= 0.5;
            object.angular_velocity_x = 0.0;
            object.angular_velocity_y = 0.0;
            object.angular_velocity_z = 0.0;
            object.body.gravity_scale = 0.0;
            object.body.is_sleeping = false;
            object.body.sleep_timer_seconds = 0.0;
            fan_ids.push(object.id);
            fans.push((object.id, object_center(object), yaw));
        }
        self.fan_yaws.retain(|id, _| fan_ids.contains(id));
        if fans.is_empty() || dt <= 0.0 {
            return;
        }

        let objects = self.scene.objects().to_vec();
        let cone_tan = FAN_HALF_ANGLE_RADIANS.tan();
        let mut pushes = Vec::new();
        for (fan_id, fan_center, yaw_degrees) in fans {
            let radians = (yaw_degrees as f32).to_radians();
            let forward = Vector2::new(radians.cos(), radians.sin());
            let normal = Vector2::new(-forward.y, forward.x);
            for object in objects.iter().filter(|object| self.is_fan_push_candidate(fan_id, object)) {
                let target_center = object_center(object);
                let delta = target_center - fan_center;
                let forward_distance = vector_dot(delta, forward);
                if !(22.0..=FAN_RANGE_PIXELS).contains(&forward_distance) {
                    continue;
                }
                let lateral = vector_dot(delta, normal).abs();
                let body_radius = object.body.width.max(object.body.height) * 0.5;
                let cone_half_width = forward_distance * cone_tan + body_radius;
                if lateral > cone_half_width {
                    continue;
                }
                let distance_falloff = 1.0 - (forward_distance / FAN_RANGE_PIXELS).clamp(0.0, 1.0);
                let lateral_falloff = 1.0 - (lateral / cone_half_width.max(1.0)).clamp(0.0, 1.0) * 0.35;
                let mass_falloff = (1.35 / object.body.mass.max(0.35)).sqrt().clamp(0.45, 1.7);
                let impulse = FAN_PUSH_ACCELERATION * dt * distance_falloff.max(0.12) * lateral_falloff * mass_falloff;
                pushes.push((object.id, forward * impulse));
            }
        }
        for (id, delta) in pushes {
            self.scene.add_object_velocity(id, delta);
        }
    }

    fn is_fan_push_candidate(&self, fan_id: u64, object: &ObjectState) -> bool {
        object.id != fan_id
            && object.body.collidable
            && !object.is_dragging
            && !object.body.is_dragging
            && object.depth_z >= -1.0
            && !self.robot_bin_ids.contains(&object.id)
            && !matches!(
                object.visual_kind,
                ObjectVisualKind::Fan
                    | ObjectVisualKind::RobotBuddy
                    | ObjectVisualKind::QuadDrone
                    | ObjectVisualKind::BasketballHoop
                    | ObjectVisualKind::ImportedModel
                    | ObjectVisualKind::ScreenShard
            )
    }

    fn restore_fan_yaws(&mut self) {
        for object in self
            .scene
            .objects_mut()
            .iter_mut()
            .filter(|object| object.visual_kind == ObjectVisualKind::Fan)
        {
            if let Some(yaw) = self.fan_yaws.get(&object.id).copied() {
                object.rotation_z = yaw;
                object.angular_velocity_z = 0.0;
            }
        }
    }

    fn update_quad_drones(&mut self, dt: f32) {
        if self.slingshot_game.active || self.basketball_game.active || dt <= 0.0 {
            return;
        }

        let objects = self.scene.objects().to_vec();
        let drone_ids: Vec<u64> = objects
            .iter()
            .filter(|object| object.visual_kind == ObjectVisualKind::QuadDrone)
            .map(|object| object.id)
            .collect();
        if drone_ids.is_empty() {
            self.drone_carries.clear();
            self.drone_drop_cooldowns.clear();
            return;
        }
        self.ensure_robot_bin();
        self.drone_carries
            .retain(|drone_id, carry| drone_ids.contains(drone_id) && objects.iter().any(|object| object.id == carry.object_id));
        self.drone_drop_cooldowns.retain(|drone_id, cooldown| {
            drone_ids.contains(drone_id)
                && self.frame_clock.elapsed_seconds - cooldown.dropped_at < DRONE_DROP_COOLDOWN_SECONDS
                && objects.iter().any(|object| object.id == cooldown.object_id)
        });

        for drone_id in drone_ids {
            let Some(drone_snapshot) = objects.iter().find(|object| object.id == drone_id) else {
                continue;
            };
            if self.drag_controller.dragged_id() == Some(drone_id) || drone_snapshot.is_dragging {
                self.drop_drone_carry(drone_id, Vector2::new(0.0, -40.0));
                continue;
            }

            if let Some(carry) = self.drone_carries.get(&drone_id).copied() {
                if self.frame_clock.elapsed_seconds - carry.picked_up_at > 12.0 {
                    self.drop_drone_carry(drone_id, Vector2::new(0.0, 80.0));
                    continue;
                }
                let target = self.drone_drop_target();
                self.fly_drone_toward(drone_id, target, dt);
                if let Some(updated_drone) = self.scene.objects().iter().find(|object| object.id == drone_id).cloned() {
                    self.position_drone_carry(&updated_drone, carry.object_id);
                }
                let drone_center = self
                    .scene
                    .objects()
                    .iter()
                    .find(|object| object.id == drone_id)
                    .map(object_center)
                    .unwrap_or_else(|| object_center(drone_snapshot));
                if (drone_center - target).length_squared()
                    <= DRONE_BIN_RELEASE_RADIUS_PIXELS * DRONE_BIN_RELEASE_RADIUS_PIXELS
                {
                    self.drop_drone_carry(drone_id, Vector2::new(0.0, 115.0));
                }
                continue;
            }

            let drone_center = object_center(drone_snapshot);
            let nearest = objects
                .iter()
                .filter(|object| self.is_drone_carry_candidate(drone_id, object))
                .map(|object| {
                    let center = object_center(object);
                    let distance = (center - drone_center).length_squared();
                    let priority = if object.visual_kind == ObjectVisualKind::BitCrystal { 0.18 } else { 1.0 };
                    (object.id, center, distance, distance * priority)
                })
                .min_by(|(_, _, _, left), (_, _, _, right)| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

            if let Some((object_id, object_center, distance, _)) = nearest {
                let target = object_center + Vector2::new(0.0, -80.0);
                let reached_pickup_hover = self.fly_drone_toward(drone_id, target, dt);
                if reached_pickup_hover || distance <= DRONE_PICKUP_RADIUS_PIXELS * DRONE_PICKUP_RADIUS_PIXELS {
                    self.drone_carries.insert(
                        drone_id,
                        DroneCarry {
                            object_id,
                            picked_up_at: self.frame_clock.elapsed_seconds,
                        },
                    );
                    if let Some(updated_drone) = self.scene.objects().iter().find(|object| object.id == drone_id).cloned() {
                        self.position_drone_carry(&updated_drone, object_id);
                    }
                }
                continue;
            }

            let patrol_phase = (self.frame_clock.elapsed_seconds * 0.45 + drone_id as f64 * 0.11).sin() as f32;
            let bounds = self.scene_bounds();
            let target = Vector2::new(
                (bounds.width * 0.58 + patrol_phase * 190.0).clamp(120.0, bounds.right() - 120.0),
                (bounds.height * 0.22).clamp(90.0, bounds.bottom() - 220.0),
            );
            self.fly_drone_toward(drone_id, target, dt);
        }
    }

    fn is_drone_carry_candidate(&self, drone_id: u64, object: &ObjectState) -> bool {
        let bin = self.robot_bin_rect();
        let center = object_center(object);
        let in_bin_zone = center.x > bin.x - object.body.width
            && center.x < bin.right() + object.body.width
            && center.y > bin.y - object.body.height
            && center.y < bin.bottom() + object.body.height;
        object.id != drone_id
            && object.body.collidable
            && !object.is_dragging
            && !object.body.is_dragging
            && object.depth_z >= -1.0
            && object.body.width <= 150.0
            && object.body.height <= 150.0
            && !in_bin_zone
            && !self.robot_bin_ids.contains(&object.id)
            && !self.robot_carries.values().any(|carry| carry.object_id == object.id)
            && !self.drone_carries.values().any(|carry| carry.object_id == object.id)
            && !self
                .drone_drop_cooldowns
                .get(&drone_id)
                .is_some_and(|cooldown| cooldown.object_id == object.id)
            && !matches!(
                object.visual_kind,
                ObjectVisualKind::RobotBuddy
                    | ObjectVisualKind::QuadDrone
                    | ObjectVisualKind::Fan
                    | ObjectVisualKind::Snail
                    | ObjectVisualKind::FoxBuddy
                    | ObjectVisualKind::ImportedModel
                    | ObjectVisualKind::GamePlank
                    | ObjectVisualKind::BasketballHoop
                    | ObjectVisualKind::ScreenShard
            )
    }

    fn fly_drone_toward(&mut self, drone_id: u64, target: Vector2, dt: f32) -> bool {
        let Some(drone) = self.scene.objects_mut().iter_mut().find(|object| object.id == drone_id) else {
            return false;
        };
        let center = object_center(drone);
        let delta = target - center;
        let distance = delta.length_squared().sqrt();
        let reached = distance <= DRONE_DROP_RADIUS_PIXELS;
        let direction = if distance > 0.001 {
            delta / distance
        } else {
            Vector2::ZERO
        };
        let braking_speed = (2.0 * DRONE_ACCELERATION_PIXELS_PER_SECOND_SQUARED * distance).sqrt();
        let desired_speed = DRONE_SPEED_PIXELS_PER_SECOND.min(braking_speed);
        let desired_velocity = direction * desired_speed;
        let velocity_delta = clamp_vector(
            desired_velocity - drone.body.velocity,
            DRONE_ACCELERATION_PIXELS_PER_SECOND_SQUARED * dt,
        );
        let mut flight_velocity = drone.body.velocity + velocity_delta;
        let movement = clamp_vector(flight_velocity * dt, distance);
        let next_center = center + movement;
        if distance < 2.0 {
            flight_velocity = Vector2::ZERO;
        }
        drone.body.position = Vector2::new(
            next_center.x - drone.body.width * 0.5,
            next_center.y - drone.body.height * 0.5,
        );
        drone.body.velocity = flight_velocity;
        drone.body.gravity_scale = 0.0;
        drone.body.is_dragging = true;
        drone.is_dragging = false;
        drone.body.is_sleeping = false;
        drone.body.sleep_timer_seconds = 0.0;
        drone.body.lock_rotation = true;
        drone.rotation_z = 0.0;
        drone.angular_velocity_x = 0.0;
        drone.angular_velocity_y = 0.0;
        drone.angular_velocity_z = 0.0;
        reached
    }

    fn position_drone_carry(&mut self, drone: &ObjectState, object_id: u64) {
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == object_id) {
            object.is_dragging = true;
            object.body.is_dragging = true;
            object.body.is_sleeping = false;
            object.body.velocity = Vector2::ZERO;
            let drone_center = object_center(drone);
            let swing_x = -drone.body.velocity.x * 0.075;
            let swing_drop = drone.body.velocity.x.abs() * 0.018;
            let cargo_center = drone_center
                + Vector2::new(
                    swing_x,
                    drone.body.height * 0.42 + object.body.height * 0.34 + swing_drop,
                );
            let target_rotation = (drone.body.velocity.x * -0.045).clamp(-12.0, 12.0) as f64;
            object.rotation_z = object.rotation_z * 0.84 + target_rotation * 0.16;
            object.body.position = Vector2::new(
                cargo_center.x - object.body.width * 0.5,
                cargo_center.y - object.body.height * 0.5,
            );
        }
    }

    fn drop_drone_carry(&mut self, drone_id: u64, release_velocity: Vector2) {
        let Some(carry) = self.drone_carries.remove(&drone_id) else {
            return;
        };
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == carry.object_id) {
            object.is_dragging = false;
            object.body.is_dragging = false;
            object.body.is_sleeping = false;
            object.body.velocity = release_velocity;
            object.body.gravity_scale = 1.0;
        }
        self.drone_drop_cooldowns.insert(
            drone_id,
            DroneDropCooldown {
                object_id: carry.object_id,
                dropped_at: self.frame_clock.elapsed_seconds,
            },
        );
    }

    fn drone_drop_target(&self) -> Vector2 {
        let bin = self.robot_bin_rect();
        Vector2::new(bin.x + bin.width * 0.5, bin.y - 225.0)
    }

    fn stabilize_quad_drones(&mut self) {
        let dragged_id = self.drag_controller.dragged_id();
        for object in self
            .scene
            .objects_mut()
            .iter_mut()
            .filter(|object| object.visual_kind == ObjectVisualKind::QuadDrone)
        {
            if dragged_id == Some(object.id) {
                continue;
            }
            object.body.gravity_scale = 0.0;
            object.body.is_dragging = true;
            object.body.is_sleeping = false;
            object.rotation_z = 0.0;
            object.angular_velocity_x = 0.0;
            object.angular_velocity_y = 0.0;
            object.angular_velocity_z = 0.0;
        }
    }

    fn update_portals(&mut self, dt: f32) {
        let now = self.frame_clock.elapsed_seconds;
        let bounds = self.scene_bounds();
        let teleports = self
            .portal_pair_tool
            .teleport_candidates(self.scene.objects(), bounds, dt, now);
        for teleport in teleports {
            let _ = self.scene.teleport_object(teleport.id, teleport.position, teleport.velocity);
        }
        self.portal_pair_tool
            .rebuild_render_cells(self.cursor_local, bounds, now);
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
        self.scene.clear_static_colliders();
        self.lasso_tool.deactivate();
        self.scene.clear_objects();
        self.drag_controller.cancel_drag(self.scene.objects_mut());
        self.basketball_game = BasketballGame::default();
        self.basketball_confetti.clear();
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

    fn toggle_basketball_game(&mut self) {
        if self.basketball_game.active {
            self.basketball_game = BasketballGame::default();
            self.basketball_confetti.clear();
            self.scene.clear_static_colliders();
            self.scene.reset(self.scene_bounds());
            self.selected_id = self.scene.objects().last().map(|object| object.id);
            self.push_status_message("Basketball game off.".to_string());
        } else {
            self.start_basketball_court();
        }
    }

    fn start_basketball_court(&mut self) {
        let bounds = self.scene_bounds();
        self.scene.clear_static_colliders();
        self.lasso_tool.deactivate();
        self.scene.clear_objects();
        self.basketball_confetti.clear();
        self.drag_controller.cancel_drag(self.scene.objects_mut());
        self.slingshot_game = SlingshotGame::default();
        self.selected_id = None;
        self.basketball_game = BasketballGame::new(bounds);
        self.build_basketball_court();
        self.push_status_message(
            "Basketball: grab the ball and flick it up toward the hoop. F3 resets, F12 exits.".to_string(),
        );
    }

    fn build_basketball_colliders(&mut self) {
        let bounds = self.scene_bounds();
        let geometry = hoop_geometry(BASKETBALL_HOOP_WIDTH, BASKETBALL_HOOP_HEIGHT);
        let hoop_center = self.basketball_game.hoop_center;
        let rim_center_y = hoop_center.y + geometry.rim_center_y;
        let rim_z = -BASKETBALL_HOOP_DEPTH + geometry.rim_center_z;

        // Backboard: a real slab the ball can bank off.
        self.scene.add_static_box_collider(
            (
                hoop_center.x,
                hoop_center.y + geometry.backboard_center_y,
                -BASKETBALL_HOOP_DEPTH + geometry.backboard_front_z - 8.0,
            ),
            (geometry.backboard_width * 0.5, geometry.backboard_height * 0.5, 8.0),
            0.3,
            0.62,
        );

        // Rim: a ring of static spheres approximating the torus, open in the middle.
        for index in 0..BASKETBALL_RIM_COLLIDER_COUNT {
            let angle = std::f32::consts::TAU * (index as f32 / BASKETBALL_RIM_COLLIDER_COUNT as f32);
            self.scene.add_static_sphere_collider(
                (
                    hoop_center.x + geometry.rim_radius * angle.cos(),
                    rim_center_y,
                    rim_z + geometry.rim_radius * angle.sin(),
                ),
                BASKETBALL_RIM_COLLIDER_RADIUS,
                0.5,
                0.45,
            );
        }

        // Invisible walls keeping rebounds inside the play space: one deep
        // behind the hoop, one just in front of the screen plane.
        self.scene.add_static_box_collider(
            (bounds.width * 0.5, bounds.height * 0.5, -BASKETBALL_HOOP_DEPTH - 360.0),
            (bounds.width, bounds.height, 24.0),
            0.4,
            0.5,
        );
        self.scene.add_static_box_collider(
            (bounds.width * 0.5, bounds.height * 0.5, 150.0),
            (bounds.width, bounds.height, 24.0),
            0.4,
            0.35,
        );
    }

    fn build_basketball_court(&mut self) {
        self.build_basketball_colliders();
        let hoop_center = self.basketball_game.hoop_center;
        let hoop_id = self.scene.spawn_custom_object(
            Vector2::new(
                hoop_center.x - BASKETBALL_HOOP_WIDTH * 0.5,
                hoop_center.y - BASKETBALL_HOOP_HEIGHT * 0.5,
            ),
            Vector2::new(BASKETBALL_HOOP_WIDTH, BASKETBALL_HOOP_HEIGHT),
            AppColor::from_rgb(252, 88, 38),
            ObjectVisualKind::BasketballHoop,
            CollisionShape::Box,
        );
        if let Some(hoop) = self.scene.objects_mut().iter_mut().find(|object| object.id == hoop_id) {
            hoop.body.is_dragging = true;
            hoop.is_dragging = true;
            hoop.body.gravity_scale = 0.0;
            hoop.body.velocity = Vector2::ZERO;
            hoop.body.collidable = false;
            hoop.depth_z = -BASKETBALL_HOOP_DEPTH;
        }
        self.basketball_game.hoop_id = Some(hoop_id);

        let tee = self.basketball_game.tee;
        let ball_id = self.scene.spawn_custom_object(
            Vector2::new(tee.x - BASKETBALL_SIZE * 0.5, tee.y - BASKETBALL_SIZE * 0.5),
            Vector2::new(BASKETBALL_SIZE, BASKETBALL_SIZE),
            AppColor::from_rgb(235, 122, 48),
            ObjectVisualKind::Basketball,
            CollisionShape::Circle,
        );
        if let Some(ball) = self.scene.objects_mut().iter_mut().find(|object| object.id == ball_id) {
            ball.body.is_dragging = true;
            ball.is_dragging = true;
            ball.body.restitution = 0.7;
            ball.body.friction = 0.6;
            ball.body.linear_damping = 0.996;
            ball.body.collision_scale = BASKETBALL_COLLISION_SCALE;
            ball.depth_unlocked = true;
        }
        self.basketball_game.ball_id = Some(ball_id);
        self.selected_id = Some(ball_id);
    }

    fn handle_basketball_mouse(&mut self, is_left_down: bool) {
        let Some(ball_id) = self.basketball_game.ball_id else {
            return;
        };

        if is_left_down && !self.was_left_down && self.basketball_game.ready && self.cursor_is_over_basketball(ball_id) {
            self.basketball_game.aiming = true;
            self.basketball_game.grab_offset = self
                .scene
                .objects()
                .iter()
                .find(|object| object.id == ball_id)
                .map(|ball| {
                    let center = Vector2::new(
                        ball.body.position.x + ball.body.width * 0.5,
                        ball.body.position.y + ball.body.height * 0.5,
                    );
                    self.cursor_local - center
                })
                .unwrap_or(Vector2::ZERO);
            self.basketball_tracker.clear();
            self.basketball_tracker
                .add_sample(self.cursor_local, self.frame_clock.elapsed_seconds);
        }

        if is_left_down && self.basketball_game.aiming {
            self.aim_basketball(ball_id);
        }

        if !is_left_down && self.was_left_down && self.basketball_game.aiming {
            self.launch_basketball(ball_id);
        }
    }

    fn cursor_is_over_basketball(&self, ball_id: u64) -> bool {
        self.scene
            .objects()
            .iter()
            .find(|object| object.id == ball_id)
            .map(|object| {
                let center = Vector2::new(
                    object.body.position.x + object.body.width * 0.5,
                    object.body.position.y + object.body.height * 0.5,
                );
                (self.cursor_local - center).length_squared() <= 80.0 * 80.0
            })
            .unwrap_or(false)
    }

    fn aim_basketball(&mut self, ball_id: u64) {
        let target_center = self.cursor_local - self.basketball_game.grab_offset;
        self.basketball_tracker
            .add_sample(self.cursor_local, self.frame_clock.elapsed_seconds);
        let Some(ball) = self.scene.objects_mut().iter_mut().find(|object| object.id == ball_id) else {
            return;
        };
        ball.body.position = target_center - Vector2::new(ball.body.width * 0.5, ball.body.height * 0.5);
        ball.body.velocity = Vector2::ZERO;
        ball.body.is_dragging = true;
        ball.is_dragging = true;
        ball.body.is_sleeping = false;
        ball.depth_z = 0.0;
        ball.depth_velocity = 0.0;
        ball.angular_velocity_x = 0.0;
        ball.angular_velocity_y = 0.0;
        ball.angular_velocity_z = 0.0;
    }

    fn launch_basketball(&mut self, ball_id: u64) {
        self.basketball_tracker
            .add_sample(self.cursor_local, self.frame_clock.elapsed_seconds);
        let flick = self.basketball_tracker.estimate_velocity(0.085, 1.0, 3200.0);
        self.basketball_tracker.clear();
        self.basketball_game.aiming = false;

        let up_speed = (-flick.y).clamp(0.0, BASKETBALL_MAX_UP_SPEED);
        let is_shot = up_speed >= BASKETBALL_MIN_SHOT_UP_SPEED;

        let Some(ball) = self.scene.objects_mut().iter_mut().find(|object| object.id == ball_id) else {
            return;
        };
        ball.body.is_dragging = false;
        ball.is_dragging = false;
        ball.body.is_sleeping = false;

        if is_shot {
            // Throw: the flick's upward speed sets the arc, and also drives the
            // ball backwards into the scene toward the hoop.
            ball.body.velocity = Vector2::new(
                flick.x.clamp(-BASKETBALL_MAX_SIDE_SPEED, BASKETBALL_MAX_SIDE_SPEED),
                -up_speed,
            );
            ball.depth_velocity = -(BASKETBALL_DEPTH_BASE_SPEED + up_speed * BASKETBALL_DEPTH_UP_FACTOR);
            self.basketball_game.shots += 1;
        } else {
            // Too gentle to count as a shot: just let the ball drop at the front.
            ball.body.velocity = Vector2::new(flick.x.clamp(-600.0, 600.0), flick.y.max(0.0));
            ball.depth_velocity = 0.0;
        }

        self.basketball_game.last_ball_y = ball.body.position.y + ball.body.height * 0.5;
        self.basketball_game.min_depth_this_shot = 0.0;
        self.basketball_game.ready = false;
        self.basketball_game.scored_this_shot = false;
        self.basketball_game.launched_at = self.frame_clock.elapsed_seconds;
    }

    fn update_basketball_game(&mut self, _dt: f32) {
        self.expire_basketball_confetti();

        if !self.basketball_game.active {
            return;
        }
        let Some(ball_id) = self.basketball_game.ball_id else {
            return;
        };
        if self.basketball_game.ready || self.basketball_game.aiming {
            return;
        }

        // Box3D simulates the flight and the rim/backboard contacts; here we
        // only detect the ball dropping through the rim circle.
        let geometry = hoop_geometry(BASKETBALL_HOOP_WIDTH, BASKETBALL_HOOP_HEIGHT);
        let hoop_center = self.basketball_game.hoop_center;
        let rim_center_y = hoop_center.y + geometry.rim_center_y;
        let rim_z = -BASKETBALL_HOOP_DEPTH + geometry.rim_center_z;

        let Some(ball) = self.scene.objects().iter().find(|object| object.id == ball_id) else {
            return;
        };
        let ball_center = Vector2::new(
            ball.body.position.x + ball.body.width * 0.5,
            ball.body.position.y + ball.body.height * 0.5,
        );
        let ball_depth = ball.depth_z;
        let ball_radius = ball.body.width * 0.5 * BASKETBALL_COLLISION_SCALE;
        let falling = ball.body.velocity.y > 0.0;
        let previous_y = self.basketball_game.last_ball_y;
        self.basketball_game.last_ball_y = ball_center.y;
        self.basketball_game.min_depth_this_shot = self.basketball_game.min_depth_this_shot.min(ball_depth);

        let crossed_rim_height = previous_y <= rim_center_y && ball_center.y > rim_center_y;
        if crossed_rim_height && falling && !self.basketball_game.scored_this_shot {
            let delta_x = ball_center.x - hoop_center.x;
            let delta_z = ball_depth - rim_z;
            let planar_distance = ((delta_x * delta_x) + (delta_z * delta_z)).sqrt();
            if planar_distance <= (geometry.rim_radius - ball_radius).max(8.0) + 12.0 {
                self.score_basketball(Vector2::new(hoop_center.x, rim_center_y), rim_z, geometry.backboard_front_z);
            }
        }

        if self.basketball_shot_is_over(ball_id) {
            self.reload_basketball(ball_id);
        }
    }

    fn score_basketball(&mut self, rim_center: Vector2, rim_z: f32, backboard_front_z: f32) {
        let now = self.frame_clock.elapsed_seconds;
        self.basketball_game.scored_this_shot = true;
        self.basketball_game.score += 1;
        self.basketball_game.last_score_at = now;

        let board_plane = -BASKETBALL_HOOP_DEPTH + backboard_front_z;
        let swish = self.basketball_game.min_depth_this_shot > board_plane + 50.0;
        self.push_status_message(if swish {
            "Swish! +1".to_string()
        } else {
            "Off the glass! +1".to_string()
        });
        self.spawn_basketball_confetti(rim_center, rim_z);
    }

    fn spawn_basketball_confetti(&mut self, origin: Vector2, depth: f32) {
        const CONFETTI_COUNT: usize = 14;
        const CONFETTI_PALETTE: [AppColor; 5] = [
            AppColor::from_rgb(255, 214, 82),
            AppColor::from_rgb(255, 120, 96),
            AppColor::from_rgb(120, 226, 160),
            AppColor::from_rgb(120, 190, 255),
            AppColor::from_rgb(226, 140, 255),
        ];
        let now = self.frame_clock.elapsed_seconds;
        for index in 0..CONFETTI_COUNT {
            let angle = std::f32::consts::TAU * (index as f32 / CONFETTI_COUNT as f32);
            let size = 13.0 + ((index % 3) as f32 * 4.0);
            let id = self.scene.spawn_custom_object(
                Vector2::new(
                    origin.x + angle.cos() * 26.0 - size * 0.5,
                    origin.y - 10.0 - size * 0.5,
                ),
                Vector2::new(size, size),
                CONFETTI_PALETTE[index % CONFETTI_PALETTE.len()],
                ObjectVisualKind::Cube,
                CollisionShape::Box,
            );
            if let Some(piece) = self.scene.objects_mut().iter_mut().find(|object| object.id == id) {
                piece.depth_unlocked = true;
                piece.depth_z = depth;
                piece.depth_velocity = angle.sin() * 190.0;
                piece.body.velocity = Vector2::new(angle.cos() * 300.0, -260.0 - ((index % 4) as f32 * 110.0));
                piece.body.restitution = 0.55;
                piece.body.friction = 0.5;
                piece.body.mass = 0.25;
            }
            self.basketball_confetti.push((id, now));
        }
    }

    fn expire_basketball_confetti(&mut self) {
        if self.basketball_confetti.is_empty() {
            return;
        }
        let now = self.frame_clock.elapsed_seconds;
        let expired: Vec<u64> = self
            .basketball_confetti
            .iter()
            .filter(|(_, spawned_at)| now - spawned_at > BASKETBALL_CONFETTI_LIFETIME_SECONDS)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            let _ = self.scene.remove_object(id);
        }
        self.basketball_confetti
            .retain(|(_, spawned_at)| now - spawned_at <= BASKETBALL_CONFETTI_LIFETIME_SECONDS);
    }

    fn basketball_shot_is_over(&self, ball_id: u64) -> bool {
        let Some(ball) = self.scene.objects().iter().find(|object| object.id == ball_id) else {
            return false;
        };
        let bounds = self.scene_bounds();
        let elapsed = self.frame_clock.elapsed_seconds - self.basketball_game.launched_at;
        elapsed > BASKETBALL_RELOAD_TIMEOUT_SECONDS
            || ball.body.is_sleeping
            || ball.body.position.x > bounds.right() + 160.0
            || ball.body.position.x < -160.0 - ball.body.width
    }

    fn reload_basketball(&mut self, ball_id: u64) {
        let tee = self.basketball_game.tee;
        let Some(ball) = self.scene.objects_mut().iter_mut().find(|object| object.id == ball_id) else {
            return;
        };
        ball.body.position = Vector2::new(tee.x - ball.body.width * 0.5, tee.y - ball.body.height * 0.5);
        ball.body.velocity = Vector2::ZERO;
        ball.body.is_dragging = true;
        ball.is_dragging = true;
        ball.body.is_sleeping = false;
        ball.depth_z = 0.0;
        ball.depth_velocity = 0.0;
        ball.rotation_x = 0.0;
        ball.rotation_y = 0.0;
        ball.rotation_z = 0.0;
        ball.angular_velocity_x = 0.0;
        ball.angular_velocity_y = 0.0;
        ball.angular_velocity_z = 0.0;
        self.basketball_game.ready = true;
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
        self.finish_window_capture(selected_id);
    }

    fn simulate_twitch_cheer(&mut self, payload: Option<&serde_json::Value>) {
        if payload.and_then(|value| value.get("width")).is_some() {
            self.set_spawn_monitor(payload);
        }
        let bits = payload
            .and_then(|value| value.get("bits"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(100)
            .clamp(1, 100_000) as u32;
        let anonymous = payload
            .and_then(|value| value.get("anonymous"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let donor = if anonymous {
            "MYSTERIOUS CHEERER".to_string()
        } else {
            payload
                .and_then(|value| value.get("donor"))
                .and_then(serde_json::Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or("GoblinFan42")
                .chars()
                .take(24)
                .collect()
        };
        let message = payload
            .and_then(|value| value.get("message"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("!!!");
        self.spawn_cheer_drop(bits, donor, message, anonymous);
    }

    fn spawn_cheer_drop(&mut self, bits: u32, donor: String, message: &str, anonymous: bool) {
        let bounds = self.spawn_bounds();
        let center = Vector2::new(
            bounds.x + bounds.width * 0.5,
            bounds.y + 82.0,
        );
        let count = bits.min(300) as usize;
        let crystal_value = bits.div_ceil(count as u32);
        let tier = crystal_value.ilog10().min(4);
        let color = cheer_tier_color(tier, anonymous);
        let size = (68.0 + tier as f32 * 8.0).clamp(68.0, 100.0);
        let excitement = message.chars().filter(|character| matches!(character, '!' | '?')).count().min(8) as f32;
        let mass_per_crystal = ((bits as f32 / 100.0) / count as f32).clamp(0.35, 10.0);
        let base_value = bits / count as u32;
        let remainder = bits % count as u32;

        let now = self.frame_clock.elapsed_seconds;
        self.pending_cheer_drops.push(PendingCheerDrop {
            donor: donor.clone(),
            bits,
            center,
            color,
            count,
            emitted: 0,
            size,
            tier,
            excitement,
            mass_per_crystal,
            base_value,
            remainder,
            started_at: now,
            emission_seconds: 4.0,
        });
        self.cheer_drop_effects.push(CheerDropEffect {
            donor: donor.clone(),
            bits,
            center,
            color,
            started_at: now,
            ends_at: now + 4.8,
            anonymous,
        });
        self.push_status_message(format!("{donor} cheered {bits} simulated Bits."));
    }

    fn update_cheer_drop_effects(&mut self, now: f64) {
        let mut emissions = Vec::new();
        for drop in &mut self.pending_cheer_drops {
            let progress = ((now - drop.started_at) / drop.emission_seconds).clamp(0.0, 1.0);
            let target = ((drop.count as f64 * progress).floor() as usize).max(usize::from(drop.emitted == 0));
            while drop.emitted < target.min(drop.count) {
                emissions.push((drop.clone(), drop.emitted));
                drop.emitted += 1;
            }
        }
        self.pending_cheer_drops.retain(|drop| drop.emitted < drop.count);
        for (drop, index) in emissions {
            self.emit_cheer_crystal(&drop, index);
        }

        self.cheer_drop_effects.retain(|effect| now < effect.ends_at);
        self.cheer_portals.clear();
        self.cheer_labels.clear();
        for effect in &self.cheer_drop_effects {
            let age = (now - effect.started_at).max(0.0) as f32;
            let remaining = (effect.ends_at - now).max(0.0) as f32;
            let open = (age / 0.24).clamp(0.0, 1.0);
            let close = (remaining / 0.55).clamp(0.0, 1.0);
            let intensity = open.min(close);
            self.cheer_portals.push(CheerPortalVisual {
                center: effect.center,
                radius: (70.0 + (effect.bits as f32).log10() * 16.0) * intensity.max(0.12),
                color: effect.color,
                intensity,
            });
            if age < 2.7 {
                let alpha = if age < 0.25 {
                    (age / 0.25 * 255.0) as u8
                } else {
                    ((2.7 - age).clamp(0.0, 0.7) / 0.7 * 255.0).min(255.0) as u8
                };
                let prefix = if effect.anonymous { "?" } else { "+" };
                self.cheer_labels.push(ScreenLabel {
                    position: Vector2::new(effect.center.x, effect.center.y + 34.0),
                    text: format!("{prefix} {}  {} BITS", effect.donor.to_uppercase(), effect.bits),
                    color: AppColor::from_argb(alpha, 248, 244, 255),
                    scale: 2,
                });
            }
        }
    }

    fn emit_cheer_crystal(&mut self, drop: &PendingCheerDrop, index: usize) {
        let phase = index as f32 * 2.399_963_1 + drop.bits as f32 * 0.017;
        let fan = if drop.count > 1 {
            index as f32 / (drop.count - 1) as f32 * 2.0 - 1.0
        } else {
            0.0
        };
        let position = Vector2::new(
            drop.center.x + phase.sin() * 34.0 - drop.size * 0.36,
            drop.center.y + 10.0 + phase.cos().abs() * 16.0,
        );
        let id = self.scene.spawn_custom_object(
            position,
            Vector2::new(drop.size * 0.72, drop.size),
            drop.color,
            ObjectVisualKind::BitCrystal,
            CollisionShape::Diamond,
        );
        if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == id) {
            object.body.mass = drop.mass_per_crystal;
            object.body.restitution = (0.48 + drop.tier as f32 * 0.055).clamp(0.48, 0.82);
            object.body.friction = 0.58;
            object.body.velocity = Vector2::new(
                fan * (360.0 + drop.excitement * 34.0) + phase.sin() * 170.0,
                170.0 + phase.cos().abs() * 190.0 + drop.excitement * 24.0,
            );
            object.rotation_y = phase as f64 * 35.0;
            object.angular_velocity_x = phase.cos() as f64 * 180.0;
            object.angular_velocity_y = phase.sin() as f64 * 240.0;
            object.angular_velocity_z = fan as f64 * 280.0;
            object.source_owner = Some(drop.donor.clone());
            object.source_value = Some(drop.base_value + u32::from((index as u32) < drop.remainder));
        }
        self.selected_id = Some(id);
    }

    fn finish_window_capture(&mut self, object_id: u64) {
        let previous = self.window_captures.remove(&object_id);
        let Some(target) = self.window_capture_candidate.take() else {
            if let Some(previous) = previous {
                let cursor_screen = Vector2::new(
                    self.cursor_local.x + self.bounds.x,
                    self.cursor_local.y + self.bounds.y,
                );
                if previous.client_rect_screen.contains(cursor_screen) {
                    let client_rect = previous.client_rect_screen;
                    self.window_captures.insert(object_id, previous);
                    self.constrain_object_to_window(object_id, client_rect, Vector2::ZERO);
                } else {
                    self.push_status_message("Released object from its window terrarium.".to_string());
                }
            }
            return;
        };

        let title = target.title.clone();
        let object_collidable = self
            .scene
            .objects()
            .iter()
            .find(|object| object.id == object_id)
            .map(|object| object.body.collidable)
            .unwrap_or(true);
        self.window_captures.insert(
            object_id,
            WindowCapture {
                window_id: target.id,
                title: title.clone(),
                client_rect_screen: target.client_rect,
                object_collidable,
            },
        );
        self.constrain_object_to_window(object_id, target.client_rect, Vector2::ZERO);
        self.push_status_message(format!("Captured object in {title}. Drag it outside a window to release."));
    }

    fn enforce_window_captures(&mut self) {
        let dragged_id = self.drag_controller.dragged_id();
        let captures: Vec<(u64, WindowCapture)> = self
            .window_captures
            .iter()
            .map(|(&object_id, capture)| (object_id, capture.clone()))
            .collect();
        let mut missing = Vec::new();

        for (object_id, capture) in captures {
            if dragged_id == Some(object_id) {
                continue;
            }
            let Some(target) = desktop_window_by_id(capture.window_id) else {
                missing.push((object_id, capture.title, capture.object_collidable));
                continue;
            };
            if !target.is_visible {
                if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == object_id) {
                    object.is_visible = false;
                    object.body.collidable = false;
                    object.body.velocity = Vector2::ZERO;
                }
                continue;
            }
            if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == object_id) {
                object.is_visible = true;
                object.body.collidable = capture.object_collidable;
            }
            let delta = Vector2::new(
                target.client_rect.x - capture.client_rect_screen.x,
                target.client_rect.y - capture.client_rect_screen.y,
            );
            self.constrain_object_to_window(object_id, target.client_rect, delta);
            if let Some(stored) = self.window_captures.get_mut(&object_id) {
                stored.client_rect_screen = target.client_rect;
                stored.title = target.title;
            }
        }

        for (object_id, title, object_collidable) in missing {
            self.window_captures.remove(&object_id);
            if let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == object_id) {
                object.is_visible = true;
                object.body.collidable = object_collidable;
            }
            self.push_status_message(format!("Released object because {title} was closed."));
        }
    }

    fn constrain_object_to_window(&mut self, object_id: u64, client_rect_screen: RectF, window_delta: Vector2) {
        let local_rect = self.screen_rect_to_local(client_rect_screen);
        let Some(object) = self.scene.objects().iter().find(|object| object.id == object_id) else {
            self.window_captures.remove(&object_id);
            return;
        };

        let mut position = object.body.position + window_delta;
        let mut velocity = object.body.velocity;
        let restitution = object.body.restitution;
        let max_x = (local_rect.right() - object.body.width).max(local_rect.left());
        let max_y = (local_rect.bottom() - object.body.height).max(local_rect.top());
        if position.x < local_rect.left() {
            position.x = local_rect.left();
            velocity.x = velocity.x.abs() * restitution;
        } else if position.x > max_x {
            position.x = max_x;
            velocity.x = -velocity.x.abs() * restitution;
        }
        if position.y < local_rect.top() {
            position.y = local_rect.top();
            velocity.y = velocity.y.abs() * restitution;
        } else if position.y > max_y {
            position.y = max_y;
            velocity.y = -velocity.y.abs() * restitution;
        }

        if position != object.body.position || velocity != object.body.velocity {
            let _ = self.scene.teleport_object(object_id, position, velocity);
        }
    }

    fn screen_rect_to_local(&self, rect: RectF) -> RectF {
        RectF::new(rect.x - self.bounds.x, rect.y - self.bounds.y, rect.width, rect.height)
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

        if self.debug_visible {
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
                    PanelLine {
                        text: "F12: basketball".to_string(),
                        selected: false,
                    },
                ],
                footer: Vec::new(),
            });

            let lines = vec![
                PanelLine {
                    text: format!("ForceInteractive: {}", if self.force_interactive_for_debug { "ON" } else { "off" }),
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

        if self.basketball_game.active {
            let just_scored = self.basketball_game.score > 0
                && self.frame_clock.elapsed_seconds - self.basketball_game.last_score_at < 1.6;
            panels.push(OverlayPanel {
                title: "Basketball".to_string(),
                lines: vec![
                    PanelLine {
                        text: if just_scored {
                            format!("Score: {}  BUCKET!", self.basketball_game.score)
                        } else {
                            format!("Score: {}", self.basketball_game.score)
                        },
                        selected: just_scored,
                    },
                    PanelLine {
                        text: format!("Shots: {}", self.basketball_game.shots),
                        selected: false,
                    },
                    PanelLine {
                        text: if self.basketball_game.aiming {
                            "Flick up to shoot!".to_string()
                        } else if self.basketball_game.ready {
                            "Grab the ball, flick it up.".to_string()
                        } else {
                            "Ball in flight.".to_string()
                        },
                        selected: self.basketball_game.aiming,
                    },
                ],
                footer: vec!["F3 reset. F12 exit.".to_string()],
            });
        }

        if self.sand_world.active {
            panels.push(OverlayPanel {
                title: "Sand".to_string(),
                lines: vec![
                    PanelLine {
                        text: format!("Cells: {}", self.sand_world.occupied_count()),
                        selected: false,
                    },
                    PanelLine {
                        text: "Left pours. Right erases.".to_string(),
                        selected: false,
                    },
                ],
                footer: vec!["F6 toggle. F3 clear.".to_string()],
            });
        }

        if self.weather_world.active {
            panels.push(OverlayPanel {
                title: "Rain".to_string(),
                lines: vec![PanelLine {
                    text: format!("Drops: {}", self.weather_world.drop_count()),
                    selected: false,
                }],
                footer: vec!["F5 toggle.".to_string()],
            });
        }

        if self.measure_tool.active {
            let lines = if self.measure_tool.has_measurement {
                vec![
                    PanelLine {
                        text: format!("Distance: {:.1} px", self.measure_tool.distance()),
                        selected: self.measure_tool.dragging,
                    },
                    PanelLine {
                        text: format!(
                            "Delta: {:+.1}, {:+.1}",
                            self.measure_tool.delta().x,
                            self.measure_tool.delta().y
                        ),
                        selected: false,
                    },
                    PanelLine {
                        text: format!("Angle: {:.1} deg", self.measure_tool.angle_degrees()),
                        selected: false,
                    },
                ]
            } else {
                vec![PanelLine {
                    text: "Drag to measure.".to_string(),
                    selected: false,
                }]
            };
            panels.push(OverlayPanel {
                title: "Measure".to_string(),
                lines,
                footer: vec!["Right clears. Click-through on.".to_string()],
            });
        }

        if self.spotlight_tool.active {
            panels.push(OverlayPanel {
                title: "Spotlight".to_string(),
                lines: vec![PanelLine {
                    text: "Following cursor.".to_string(),
                    selected: false,
                }],
                footer: vec!["Click-through on. Tray/L toggle.".to_string()],
            });
        }

        if self.lasso_tool.active {
            let lines = if self.lasso_tool.drawing {
                vec![
                    PanelLine {
                        text: format!("Loop points: {}", self.lasso_tool.path_len()),
                        selected: true,
                    },
                    PanelLine {
                        text: "Release to snare objects.".to_string(),
                        selected: false,
                    },
                ]
            } else if self.lasso_tool.has_capture() {
                vec![
                    PanelLine {
                        text: format!("Snared: {}", self.lasso_tool.captured_count()),
                        selected: true,
                    },
                    PanelLine {
                        text: "Move cursor to whip.".to_string(),
                        selected: false,
                    },
                ]
            } else {
                vec![PanelLine {
                    text: "Drag a loop around objects.".to_string(),
                    selected: false,
                }]
            };
            panels.push(OverlayPanel {
                title: "Rope Lasso".to_string(),
                lines,
                footer: vec!["Right releases. R toggles.".to_string()],
            });
        }

        if self.portal_pair_tool.needs_interactive() || self.portal_pair_tool.has_portal_pair() {
            let status = if self.portal_pair_tool.needs_interactive() && self.portal_pair_tool.first.is_none() {
                "Click a screen edge for Portal A."
            } else if self.portal_pair_tool.needs_interactive() {
                "Click another edge for Portal B."
            } else {
                "Linked and armed."
            };
            panels.push(OverlayPanel {
                title: "Portal Pair".to_string(),
                lines: vec![PanelLine {
                    text: status.to_string(),
                    selected: self.portal_pair_tool.needs_interactive(),
                }],
                footer: vec!["Right cancels. Toggle clears.".to_string()],
            });
        }

        if self.shatter_gun.active {
            panels.push(OverlayPanel {
                title: "Shatter Gun".to_string(),
                lines: vec![
                    PanelLine {
                        text: format!("Shots: {}", self.shatter_gun.shots_fired),
                        selected: self.frame_clock.elapsed_seconds - self.shatter_gun.last_fire_at < 0.45,
                    },
                    PanelLine {
                        text: "Click to fracture the screen.".to_string(),
                        selected: false,
                    },
                ],
                footer: vec!["B toggles. Right holsters.".to_string()],
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
            center_message: if self.frame_clock.elapsed_seconds < self.snail_death_until_seconds {
                Some("YOU DIED".to_string())
            } else {
                None
            },
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
            shatter_backdrop_active: self.shatter_backdrop_active,
            sand_cells: self.sand_world.render_cells(),
            weather_cells: self.weather_world.render_cells(),
            measure_cells: self.measure_tool.render_cells(),
            spotlight_cells: self.spotlight_tool.render_cells(),
            lasso_cells: self.lasso_tool.render_cells(),
            portal_cells: self.portal_pair_tool.render_cells(),
            shatter_gun_cells: self.shatter_gun.render_cells(),
            window_capture_guide: self
                .window_capture_candidate
                .as_ref()
                .map(|target| self.screen_rect_to_local(target.client_rect)),
            cheer_portals: &self.cheer_portals,
            screen_labels: &self.cheer_labels,
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

    fn show_control_ui(&mut self) {
        if request_control_ui_show().is_ok() {
            self.push_status_message("Control UI brought forward.".to_string());
            return;
        }

        if let Some(mut child) = self.control_ui_process.take() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.push_status_message(format!("Control UI exited with {status}; relaunching."));
                },
                Ok(None) => {
                    self.control_ui_process = Some(child);
                    match request_control_ui_show() {
                        Ok(()) => self.push_status_message("Control UI brought forward.".to_string()),
                        Err(error) => {
                            self.push_status_message(format!("Control UI is already running; show request failed: {error}."));
                        },
                    }
                    return;
                },
                Err(error) => {
                    self.push_status_message(format!("Control UI status check failed: {error}; relaunching."));
                },
            }
        }

        if let Some(control_ui_exe) = find_packaged_control_ui_exe() {
            match spawn_control_ui_exe(&control_ui_exe) {
                Ok(child) => {
                    self.control_ui_process = Some(child);
                    self.push_status_message("Packaged Control UI launched from tray.".to_string());
                },
                Err(error) => {
                    self.push_status_message(format!("Packaged Control UI launch failed: {error:#}."));
                },
            }
            return;
        }

        let Some(control_ui_dir) = find_control_ui_dir() else {
            self.push_status_message("Control UI folder not found.".to_string());
            return;
        };

        match spawn_control_ui(&control_ui_dir) {
            Ok(child) => {
                self.control_ui_process = Some(child);
                self.push_status_message("Control UI launched from tray.".to_string());
            },
            Err(error) => {
                self.push_status_message(format!("Control UI launch failed: {error:#}."));
            },
        }
    }

    fn handle_action(&mut self, action: AppAction, event_loop: &ActiveEventLoop) {
        match action {
            AppAction::ShowControlUi => {
                self.show_control_ui();
            },
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
            AppAction::ToggleBasketballGame => {
                self.toggle_basketball_game();
            },
            AppAction::Reset => {
                self.reset_everything();
            },
            AppAction::ToggleSettings => {
                self.settings_panel.visible = !self.settings_panel.visible;
                if self.settings_panel.visible {
                    self.import_panel = None;
                }
            },
            AppAction::ToggleWeather => {
                self.toggle_weather_world();
            },
            AppAction::ToggleSand => {
                self.toggle_sand_world();
            },
            AppAction::ToggleMeasureTool => {
                self.toggle_measure_tool();
            },
            AppAction::ToggleSpotlight => {
                self.toggle_spotlight();
            },
            AppAction::ToggleLassoTool => {
                self.toggle_lasso_tool();
            },
            AppAction::ToggleShatterGun => {
                self.toggle_shatter_gun();
            },
            AppAction::ShatterScreen => {
                self.trigger_screen_shatter();
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

    fn reset_everything(&mut self) {
        self.drag_controller.cancel_drag(self.scene.objects_mut());
        self.scene.clear_static_colliders();
        self.scene.reset(self.scene_bounds());
        self.selected_id = None;
        self.slingshot_game = SlingshotGame::default();
        self.basketball_game = BasketballGame::default();
        self.basketball_tracker.clear();
        self.basketball_confetti.clear();
        self.weather_world.clear();
        self.sand_world.clear();
        self.measure_tool.clear();
        self.spotlight_tool.clear();
        self.lasso_tool.clear();
        self.portal_pair_tool.clear();
        self.shatter_gun.clear();
        self.shatter_backdrop_active = false;
        self.robot_carries.clear();
        self.robot_drop_cooldowns.clear();
        self.robot_bin_ids.clear();
        self.fan_yaws.clear();
        self.drone_carries.clear();
        self.drone_drop_cooldowns.clear();
        self.import_panel = None;
        self.settings_panel.visible = false;
        self.is_rotation_dragging = false;
        self.last_rotation_cursor = Vector2::ZERO;
        self.push_status_message("Reset everything.".to_string());
    }

    fn handle_keyboard(&mut self, key_code: KeyCode, state: ElementState, event_loop: &ActiveEventLoop) {
        if state != ElementState::Pressed {
            return;
        }

        if self.keyboard_modifiers.control_key() && matches!(key_code, KeyCode::KeyC | KeyCode::KeyQ) {
            self.handle_action(AppAction::Exit, event_loop);
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
            KeyCode::F5 => self.handle_action(AppAction::ToggleWeather, event_loop),
            KeyCode::F6 => self.handle_action(AppAction::ToggleSand, event_loop),
            KeyCode::F7 => self.handle_action(AppAction::SpawnCrystal, event_loop),
            KeyCode::F8 => self.handle_action(AppAction::SpawnDvdLogo, event_loop),
            KeyCode::F9 => self.handle_action(AppAction::SpawnStressCubes, event_loop),
            KeyCode::F10 => self.handle_action(AppAction::ToggleSlingshotGame, event_loop),
            KeyCode::F11 => self.handle_action(AppAction::SpawnRobotBuddy, event_loop),
            KeyCode::F12 => self.handle_action(AppAction::ToggleBasketballGame, event_loop),
            KeyCode::KeyM => self.handle_action(AppAction::ToggleMeasureTool, event_loop),
            KeyCode::KeyL => self.handle_action(AppAction::ToggleSpotlight, event_loop),
            KeyCode::KeyR => self.handle_action(AppAction::ToggleLassoTool, event_loop),
            KeyCode::KeyB => self.handle_action(AppAction::ToggleShatterGun, event_loop),
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
        let bounds = self.spawn_bounds();
        Vector2::new(bounds.x + bounds.width * 0.5, bounds.y + bounds.height * 0.5)
    }

    fn spawn_bounds(&self) -> RectF {
        self.spawn_monitor_bounds.unwrap_or_else(|| self.scene_bounds())
    }

    fn set_spawn_monitor(&mut self, payload: Option<&serde_json::Value>) {
        let Some(payload) = payload else {
            return;
        };
        let desktop = self.scene_bounds();
        let screen_x = payload_f32(payload, "x", self.bounds.x, -100_000.0, 100_000.0);
        let screen_y = payload_f32(payload, "y", self.bounds.y, -100_000.0, 100_000.0);
        let width = payload_f32(payload, "width", desktop.width, 1.0, desktop.width);
        let height = payload_f32(payload, "height", desktop.height, 1.0, desktop.height);
        let local = RectF::new(screen_x - self.bounds.x, screen_y - self.bounds.y, width, height);
        let left = local.x.clamp(0.0, desktop.right() - 1.0);
        let top = local.y.clamp(0.0, desktop.bottom() - 1.0);
        let right = local.right().clamp(left + 1.0, desktop.right());
        let bottom = local.bottom().clamp(top + 1.0, desktop.bottom());
        self.spawn_monitor_bounds = Some(RectF::new(left, top, right - left, bottom - top));
        self.push_status_message("Control UI changed the primary spawn monitor.".to_string());
    }
}

fn desktop_bounds(event_loop: &ActiveEventLoop) -> Option<RectF> {
    let mut monitors = event_loop.available_monitors();
    let first = monitors.next()?;
    let first_position = first.position();
    let first_size = first.size();
    let mut left = first_position.x;
    let mut top = first_position.y;
    let mut right = first_position.x + first_size.width as i32;
    let mut bottom = first_position.y + first_size.height as i32;

    for monitor in monitors {
        let position = monitor.position();
        let size = monitor.size();
        left = left.min(position.x);
        top = top.min(position.y);
        right = right.max(position.x + size.width as i32);
        bottom = bottom.max(position.y + size.height as i32);
    }

    Some(RectF::new(
        left as f32,
        top as f32,
        (right - left).max(1) as f32,
        (bottom - top).max(1) as f32,
    ))
}

const CONTROL_IPC_ADDR: &str = "127.0.0.1:47731";
const CONTROL_UI_WINDOW_IPC_ADDR: &str = "127.0.0.1:47732";
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

fn control_visual_kind(kind: &str) -> Option<(ObjectVisualKind, Option<AppColor>, &'static str)> {
    let normalized = kind
        .chars()
        .filter(|character| !matches!(character, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect::<String>();

    match normalized.as_str() {
        "cube" => Some((ObjectVisualKind::Cube, None, "cube")),
        "dice" => Some((ObjectVisualKind::Dice, Some(AppColor::from_rgb(245, 245, 240)), "dice")),
        "crystal" => Some((ObjectVisualKind::Crystal, Some(AppColor::from_rgb(108, 241, 255)), "crystal")),
        "satellite" => Some((ObjectVisualKind::Satellite, Some(AppColor::from_rgb(88, 160, 255)), "satellite")),
        "dvd" | "dvdlogo" => Some((ObjectVisualKind::DvdLogo, Some(AppColor::from_rgb(244, 78, 255)), "DVD logo")),
        "ball" => Some((ObjectVisualKind::Ball, Some(AppColor::from_rgb(90, 205, 255)), "ball")),
        "softball" => Some((ObjectVisualKind::SoftBall, Some(AppColor::from_rgb(106, 236, 188)), "soft ball")),
        "glass" | "glassmarble" => Some((ObjectVisualKind::GlassMarble, Some(AppColor::from_rgb(220, 246, 255)), "glass marble")),
        "plasma" | "plasmaorb" => Some((ObjectVisualKind::PlasmaOrb, Some(AppColor::from_rgb(160, 88, 255)), "plasma orb")),
        "portal" | "portalorb" => Some((ObjectVisualKind::PortalOrb, Some(AppColor::from_rgb(80, 180, 255)), "portal orb")),
        "bubble" | "soapbubble" => Some((ObjectVisualKind::SoapBubble, Some(AppColor::from_rgb(245, 255, 255)), "soap bubble")),
        "shield" | "forcefield" | "forcefieldorb" => {
            Some((ObjectVisualKind::ForcefieldOrb, Some(AppColor::from_rgb(78, 240, 255)), "forcefield orb"))
        },
        "raycube" | "raymarchcube" => Some((ObjectVisualKind::RaymarchCube, Some(AppColor::from_rgb(130, 92, 255)), "raymarch cube")),
        "pyramid" => Some((ObjectVisualKind::Pyramid, Some(AppColor::from_rgb(255, 176, 92)), "pyramid")),
        "barrel" => Some((ObjectVisualKind::Barrel, Some(AppColor::from_rgb(126, 226, 168)), "barrel")),
        "ring" => Some((ObjectVisualKind::Ring, Some(AppColor::from_rgb(255, 118, 210)), "ring")),
        "star" => Some((ObjectVisualKind::Star, Some(AppColor::from_rgb(255, 224, 92)), "star")),
        "plank" | "gameplank" => Some((ObjectVisualKind::GamePlank, Some(AppColor::from_rgb(255, 176, 92)), "game plank")),
        "target" | "gametarget" => Some((ObjectVisualKind::GameTarget, Some(AppColor::from_rgb(255, 224, 92)), "target")),
        "fox" | "foxbuddy" => Some((ObjectVisualKind::FoxBuddy, Some(AppColor::from_rgb(255, 160, 90)), "fox buddy")),
        "robot" | "robotbuddy" => Some((ObjectVisualKind::RobotBuddy, Some(AppColor::from_rgb(150, 220, 245)), "robot buddy")),
        "snail" => Some((ObjectVisualKind::Snail, Some(AppColor::from_rgb(166, 214, 124)), "snail")),
        "fan" => Some((ObjectVisualKind::Fan, Some(AppColor::from_rgb(105, 230, 255)), "fan")),
        "drone" | "quaddrone" | "quadcopter" | "quadcopterdrone" => {
            Some((ObjectVisualKind::QuadDrone, Some(AppColor::from_rgb(248, 250, 252)), "quadcopter drone"))
        },
        "basketball" => Some((ObjectVisualKind::Basketball, Some(AppColor::from_rgb(232, 113, 38)), "basketball")),
        "hoop" | "basketballhoop" => Some((ObjectVisualKind::BasketballHoop, Some(AppColor::from_rgb(245, 245, 245)), "basketball hoop")),
        _ => None,
    }
}

fn payload_f32(payload: &serde_json::Value, key: &str, fallback: f32, min: f32, max: f32) -> f32 {
    payload
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .map(|value| (value as f32).clamp(min, max))
        .unwrap_or(fallback)
}

fn cheer_tier_color(tier: u32, anonymous: bool) -> AppColor {
    if anonymous {
        return AppColor::from_rgb(54, 34, 82);
    }
    match tier {
        0 => AppColor::from_rgb(150, 92, 255),
        1 => AppColor::from_rgb(68, 214, 255),
        2 => AppColor::from_rgb(235, 74, 255),
        3 => AppColor::from_rgb(255, 78, 112),
        _ => AppColor::from_rgb(255, 204, 72),
    }
}

#[derive(Debug, Deserialize)]
struct ControlIpcCommand {
    command: String,
    payload: Option<serde_json::Value>,
}

fn start_control_ipc_server() -> Result<Receiver<ControlIpcCommand>> {
    let listener = TcpListener::bind(CONTROL_IPC_ADDR)
        .with_context(|| format!("failed to bind {CONTROL_IPC_ADDR}"))?;
    let (sender, receiver) = mpsc::channel();

    thread::Builder::new()
        .name("control-ui-ipc".to_string())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => handle_control_ipc_stream(stream, &sender),
                    Err(error) => eprintln!("Control UI IPC accept failed: {error}"),
                }
            }
        })
        .context("failed to spawn Control UI IPC thread")?;

    Ok(receiver)
}

fn find_control_ui_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(current_dir) = env::current_dir() {
        candidates.push(current_dir.join("control-ui"));
        candidates.push(current_dir.join("..").join("control-ui"));
        candidates.push(current_dir.join("..").join("..").join("control-ui"));
    }

    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("control-ui"));

    if let Ok(current_exe) = env::current_exe() {
        for ancestor in current_exe.ancestors() {
            candidates.push(ancestor.join("control-ui"));
        }
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.join("package.json").is_file())
}

fn find_packaged_control_ui_exe() -> Option<PathBuf> {
    const CONTROL_UI_EXE_NAMES: [&str; 3] = [
        "screen-overlay-control.exe",
        "ScreenOverlayPhysics.exe",
        "ScreenOverlayPhysics Overlay UI.exe",
    ];

    let mut candidates = Vec::new();
    if let Ok(current_exe) = env::current_exe() {
        for ancestor in current_exe.ancestors() {
            for name in CONTROL_UI_EXE_NAMES {
                candidates.push(ancestor.join(name));
            }
        }
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file() && env::current_exe().map(|current| current != *candidate).unwrap_or(true))
}

fn spawn_control_ui_exe(control_ui_exe: &Path) -> Result<Child> {
    let mut command = Command::new(control_ui_exe);
    configure_hidden_process(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to start packaged Control UI at {}", control_ui_exe.display()))
}

fn spawn_control_ui(control_ui_dir: &PathBuf) -> Result<Child> {
    let mut command = control_ui_command();
    configure_hidden_process(&mut command);
    command
        .current_dir(control_ui_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    command
        .spawn()
        .with_context(|| format!("failed to start Control UI in {}", control_ui_dir.display()))
}

fn request_control_ui_show() -> Result<()> {
    let addr: SocketAddr = CONTROL_UI_WINDOW_IPC_ADDR
        .parse()
        .map_err(|error| anyhow::anyhow!("bad Control UI window endpoint: {error}"))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(180))
        .map_err(|error| anyhow::anyhow!("connect failed: {error}"))?;
    let _ = stream.set_write_timeout(Some(Duration::from_millis(180)));
    writeln!(stream, "show").context("failed to send Control UI show request")?;
    stream.flush().context("failed to flush Control UI show request")
}

#[cfg(target_os = "windows")]
fn control_ui_command() -> Command {
    let mut command = Command::new("cmd");
    command.args(["/C", "npm", "run", "tauri:dev"]);
    command
}

#[cfg(not(target_os = "windows"))]
fn control_ui_command() -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", "npm run tauri:dev"]);
    command
}

fn configure_hidden_process(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn handle_control_ipc_stream(stream: TcpStream, sender: &Sender<ControlIpcCommand>) {
    let mut writer = match stream.try_clone() {
        Ok(writer) => writer,
        Err(error) => {
            eprintln!("Control UI IPC clone failed: {error}");
            return;
        },
    };
    let _ = writer.set_write_timeout(Some(Duration::from_millis(500)));

    let response = match read_control_ipc_command(stream, sender) {
        Ok(()) => b"{\"ok\":true}\n".as_slice(),
        Err(error) => {
            eprintln!("Control UI IPC command failed: {error:#}");
            b"{\"ok\":false}\n".as_slice()
        },
    };
    let _ = writer.write_all(response);
}

fn read_control_ipc_command(stream: TcpStream, sender: &Sender<ControlIpcCommand>) -> Result<()> {
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .context("failed to set control IPC read timeout")?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .context("failed to read control IPC command")?;
    let command: ControlIpcCommand =
        serde_json::from_str(line.trim()).context("failed to parse control IPC command")?;
    sender
        .send(command)
        .context("failed to queue control IPC command")?;
    Ok(())
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
    @location(2) material: vec4<f32>,
    @location(3) material_extra: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) material: vec4<f32>,
    @location(2) material_extra: vec4<f32>,
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
    out.material = input.material;
    out.material_extra = input.material_extra;
    return out;
}

fn glass_hash(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(17.31, 41.17, 73.13))) * 43758.5453);
}

fn wglnoise_mod289_3(x: vec3<f32>) -> vec3<f32> {
    return x - floor(x / 289.0) * 289.0;
}

fn wglnoise_mod289_4(x: vec4<f32>) -> vec4<f32> {
    return x - floor(x / 289.0) * 289.0;
}

fn wglnoise_permute_4(x: vec4<f32>) -> vec4<f32> {
    return wglnoise_mod289_4((x * 34.0 + vec4<f32>(10.0)) * x);
}

fn simplex_noise3(v: vec3<f32>) -> f32 {
    var i = floor(v + vec3<f32>(dot(v, vec3<f32>(1.0 / 3.0))));
    let x0 = v - i + vec3<f32>(dot(i, vec3<f32>(1.0 / 6.0)));

    let g = select(vec3<f32>(0.0), vec3<f32>(1.0), x0.yzx <= x0.xyz);
    let l = vec3<f32>(1.0) - g;
    let i1 = min(g.xyz, l.zxy);
    let i2 = max(g.xyz, l.zxy);

    let x1 = x0 - i1 + vec3<f32>(1.0 / 6.0);
    let x2 = x0 - i2 + vec3<f32>(1.0 / 3.0);
    let x3 = x0 - vec3<f32>(0.5);

    i = wglnoise_mod289_3(i);
    var perm = wglnoise_permute_4(i.z + vec4<f32>(0.0, i1.z, i2.z, 1.0));
    perm = wglnoise_permute_4(perm + i.y + vec4<f32>(0.0, i1.y, i2.y, 1.0));
    perm = wglnoise_permute_4(perm + i.x + vec4<f32>(0.0, i1.x, i2.x, 1.0));

    var gx = vec4<f32>(-1.0) + fract(perm / 7.0) * 2.0;
    var gy = vec4<f32>(-1.0) + fract(floor(perm / 7.0) / 7.0) * 2.0;
    let gz = vec4<f32>(1.0) - abs(gx) - abs(gy);
    let zn = gz < vec4<f32>(0.0);
    let gx_adjust = select(vec4<f32>(-1.0), vec4<f32>(1.0), gx < vec4<f32>(0.0));
    let gy_adjust = select(vec4<f32>(-1.0), vec4<f32>(1.0), gy < vec4<f32>(0.0));
    gx = gx + select(vec4<f32>(0.0), gx_adjust, zn);
    gy = gy + select(vec4<f32>(0.0), gy_adjust, zn);

    let g0 = normalize(vec3<f32>(gx.x, gy.x, gz.x));
    let g1 = normalize(vec3<f32>(gx.y, gy.y, gz.y));
    let g2 = normalize(vec3<f32>(gx.z, gy.z, gz.z));
    let g3 = normalize(vec3<f32>(gx.w, gy.w, gz.w));

    let px = vec4<f32>(dot(g0, x0), dot(g1, x1), dot(g2, x2), dot(g3, x3));
    var m = max(vec4<f32>(0.5) - vec4<f32>(dot(x0, x0), dot(x1, x1), dot(x2, x2), dot(x3, x3)), vec4<f32>(0.0));
    let m3 = m * m * m;
    let m4 = m * m3;
    return 107.0 * dot(m4, px);
}

fn fbm_noise3(seed: vec3<f32>) -> f32 {
    var p = seed;
    var value = 0.0;
    var amplitude = 0.52;
    for (var octave: i32 = 0; octave < 4; octave = octave + 1) {
        value += amplitude * (simplex_noise3(p) * 0.5 + 0.5);
        p = p * 2.03 + vec3<f32>(13.1, -7.7, 5.3);
        amplitude *= 0.50;
    }
    return clamp(value, 0.0, 1.0);
}

fn marble_stripes(value: f32, frequency: f32) -> f32 {
    let t = 0.5 + 0.5 * sin(frequency * 6.2831853 * value);
    return t * t;
}

fn sphere_spot(center: vec3<f32>, radius: f32, feather: f32, p: vec3<f32>) -> f32 {
    return 1.0 - smoothstep(radius, radius + feather, length(p - center));
}

fn line_mask(width: f32, feather: f32, value: f32) -> f32 {
    return 1.0 - smoothstep(width, width + feather, abs(value));
}

fn glass_environment(dir: vec3<f32>) -> vec3<f32> {
    let d = normalize(dir);
    let sky = mix(vec3<f32>(0.08, 0.11, 0.14), vec3<f32>(0.55, 0.82, 1.0), clamp(d.y * 0.5 + 0.5, 0.0, 1.0));
    let horizon = pow(1.0 - abs(d.y), 4.0);
    let window = pow(max(dot(d, normalize(vec3<f32>(-0.58, -0.44, 0.69))), 0.0), 90.0);
    return sky + vec3<f32>(1.0, 0.70, 0.38) * horizon * 0.14 + vec3<f32>(0.38, 0.90, 1.0) * window * 0.72;
}

fn display_tonemap(color: vec3<f32>) -> vec3<f32> {
    let safe = max(color, vec3<f32>(0.0));
    let peak = max(safe.r, max(safe.g, safe.b));
    let over = max(peak - 0.82, 0.0);
    let scale = 1.0 / (1.0 + over * 1.10);
    return clamp(safe * scale, vec3<f32>(0.0), vec3<f32>(0.98));
}

fn emissive_grade(color: vec3<f32>) -> vec3<f32> {
    let mapped = display_tonemap(color);
    let luma = dot(mapped, vec3<f32>(0.2126, 0.7152, 0.0722));
    let saturated = mix(vec3<f32>(luma), mapped, 1.32);
    return clamp(saturated * 1.10 + vec3<f32>(0.018), vec3<f32>(0.0), vec3<f32>(0.98));
}

fn shade_glass_marble(input: VertexOutput) -> vec4<f32> {
    let p = normalize(input.material.yzw);
    let normal = normalize(input.material_extra.xyz);
    let view_dir = vec3<f32>(0.0, 0.0, 1.0);
    let light_dir = normalize(vec3<f32>(-0.46, -0.62, 0.64));
    let n_dot_v = clamp(dot(normal, view_dir), 0.0, 1.0);
    let n_dot_l = max(dot(normal, light_dir), 0.0);
    let fresnel = pow(1.0 - n_dot_v, 5.0);
    let rim = pow(clamp(1.0 - abs(n_dot_v), 0.0, 1.0), 2.3);

    let marble_p = p * 1.22 + vec3<f32>(0.28, -0.57, 0.13);
    let warp = fbm_noise3(marble_p * 1.45);
    let warp_fine = fbm_noise3(marble_p * 4.6 + vec3<f32>(warp * 2.6, -warp * 1.4, 1.9));
    let vein_axis = p.x * 0.88 + p.y * 0.24 - p.z * 0.58 + warp * 1.82 + warp_fine * 0.44;
    let stripe = marble_stripes(vein_axis, 1.55);
    let vein_soft = smoothstep(0.48, 0.84, stripe);
    let vein_hair = smoothstep(0.86, 0.985, stripe) * (0.45 + warp_fine * 0.55);
    let veins = clamp(vein_soft * 0.50 + vein_hair * 0.72, 0.0, 1.0);
    let cloud = fbm_noise3(p * 5.8 + vec3<f32>(10.4, -3.1, 1.7));

    var core = mix(vec3<f32>(0.78, 0.97, 1.0), vec3<f32>(0.10, 0.42, 0.68), veins * 0.58);
    core = mix(core, vec3<f32>(0.96, 1.0, 0.97), 0.20 + cloud * 0.12);

    let ribbon_curve = p.x * 0.54 - p.z * 0.24 + sin(p.y * 5.3 + warp * 3.0) * 0.11;
    let ribbon_window = smoothstep(-0.62, -0.34, p.y) * (1.0 - smoothstep(0.34, 0.62, p.y));
    let ribbon_depth = smoothstep(-0.18, 0.72, p.z) * (1.0 - smoothstep(0.88, 1.0, p.z));
    let ribbon = line_mask(0.026, 0.045, ribbon_curve) * ribbon_window * ribbon_depth;
    let ribbon_core = line_mask(0.007, 0.018, ribbon_curve) * ribbon_window * ribbon_depth;
    let ribbon_shadow = line_mask(0.062, 0.070, ribbon_curve + 0.026) * ribbon_window * ribbon_depth;
    let ribbon_color = mix(vec3<f32>(1.0, 0.18, 0.05), vec3<f32>(0.05, 0.48, 1.0), smoothstep(-0.12, 0.58, p.y + warp * 0.20));

    let bubble1 = sphere_spot(vec3<f32>(-0.34, -0.16, 0.44), 0.036, 0.028, p);
    let bubble2 = sphere_spot(vec3<f32>(0.26, 0.22, 0.30), 0.026, 0.022, p);
    let bubble3 = sphere_spot(vec3<f32>(0.08, -0.40, 0.50), 0.020, 0.018, p);
    let bubbles = clamp(bubble1 + bubble2 + bubble3, 0.0, 1.0);

    let reflect_dir = reflect(-view_dir, normal);
    let refract_dir = normalize(refract(-view_dir, normal, 1.0 / 1.45) + vec3<f32>((warp - 0.5) * 0.12, (warp_fine - 0.5) * 0.08, 0.0));
    let reflection = glass_environment(reflect_dir);
    let refraction = glass_environment(refract_dir);
    let half_dir = normalize(light_dir + view_dir);
    let specular = pow(max(dot(normal, half_dir), 0.0), 110.0);
    let sharp_glint = pow(max(dot(normal, normalize(vec3<f32>(-0.70, -0.46, 0.54))), 0.0), 190.0);
    let sparkle = smoothstep(0.982, 0.999, glass_hash(floor((p + vec3<f32>(1.0)) * 31.0))) * smoothstep(-0.18, 0.90, p.z);
    let backlight = pow(max(dot(-normal, light_dir), 0.0), 1.7);

    var color = core * (0.48 + n_dot_l * 0.25);
    color += refraction * (0.24 + (1.0 - fresnel) * 0.22);
    color += reflection * (0.18 + fresnel * 0.78);
    color += vec3<f32>(0.52, 0.90, 1.0) * rim * 0.58;
    color += vec3<f32>(0.30, 0.72, 1.0) * backlight * 0.20;
    color = mix(color, vec3<f32>(0.03, 0.07, 0.10), ribbon_shadow * 0.16);
    color = mix(color, ribbon_color, ribbon * 0.82);
    color = mix(color, vec3<f32>(1.0, 0.84, 0.48), ribbon_core * 0.66);
    color = mix(color, vec3<f32>(0.62, 0.96, 1.0), bubbles * 0.30);
    color += vec3<f32>(0.58, 0.88, 1.0) * (specular * 0.28 + sharp_glint * 0.36 + sparkle * 0.06);

    color = emissive_grade(color);
    let alpha = clamp(0.36 + fresnel * 0.30 + rim * 0.18 + veins * 0.07 + ribbon * 0.18 + bubbles * 0.08 + specular * 0.08, 0.32, 0.84);
    return vec4<f32>(color, alpha);
}

fn plasma_palette(value: f32) -> vec3<f32> {
    let t = value * 6.2831853;
    return 0.58 + 0.42 * cos(vec3<f32>(t, t + 2.15, t + 4.20));
}

fn shade_plasma_orb(input: VertexOutput) -> vec4<f32> {
    let p = normalize(input.material.yzw);
    let normal = normalize(input.material_extra.xyz);
    let time = input.material_extra.w;
    let view_dir = vec3<f32>(0.0, 0.0, 1.0);
    let n_dot_v = clamp(dot(normal, view_dir), 0.0, 1.0);
    let fresnel = pow(1.0 - n_dot_v, 2.15);

    let flow_a = fbm_noise3(p * 2.2 + vec3<f32>(time * 0.23, -time * 0.18, time * 0.11));
    let flow_b = fbm_noise3(p.yzx * 4.7 + vec3<f32>(-time * 0.34, time * 0.27, 3.4));
    let flow_c = fbm_noise3(p.zxy * 9.5 + vec3<f32>(time * 0.82, 1.7, -time * 0.52));

    let swirl = sin(p.x * 5.8 - p.y * 4.1 + p.z * 3.7 + flow_a * 6.2 + time * 2.8) * 0.5 + 0.5;
    let counter_swirl = sin(p.x * -3.2 + p.y * 6.0 + flow_b * 7.5 - time * 3.4) * 0.5 + 0.5;
    let aurora = smoothstep(0.42, 0.96, swirl) * smoothstep(0.12, 0.95, counter_swirl);
    let bands = smoothstep(0.74, 0.98, abs(sin((p.y + flow_a * 0.28) * 18.0 + time * 4.1)));
    let lightning = smoothstep(0.88, 0.996, abs(sin((p.x - p.z) * 26.0 + flow_b * 9.0 + time * 7.2))) * smoothstep(0.50, 0.98, flow_c);
    let hot_core = pow(clamp(1.0 - length(p.xy * vec2<f32>(0.88, 1.12)), 0.0, 1.0), 2.4);
    let pulse = 0.74 + 0.26 * sin(time * 4.6 + flow_a * 6.2831853);

    let color_a = plasma_palette(flow_a * 0.55 + time * 0.065);
    let color_b = plasma_palette(flow_b * 0.72 + 0.38 - time * 0.050).bgr;
    var color = mix(vec3<f32>(0.03, 0.04, 0.12), color_a, 0.42 + aurora * 0.46);
    color = mix(color, color_b * vec3<f32>(0.70, 1.10, 1.45), bands * 0.70);
    color += vec3<f32>(0.10, 0.72, 1.0) * fresnel * 0.88;
    color += vec3<f32>(1.0, 0.12, 0.86) * aurora * 0.44 * pulse;
    color += vec3<f32>(1.0, 0.56, 0.16) * lightning * 0.58;
    color += vec3<f32>(0.38, 0.95, 1.0) * hot_core * 0.52;
    color += vec3<f32>(0.58, 0.35, 1.0) * pow(max(dot(normal, normalize(vec3<f32>(-0.32, -0.55, 0.77))), 0.0), 72.0) * 0.16;

    color = emissive_grade(color);
    let alpha = clamp(0.88 + fresnel * 0.08 + aurora * 0.02 + lightning * 0.04, 0.84, 0.98);
    return vec4<f32>(color, alpha);
}

fn shade_portal_orb(input: VertexOutput) -> vec4<f32> {
    let p = normalize(input.material.yzw);
    let normal = normalize(input.material_extra.xyz);
    let time = input.material_extra.w;
    let view_dir = vec3<f32>(0.0, 0.0, 1.0);
    let n_dot_v = clamp(dot(normal, view_dir), 0.0, 1.0);
    let fresnel = pow(1.0 - n_dot_v, 2.0);
    let radius = length(p.xy);
    let angle = atan2(p.y, p.x);
    let depth = clamp(p.z * 0.5 + 0.5, 0.0, 1.0);

    let flow = fbm_noise3(p * 3.4 + vec3<f32>(time * 0.36, -time * 0.22, time * 0.15));
    let swirl = sin(angle * 5.0 + radius * 21.0 - time * 5.4 + flow * 6.0) * 0.5 + 0.5;
    let reverse = sin(angle * -3.0 + radius * 13.5 + time * 3.6 + flow * 4.5) * 0.5 + 0.5;
    let tunnel = smoothstep(0.20, 0.92, radius) * (1.0 - smoothstep(0.92, 1.05, radius));
    let ring = 1.0 - smoothstep(0.018, 0.080, abs(radius - (0.62 + sin(time * 1.7) * 0.035)));
    let inner_ring = 1.0 - smoothstep(0.010, 0.050, abs(radius - (0.28 + flow * 0.08)));
    let sparks = smoothstep(0.965, 0.999, glass_hash(floor((p + vec3<f32>(1.0)) * 26.0 + vec3<f32>(time * 2.0)))) * tunnel;

    var color = mix(vec3<f32>(0.01, 0.01, 0.05), vec3<f32>(0.10, 0.55, 1.0), tunnel * depth);
    color += vec3<f32>(0.70, 0.08, 1.0) * swirl * tunnel * 0.82;
    color += vec3<f32>(0.03, 0.95, 1.0) * reverse * tunnel * 0.52;
    color += vec3<f32>(1.0, 0.47, 0.10) * ring * 0.95;
    color += vec3<f32>(0.85, 0.35, 1.0) * inner_ring * 0.68;
    color += vec3<f32>(0.18, 0.86, 1.0) * fresnel * 0.95;
    color += vec3<f32>(1.0, 0.92, 0.50) * sparks * 0.62;

    color = emissive_grade(color);
    let alpha = clamp(0.90 + tunnel * 0.04 + ring * 0.04 + fresnel * 0.04 + sparks * 0.02, 0.86, 0.99);
    return vec4<f32>(color, alpha);
}

fn shade_soap_bubble(input: VertexOutput) -> vec4<f32> {
    let p = normalize(input.material.yzw);
    let normal = normalize(input.material_extra.xyz);
    let time = input.material_extra.w;
    let view_dir = vec3<f32>(0.0, 0.0, 1.0);
    let light_dir = normalize(vec3<f32>(-0.42, -0.58, 0.70));
    let n_dot_v = clamp(dot(normal, view_dir), 0.0, 1.0);
    let fresnel = pow(1.0 - n_dot_v, 1.65);
    let film_noise = fbm_noise3(p * 3.2 + vec3<f32>(time * 0.08, -time * 0.06, time * 0.045));
    let film = fresnel * 3.6 + p.y * 1.55 + p.x * 0.65 + film_noise * 1.25 + time * 0.16;
    let rainbow = 0.55 + 0.45 * cos(vec3<f32>(0.0, 2.09, 4.18) + film * 6.2831853);
    let oil_band = smoothstep(0.35, 0.94, abs(sin(film * 4.2 + time * 0.7)));
    let highlight = pow(max(dot(normal, normalize(light_dir + view_dir)), 0.0), 120.0);
    let second_highlight = pow(max(dot(normal, normalize(vec3<f32>(0.54, -0.72, 0.44))), 0.0), 86.0);

    var color = rainbow * (0.24 + oil_band * 0.48);
    color += vec3<f32>(0.80, 0.96, 1.0) * fresnel * 0.42;
    color += mix(rainbow, vec3<f32>(0.60, 0.96, 1.0), 0.36) * (highlight * 0.28 + second_highlight * 0.18);
    color += vec3<f32>(0.70, 0.95, 1.0) * pow(clamp(1.0 - length(p.xy), 0.0, 1.0), 2.0) * 0.16;

    color = emissive_grade(color);
    let alpha = clamp(0.16 + fresnel * 0.42 + oil_band * 0.10 + highlight * 0.10, 0.14, 0.58);
    return vec4<f32>(color, alpha);
}

fn forcefield_grid(uv: vec2<f32>, time: f32) -> f32 {
    let scale = 18.0;
    let a = abs(sin((uv.x + time * 0.035) * scale));
    let b = abs(sin((uv.x * 0.5 + uv.y * 0.8660254 - time * 0.028) * scale));
    let c = abs(sin((uv.x * 0.5 - uv.y * 0.8660254 + time * 0.022) * scale));
    return 1.0 - smoothstep(0.045, 0.155, min(min(a, b), c));
}

fn shade_forcefield_orb(input: VertexOutput) -> vec4<f32> {
    let p = normalize(input.material.yzw);
    let normal = normalize(input.material_extra.xyz);
    let time = input.material_extra.w;
    let view_dir = vec3<f32>(0.0, 0.0, 1.0);
    let n_dot_v = clamp(dot(normal, view_dir), 0.0, 1.0);
    let fresnel = pow(1.0 - n_dot_v, 1.55);
    let grid = forcefield_grid(p.xy + vec2<f32>(sin(time * 0.6), cos(time * 0.4)) * 0.035, time);
    let scan = smoothstep(0.80, 0.99, abs(sin((p.y + time * 0.34) * 42.0)));
    let ripple = smoothstep(0.82, 0.995, sin(length(p.xy) * 28.0 - time * 5.8) * 0.5 + 0.5);
    let glitch = smoothstep(0.90, 0.997, glass_hash(floor(vec3<f32>(p.x * 10.0 + time * 3.0, p.y * 18.0, p.z * 6.0))));
    let impact_ring = 1.0 - smoothstep(0.018, 0.075, abs(length(p.xy - vec2<f32>(0.23, -0.18)) - (0.22 + fract(time * 0.35) * 0.48)));

    var color = vec3<f32>(0.015, 0.09, 0.12);
    color += vec3<f32>(0.05, 0.95, 1.0) * grid * 1.15;
    color += vec3<f32>(0.38, 0.78, 1.0) * scan * 0.28;
    color += vec3<f32>(0.10, 0.48, 1.0) * ripple * 0.38;
    color += vec3<f32>(0.75, 1.0, 1.0) * fresnel * 0.95;
    color += vec3<f32>(0.18, 0.92, 1.0) * impact_ring * 0.46;
    color += vec3<f32>(0.40, 1.0, 0.72) * glitch * 0.22;

    color = emissive_grade(color);
    let alpha = clamp(0.82 + fresnel * 0.08 + grid * 0.06 + impact_ring * 0.04, 0.78, 0.96);
    return vec4<f32>(color, alpha);
}

fn rotate2d(value: vec2<f32>, theta: f32) -> vec2<f32> {
    let s = sin(theta);
    let c = cos(theta);
    return vec2<f32>(value.x * c - value.y * s, value.x * s + value.y * c);
}

fn aces_tonemap(color: vec3<f32>) -> vec3<f32> {
    let m1 = mat3x3<f32>(
        vec3<f32>(0.59719, 0.07600, 0.02840),
        vec3<f32>(0.35458, 0.90834, 0.13383),
        vec3<f32>(0.04823, 0.01566, 0.83777)
    );
    let m2 = mat3x3<f32>(
        vec3<f32>(1.60475, -0.10208, -0.00327),
        vec3<f32>(-0.53108, 1.10813, -0.07276),
        vec3<f32>(-0.07367, -0.00605, 1.07602)
    );
    let value = m1 * color;
    let top = value * (value + vec3<f32>(0.0245786)) - vec3<f32>(0.000090537);
    let bottom = value * (vec3<f32>(0.983729) * value + vec3<f32>(0.4329510)) + vec3<f32>(0.238081);
    return clamp(m2 * (top / bottom), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn gold_dot_noise(p: vec3<f32>) -> f32 {
    let phi = 1.618033988;
    let q = vec3<f32>(
        dot(p, vec3<f32>(-0.571464913, -0.278044873, 0.772087367)),
        dot(p, vec3<f32>(0.814921382, -0.303026659, 0.494042493)),
        dot(p, vec3<f32>(0.096597072, 0.911518454, 0.399753815))
    );
    let r = vec3<f32>(
        dot(p, vec3<f32>(-0.571464913, 0.814921382, 0.096597072)),
        dot(p, vec3<f32>(-0.278044873, -0.303026659, 0.911518454)),
        dot(p, vec3<f32>(0.772087367, 0.494042493, 0.399753815))
    );
    return dot(cos(q), sin(phi * r));
}

fn cube_face_uv(p: vec3<f32>, normal: vec3<f32>) -> vec2<f32> {
    let an = abs(normal);
    if (an.z >= an.x && an.z >= an.y) {
        return p.xy;
    }
    if (an.x >= an.y) {
        return p.zy;
    }
    return p.xz;
}

fn shade_raymarch_cube(input: VertexOutput) -> vec4<f32> {
    let p = input.material.yzw;
    let normal = normalize(input.material_extra.xyz);
    let time = input.material_extra.w;
    let uv = cube_face_uv(p, normal);
    let face_edge = max(abs(uv.x), abs(uv.y));
    let face_rim = smoothstep(0.72, 1.0, face_edge);

    var ray_pos = vec3<f32>(uv * 1.18, -1.0 - 0.5 * sin(time * 0.10));
    let ray_dir = normalize(vec3<f32>(uv * 1.55, 1.42));
    var energy = vec3<f32>(0.0);

    for (var index: i32 = 0; index < 10; index = index + 1) {
        let fi = f32(index);
        var warped = ray_pos;
        warped.xy = rotate2d(sin(warped.xy * 0.25), time * 0.5 + warped.z * 2.0);
        var step_size = 0.001 + abs(gold_dot_noise(warped * 20.0) / 20.0 - gold_dot_noise(warped)) * 0.70;
        step_size += abs(ray_pos.y * 0.20 + sin(ray_pos.z * 2.0 + abs(ray_pos.x) * 0.50)) * 0.50;
        ray_pos += ray_dir * step_size;
        let wave = max(vec3<f32>(0.0), 1.0 + 1.5 * sin(vec3<f32>(fi) + length(ray_pos.xy * 0.1) + 2.0 + vec3<f32>(3.0, 1.5, 0.5)));
        energy += wave / step_size;
    }

    var color = aces_tonemap(energy * energy / vec3<f32>(500.0));
    color = mix(color, color.brg * vec3<f32>(1.35, 0.95, 1.55), 0.28 + 0.22 * sin(time + p.x * 4.0));
    color += vec3<f32>(0.30, 0.86, 1.0) * face_rim * 0.24;
    color += vec3<f32>(1.0, 0.42, 0.95) * smoothstep(0.86, 1.0, glass_hash(floor(p * 18.0 + vec3<f32>(time * 2.0)))) * 0.10;
    color = emissive_grade(color);

    let light = 1.0;
    let alpha = 1.0;
    return vec4<f32>(color * light, alpha);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if (input.material.x > 6.5) {
        let normal = normalize(input.material_extra.xyz);
        let key = clamp(dot(normal, normalize(vec3<f32>(-0.34, -0.46, 0.82))), 0.0, 1.0);
        let shade = clamp(0.60 + key * 0.34, 0.42, 1.0);
        return vec4<f32>(input.color.rgb * shade, input.color.a);
    }
    if (input.material.x > 5.5) {
        return shade_raymarch_cube(input);
    }
    if (input.material.x > 4.5) {
        return shade_forcefield_orb(input);
    }
    if (input.material.x > 3.5) {
        return shade_soap_bubble(input);
    }
    if (input.material.x > 2.5) {
        return shade_portal_orb(input);
    }
    if (input.material.x > 1.5) {
        return shade_plasma_orb(input);
    }
    if (input.material.x > 0.5) {
        return shade_glass_marble(input);
    }
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
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4],
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
    shatter_texture_view: ID3D11ShaderResourceView,
    shatter_sampler: ID3D11SamplerState,
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
        let shatter_texture_view = create_d3d_texture_view(&device, &[43, 43, 45, 255], 1, 1)
            .context("Failed to create fallback shatter texture")?;
        let shatter_sampler = create_d3d_sampler(&device).context("Failed to create shatter sampler")?;

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
            shatter_texture_view,
            shatter_sampler,
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

    fn capture_screen_texture(&mut self, bounds: RectF) -> Result<()> {
        let capture = capture_desktop_bgra(bounds).context("Failed to capture desktop pixels")?;
        self.shatter_texture_view =
            create_d3d_texture_view(&self.device, &capture.pixels, capture.width, capture.height)
                .context("Failed to upload shatter screenshot texture")?;
        Ok(())
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
        let shader_resources = [Some(self.shatter_texture_view.clone())];
        let samplers = [Some(self.shatter_sampler.clone())];

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
            self.context.PSSetShaderResources(0, Some(&shader_resources));
            self.context.PSSetSamplers(0, Some(&samplers));
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
struct DesktopCapture {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

#[cfg(target_os = "windows")]
fn capture_desktop_bgra(bounds: RectF) -> Result<DesktopCapture> {
    let width = bounds.width.max(1.0).round() as i32;
    let height = bounds.height.max(1.0).round() as i32;
    let source_x = bounds.x.round() as i32;
    let source_y = bounds.y.round() as i32;
    let mut pixels = vec![0u8; width as usize * height as usize * 4];

    unsafe {
        let screen_dc = GetDC(HWND(0));
        if screen_dc.0 == 0 {
            anyhow::bail!("GetDC returned null");
        }

        let memory_dc = CreateCompatibleDC(screen_dc);
        if memory_dc.0 == 0 {
            ReleaseDC(HWND(0), screen_dc);
            anyhow::bail!("CreateCompatibleDC returned null");
        }

        let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
        if bitmap.0 == 0 {
            DeleteDC(memory_dc);
            ReleaseDC(HWND(0), screen_dc);
            anyhow::bail!("CreateCompatibleBitmap returned null");
        }

        let previous = SelectObject(memory_dc, HGDIOBJ(bitmap.0));
        if let Err(error) = BitBlt(memory_dc, 0, 0, width, height, screen_dc, source_x, source_y, SRCCOPY) {
            SelectObject(memory_dc, previous);
            DeleteObject(HGDIOBJ(bitmap.0));
            DeleteDC(memory_dc);
            ReleaseDC(HWND(0), screen_dc);
            return Err(error).context("BitBlt failed");
        }

        let mut bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: pixels.len() as u32,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            ..Default::default()
        };

        let lines = GetDIBits(
            memory_dc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut bitmap_info,
            DIB_RGB_COLORS,
        );

        SelectObject(memory_dc, previous);
        DeleteObject(HGDIOBJ(bitmap.0));
        DeleteDC(memory_dc);
        ReleaseDC(HWND(0), screen_dc);

        if lines == 0 {
            anyhow::bail!("GetDIBits returned no scanlines");
        }
    }

    Ok(DesktopCapture {
        pixels,
        width: width as u32,
        height: height as u32,
    })
}

#[cfg(target_os = "windows")]
fn create_d3d_texture_view(
    device: &ID3D11Device,
    pixels_bgra: &[u8],
    width: u32,
    height: u32,
) -> Result<ID3D11ShaderResourceView> {
    let width = width.max(1);
    let height = height.max(1);
    let expected_len = width as usize * height as usize * 4;
    if pixels_bgra.len() < expected_len {
        anyhow::bail!(
            "Texture upload had {} bytes, expected at least {expected_len}",
            pixels_bgra.len()
        );
    }

    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_IMMUTABLE,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let initial_data = D3D11_SUBRESOURCE_DATA {
        pSysMem: pixels_bgra.as_ptr().cast(),
        SysMemPitch: width * 4,
        SysMemSlicePitch: expected_len as u32,
    };
    let mut texture = None;
    unsafe {
        device
            .CreateTexture2D(&desc, Some(&initial_data), Some(&mut texture))
            .context("Failed to create D3D shatter texture")?;
    }
    let texture = texture.context("CreateTexture2D returned no shatter texture")?;
    let mut view = None;
    unsafe {
        device
            .CreateShaderResourceView(&texture, None, Some(&mut view))
            .context("Failed to create D3D shatter texture view")?;
    }
    view.context("CreateShaderResourceView returned no shatter texture view")
}

#[cfg(target_os = "windows")]
fn create_d3d_sampler(device: &ID3D11Device) -> Result<ID3D11SamplerState> {
    let desc = D3D11_SAMPLER_DESC {
        Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
        AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
        AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
        AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
        ComparisonFunc: D3D11_COMPARISON_NEVER,
        MaxLOD: f32::MAX,
        ..Default::default()
    };
    let mut sampler = None;
    unsafe {
        device
            .CreateSamplerState(&desc, Some(&mut sampler))
            .context("Failed to create D3D sampler")?;
    }
    sampler.context("CreateSamplerState returned no sampler")
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
Texture2D shatter_texture : register(t0);
SamplerState shatter_sampler : register(s0);

cbuffer RenderUniforms : register(b0) {
    float2 viewport_size;
    float camera_distance;
    float padding;
};

struct VSInput {
    float3 position : POSITION;
    float4 color : COLOR0;
    float4 material : TEXCOORD0;
    float4 material_extra : TEXCOORD1;
};
struct PSInput {
    float4 position : SV_POSITION;
    float4 color : COLOR0;
    float4 material : TEXCOORD0;
    float4 material_extra : TEXCOORD1;
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
    output.material = input.material;
    output.material_extra = input.material_extra;
    return output;
}

float glass_hash(float3 p) {
    return frac(sin(dot(p, float3(17.31f, 41.17f, 73.13f))) * 43758.5453f);
}

float3 wglnoise_mod289_3(float3 x) {
    return x - floor(x / 289.0f) * 289.0f;
}

float4 wglnoise_mod289_4(float4 x) {
    return x - floor(x / 289.0f) * 289.0f;
}

float4 wglnoise_permute_4(float4 x) {
    return wglnoise_mod289_4((x * 34.0f + 10.0f) * x);
}

float simplex_noise3(float3 v) {
    float3 i = floor(v + dot(v, 1.0f / 3.0f));
    float3 x0 = v - i + dot(i, 1.0f / 6.0f);

    float3 g = float3(
        x0.y <= x0.x ? 1.0f : 0.0f,
        x0.z <= x0.y ? 1.0f : 0.0f,
        x0.x <= x0.z ? 1.0f : 0.0f
    );
    float3 l = 1.0f - g;
    float3 i1 = min(g.xyz, l.zxy);
    float3 i2 = max(g.xyz, l.zxy);

    float3 x1 = x0 - i1 + 1.0f / 6.0f;
    float3 x2 = x0 - i2 + 1.0f / 3.0f;
    float3 x3 = x0 - 0.5f;

    i = wglnoise_mod289_3(i);
    float4 perm = wglnoise_permute_4(i.z + float4(0.0f, i1.z, i2.z, 1.0f));
    perm = wglnoise_permute_4(perm + i.y + float4(0.0f, i1.y, i2.y, 1.0f));
    perm = wglnoise_permute_4(perm + i.x + float4(0.0f, i1.x, i2.x, 1.0f));

    float4 gx = -1.0f + frac(perm / 7.0f) * 2.0f;
    float4 gy = -1.0f + frac(floor(perm / 7.0f) / 7.0f) * 2.0f;
    float4 gz = 1.0f - abs(gx) - abs(gy);
    float4 zn = float4(gz.x < 0.0f ? 1.0f : 0.0f, gz.y < 0.0f ? 1.0f : 0.0f, gz.z < 0.0f ? 1.0f : 0.0f, gz.w < 0.0f ? 1.0f : 0.0f);
    float4 gx_adjust = float4(gx.x < 0.0f ? 1.0f : -1.0f, gx.y < 0.0f ? 1.0f : -1.0f, gx.z < 0.0f ? 1.0f : -1.0f, gx.w < 0.0f ? 1.0f : -1.0f);
    float4 gy_adjust = float4(gy.x < 0.0f ? 1.0f : -1.0f, gy.y < 0.0f ? 1.0f : -1.0f, gy.z < 0.0f ? 1.0f : -1.0f, gy.w < 0.0f ? 1.0f : -1.0f);
    gx += zn * gx_adjust;
    gy += zn * gy_adjust;

    float3 g0 = normalize(float3(gx.x, gy.x, gz.x));
    float3 g1 = normalize(float3(gx.y, gy.y, gz.y));
    float3 g2 = normalize(float3(gx.z, gy.z, gz.z));
    float3 g3 = normalize(float3(gx.w, gy.w, gz.w));

    float4 px = float4(dot(g0, x0), dot(g1, x1), dot(g2, x2), dot(g3, x3));
    float4 m = max(0.5f - float4(dot(x0, x0), dot(x1, x1), dot(x2, x2), dot(x3, x3)), 0.0f);
    float4 m3 = m * m * m;
    float4 m4 = m * m3;
    return 107.0f * dot(m4, px);
}

float fbm_noise3(float3 seed) {
    float3 p = seed;
    float value = 0.0f;
    float amplitude = 0.52f;
    [unroll]
    for (int octave = 0; octave < 4; ++octave) {
        value += amplitude * (simplex_noise3(p) * 0.5f + 0.5f);
        p = p * 2.03f + float3(13.1f, -7.7f, 5.3f);
        amplitude *= 0.50f;
    }
    return saturate(value);
}

float marble_stripes(float value, float frequency) {
    float t = 0.5f + 0.5f * sin(frequency * 6.2831853f * value);
    return t * t;
}

float smooth01(float edge0, float edge1, float value) {
    float t = saturate((value - edge0) / max(edge1 - edge0, 0.00001f));
    return t * t * (3.0f - 2.0f * t);
}

float sphere_spot(float3 center, float radius, float feather, float3 p) {
    return 1.0f - smooth01(radius, radius + feather, length(p - center));
}

float line_mask(float width, float feather, float value) {
    return 1.0f - smooth01(width, width + feather, abs(value));
}

float3 glass_environment(float3 dir) {
    float3 d = normalize(dir);
    float3 sky = lerp(float3(0.08f, 0.11f, 0.14f), float3(0.55f, 0.82f, 1.0f), saturate(d.y * 0.5f + 0.5f));
    float horizon = pow(1.0f - abs(d.y), 4.0f);
    float window = pow(max(dot(d, normalize(float3(-0.58f, -0.44f, 0.69f))), 0.0f), 90.0f);
    return sky + float3(1.0f, 0.70f, 0.38f) * horizon * 0.14f + float3(0.38f, 0.90f, 1.0f) * window * 0.72f;
}

float3 display_tonemap(float3 color) {
    float3 safe = max(color, 0.0f);
    float peak = max(safe.r, max(safe.g, safe.b));
    float over = max(peak - 0.82f, 0.0f);
    float scale = 1.0f / (1.0f + over * 1.10f);
    return clamp(safe * scale, 0.0f, 0.98f);
}

float3 emissive_grade(float3 color) {
    float3 mapped = display_tonemap(color);
    float luma = dot(mapped, float3(0.2126f, 0.7152f, 0.0722f));
    float3 saturated = lerp(float3(luma, luma, luma), mapped, 1.32f);
    return clamp(saturated * 1.10f + 0.018f, 0.0f, 0.98f);
}

float4 shade_glass_marble(PSInput input) {
    float3 p = normalize(input.material.yzw);
    float3 normal = normalize(input.material_extra.xyz);
    float3 view_dir = float3(0.0f, 0.0f, 1.0f);
    float3 light_dir = normalize(float3(-0.46f, -0.62f, 0.64f));
    float n_dot_v = saturate(dot(normal, view_dir));
    float n_dot_l = max(dot(normal, light_dir), 0.0f);
    float fresnel = pow(1.0f - n_dot_v, 5.0f);
    float rim = pow(saturate(1.0f - abs(n_dot_v)), 2.3f);

    float3 marble_p = p * 1.22f + float3(0.28f, -0.57f, 0.13f);
    float warp = fbm_noise3(marble_p * 1.45f);
    float warp_fine = fbm_noise3(marble_p * 4.6f + float3(warp * 2.6f, -warp * 1.4f, 1.9f));
    float vein_axis = p.x * 0.88f + p.y * 0.24f - p.z * 0.58f + warp * 1.82f + warp_fine * 0.44f;
    float stripe = marble_stripes(vein_axis, 1.55f);
    float vein_soft = smooth01(0.48f, 0.84f, stripe);
    float vein_hair = smooth01(0.86f, 0.985f, stripe) * (0.45f + warp_fine * 0.55f);
    float veins = saturate(vein_soft * 0.50f + vein_hair * 0.72f);
    float cloud = fbm_noise3(p * 5.8f + float3(10.4f, -3.1f, 1.7f));

    float3 core = lerp(float3(0.78f, 0.97f, 1.0f), float3(0.10f, 0.42f, 0.68f), veins * 0.58f);
    core = lerp(core, float3(0.96f, 1.0f, 0.97f), 0.20f + cloud * 0.12f);

    float ribbon_curve = p.x * 0.54f - p.z * 0.24f + sin(p.y * 5.3f + warp * 3.0f) * 0.11f;
    float ribbon_window = smooth01(-0.62f, -0.34f, p.y) * (1.0f - smooth01(0.34f, 0.62f, p.y));
    float ribbon_depth = smooth01(-0.18f, 0.72f, p.z) * (1.0f - smooth01(0.88f, 1.0f, p.z));
    float ribbon = line_mask(0.026f, 0.045f, ribbon_curve) * ribbon_window * ribbon_depth;
    float ribbon_core = line_mask(0.007f, 0.018f, ribbon_curve) * ribbon_window * ribbon_depth;
    float ribbon_shadow = line_mask(0.062f, 0.070f, ribbon_curve + 0.026f) * ribbon_window * ribbon_depth;
    float3 ribbon_color = lerp(float3(1.0f, 0.18f, 0.05f), float3(0.05f, 0.48f, 1.0f), smooth01(-0.12f, 0.58f, p.y + warp * 0.20f));

    float bubble1 = sphere_spot(float3(-0.34f, -0.16f, 0.44f), 0.036f, 0.028f, p);
    float bubble2 = sphere_spot(float3(0.26f, 0.22f, 0.30f), 0.026f, 0.022f, p);
    float bubble3 = sphere_spot(float3(0.08f, -0.40f, 0.50f), 0.020f, 0.018f, p);
    float bubbles = saturate(bubble1 + bubble2 + bubble3);

    float3 reflect_dir = reflect(-view_dir, normal);
    float3 refract_dir = normalize(refract(-view_dir, normal, 1.0f / 1.45f) + float3((warp - 0.5f) * 0.12f, (warp_fine - 0.5f) * 0.08f, 0.0f));
    float3 reflection = glass_environment(reflect_dir);
    float3 refraction = glass_environment(refract_dir);
    float3 half_dir = normalize(light_dir + view_dir);
    float specular = pow(max(dot(normal, half_dir), 0.0f), 110.0f);
    float sharp_glint = pow(max(dot(normal, normalize(float3(-0.70f, -0.46f, 0.54f))), 0.0f), 190.0f);
    float sparkle = smooth01(0.982f, 0.999f, glass_hash(floor((p + float3(1.0f, 1.0f, 1.0f)) * 31.0f))) * smooth01(-0.18f, 0.90f, p.z);
    float backlight = pow(max(dot(-normal, light_dir), 0.0f), 1.7f);

    float3 color = core * (0.48f + n_dot_l * 0.25f);
    color += refraction * (0.24f + (1.0f - fresnel) * 0.22f);
    color += reflection * (0.18f + fresnel * 0.78f);
    color += float3(0.52f, 0.90f, 1.0f) * rim * 0.58f;
    color += float3(0.30f, 0.72f, 1.0f) * backlight * 0.20f;
    color = lerp(color, float3(0.03f, 0.07f, 0.10f), ribbon_shadow * 0.16f);
    color = lerp(color, ribbon_color, ribbon * 0.82f);
    color = lerp(color, float3(1.0f, 0.84f, 0.48f), ribbon_core * 0.66f);
    color = lerp(color, float3(0.62f, 0.96f, 1.0f), bubbles * 0.30f);
    color += float3(0.58f, 0.88f, 1.0f) * (specular * 0.28f + sharp_glint * 0.36f + sparkle * 0.06f);

    color = emissive_grade(color);
    float alpha = clamp(0.36f + fresnel * 0.30f + rim * 0.18f + veins * 0.07f + ribbon * 0.18f + bubbles * 0.08f + specular * 0.08f, 0.32f, 0.84f);
    return float4(color * alpha, alpha);
}

float3 plasma_palette(float value) {
    float t = value * 6.2831853f;
    return 0.58f + 0.42f * cos(float3(t, t + 2.15f, t + 4.20f));
}

float4 shade_plasma_orb(PSInput input) {
    float3 p = normalize(input.material.yzw);
    float3 normal = normalize(input.material_extra.xyz);
    float time = input.material_extra.w;
    float3 view_dir = float3(0.0f, 0.0f, 1.0f);
    float n_dot_v = saturate(dot(normal, view_dir));
    float fresnel = pow(1.0f - n_dot_v, 2.15f);

    float flow_a = fbm_noise3(p * 2.2f + float3(time * 0.23f, -time * 0.18f, time * 0.11f));
    float flow_b = fbm_noise3(p.yzx * 4.7f + float3(-time * 0.34f, time * 0.27f, 3.4f));
    float flow_c = fbm_noise3(p.zxy * 9.5f + float3(time * 0.82f, 1.7f, -time * 0.52f));

    float swirl = sin(p.x * 5.8f - p.y * 4.1f + p.z * 3.7f + flow_a * 6.2f + time * 2.8f) * 0.5f + 0.5f;
    float counter_swirl = sin(p.x * -3.2f + p.y * 6.0f + flow_b * 7.5f - time * 3.4f) * 0.5f + 0.5f;
    float aurora = smooth01(0.42f, 0.96f, swirl) * smooth01(0.12f, 0.95f, counter_swirl);
    float bands = smooth01(0.74f, 0.98f, abs(sin((p.y + flow_a * 0.28f) * 18.0f + time * 4.1f)));
    float lightning = smooth01(0.88f, 0.996f, abs(sin((p.x - p.z) * 26.0f + flow_b * 9.0f + time * 7.2f))) * smooth01(0.50f, 0.98f, flow_c);
    float hot_core = pow(saturate(1.0f - length(p.xy * float2(0.88f, 1.12f))), 2.4f);
    float pulse = 0.74f + 0.26f * sin(time * 4.6f + flow_a * 6.2831853f);

    float3 color_a = plasma_palette(flow_a * 0.55f + time * 0.065f);
    float3 color_b = plasma_palette(flow_b * 0.72f + 0.38f - time * 0.050f).bgr;
    float3 color = lerp(float3(0.03f, 0.04f, 0.12f), color_a, 0.42f + aurora * 0.46f);
    color = lerp(color, color_b * float3(0.70f, 1.10f, 1.45f), bands * 0.70f);
    color += float3(0.10f, 0.72f, 1.0f) * fresnel * 0.88f;
    color += float3(1.0f, 0.12f, 0.86f) * aurora * 0.44f * pulse;
    color += float3(1.0f, 0.56f, 0.16f) * lightning * 0.58f;
    color += float3(0.38f, 0.95f, 1.0f) * hot_core * 0.52f;
    color += float3(0.58f, 0.35f, 1.0f) * pow(max(dot(normal, normalize(float3(-0.32f, -0.55f, 0.77f))), 0.0f), 72.0f) * 0.16f;

    color = emissive_grade(color);
    float alpha = clamp(0.88f + fresnel * 0.08f + aurora * 0.02f + lightning * 0.04f, 0.84f, 0.98f);
    return float4(color * alpha, alpha);
}

float4 shade_portal_orb(PSInput input) {
    float3 p = normalize(input.material.yzw);
    float3 normal = normalize(input.material_extra.xyz);
    float time = input.material_extra.w;
    float3 view_dir = float3(0.0f, 0.0f, 1.0f);
    float n_dot_v = saturate(dot(normal, view_dir));
    float fresnel = pow(1.0f - n_dot_v, 2.0f);
    float radius = length(p.xy);
    float angle = atan2(p.y, p.x);
    float depth = saturate(p.z * 0.5f + 0.5f);

    float flow = fbm_noise3(p * 3.4f + float3(time * 0.36f, -time * 0.22f, time * 0.15f));
    float swirl = sin(angle * 5.0f + radius * 21.0f - time * 5.4f + flow * 6.0f) * 0.5f + 0.5f;
    float reverse = sin(angle * -3.0f + radius * 13.5f + time * 3.6f + flow * 4.5f) * 0.5f + 0.5f;
    float tunnel = smooth01(0.20f, 0.92f, radius) * (1.0f - smooth01(0.92f, 1.05f, radius));
    float ring = 1.0f - smooth01(0.018f, 0.080f, abs(radius - (0.62f + sin(time * 1.7f) * 0.035f)));
    float inner_ring = 1.0f - smooth01(0.010f, 0.050f, abs(radius - (0.28f + flow * 0.08f)));
    float sparks = smooth01(0.965f, 0.999f, glass_hash(floor((p + float3(1.0f, 1.0f, 1.0f)) * 26.0f + float3(time * 2.0f, time * 2.0f, time * 2.0f)))) * tunnel;

    float3 color = lerp(float3(0.01f, 0.01f, 0.05f), float3(0.10f, 0.55f, 1.0f), tunnel * depth);
    color += float3(0.70f, 0.08f, 1.0f) * swirl * tunnel * 0.82f;
    color += float3(0.03f, 0.95f, 1.0f) * reverse * tunnel * 0.52f;
    color += float3(1.0f, 0.47f, 0.10f) * ring * 0.95f;
    color += float3(0.85f, 0.35f, 1.0f) * inner_ring * 0.68f;
    color += float3(0.18f, 0.86f, 1.0f) * fresnel * 0.95f;
    color += float3(1.0f, 0.92f, 0.50f) * sparks * 0.62f;

    color = emissive_grade(color);
    float alpha = clamp(0.90f + tunnel * 0.04f + ring * 0.04f + fresnel * 0.04f + sparks * 0.02f, 0.86f, 0.99f);
    return float4(color * alpha, alpha);
}

float4 shade_soap_bubble(PSInput input) {
    float3 p = normalize(input.material.yzw);
    float3 normal = normalize(input.material_extra.xyz);
    float time = input.material_extra.w;
    float3 view_dir = float3(0.0f, 0.0f, 1.0f);
    float3 light_dir = normalize(float3(-0.42f, -0.58f, 0.70f));
    float n_dot_v = saturate(dot(normal, view_dir));
    float fresnel = pow(1.0f - n_dot_v, 1.65f);
    float film_noise = fbm_noise3(p * 3.2f + float3(time * 0.08f, -time * 0.06f, time * 0.045f));
    float film = fresnel * 3.6f + p.y * 1.55f + p.x * 0.65f + film_noise * 1.25f + time * 0.16f;
    float3 rainbow = 0.55f + 0.45f * cos(float3(0.0f, 2.09f, 4.18f) + film * 6.2831853f);
    float oil_band = smooth01(0.35f, 0.94f, abs(sin(film * 4.2f + time * 0.7f)));
    float highlight = pow(max(dot(normal, normalize(light_dir + view_dir)), 0.0f), 120.0f);
    float second_highlight = pow(max(dot(normal, normalize(float3(0.54f, -0.72f, 0.44f))), 0.0f), 86.0f);

    float3 color = rainbow * (0.24f + oil_band * 0.48f);
    color += float3(0.80f, 0.96f, 1.0f) * fresnel * 0.42f;
    color += lerp(rainbow, float3(0.60f, 0.96f, 1.0f), 0.36f) * (highlight * 0.28f + second_highlight * 0.18f);
    color += float3(0.70f, 0.95f, 1.0f) * pow(saturate(1.0f - length(p.xy)), 2.0f) * 0.16f;

    color = emissive_grade(color);
    float alpha = clamp(0.16f + fresnel * 0.42f + oil_band * 0.10f + highlight * 0.10f, 0.14f, 0.58f);
    return float4(color * alpha, alpha);
}

float forcefield_grid(float2 uv, float time) {
    float scale = 18.0f;
    float a = abs(sin((uv.x + time * 0.035f) * scale));
    float b = abs(sin((uv.x * 0.5f + uv.y * 0.8660254f - time * 0.028f) * scale));
    float c = abs(sin((uv.x * 0.5f - uv.y * 0.8660254f + time * 0.022f) * scale));
    return 1.0f - smooth01(0.045f, 0.155f, min(min(a, b), c));
}

float4 shade_forcefield_orb(PSInput input) {
    float3 p = normalize(input.material.yzw);
    float3 normal = normalize(input.material_extra.xyz);
    float time = input.material_extra.w;
    float3 view_dir = float3(0.0f, 0.0f, 1.0f);
    float n_dot_v = saturate(dot(normal, view_dir));
    float fresnel = pow(1.0f - n_dot_v, 1.55f);
    float grid = forcefield_grid(p.xy + float2(sin(time * 0.6f), cos(time * 0.4f)) * 0.035f, time);
    float scan = smooth01(0.80f, 0.99f, abs(sin((p.y + time * 0.34f) * 42.0f)));
    float ripple = smooth01(0.82f, 0.995f, sin(length(p.xy) * 28.0f - time * 5.8f) * 0.5f + 0.5f);
    float glitch = smooth01(0.90f, 0.997f, glass_hash(floor(float3(p.x * 10.0f + time * 3.0f, p.y * 18.0f, p.z * 6.0f))));
    float impact_ring = 1.0f - smooth01(0.018f, 0.075f, abs(length(p.xy - float2(0.23f, -0.18f)) - (0.22f + frac(time * 0.35f) * 0.48f)));

    float3 color = float3(0.015f, 0.09f, 0.12f);
    color += float3(0.05f, 0.95f, 1.0f) * grid * 1.15f;
    color += float3(0.38f, 0.78f, 1.0f) * scan * 0.28f;
    color += float3(0.10f, 0.48f, 1.0f) * ripple * 0.38f;
    color += float3(0.75f, 1.0f, 1.0f) * fresnel * 0.95f;
    color += float3(0.18f, 0.92f, 1.0f) * impact_ring * 0.46f;
    color += float3(0.40f, 1.0f, 0.72f) * glitch * 0.22f;

    color = emissive_grade(color);
    float alpha = clamp(0.82f + fresnel * 0.08f + grid * 0.06f + impact_ring * 0.04f, 0.78f, 0.96f);
    return float4(color * alpha, alpha);
}

float2 rotate2d(float2 value, float theta) {
    float s = sin(theta);
    float c = cos(theta);
    return float2(value.x * c - value.y * s, value.x * s + value.y * c);
}

float3 aces_tonemap(float3 color) {
    float3x3 m1 = float3x3(
        0.59719f, 0.07600f, 0.02840f,
        0.35458f, 0.90834f, 0.13383f,
        0.04823f, 0.01566f, 0.83777f
    );
    float3x3 m2 = float3x3(
        1.60475f, -0.10208f, -0.00327f,
        -0.53108f, 1.10813f, -0.07276f,
        -0.07367f, -0.00605f, 1.07602f
    );
    float3 value = mul(m1, color);
    float3 top = value * (value + 0.0245786f) - 0.000090537f;
    float3 bottom = value * (0.983729f * value + 0.4329510f) + 0.238081f;
    return clamp(mul(m2, top / bottom), 0.0f, 1.0f);
}

float gold_dot_noise(float3 p) {
    const float phi = 1.618033988f;
    float3 q = float3(
        dot(p, float3(-0.571464913f, -0.278044873f, 0.772087367f)),
        dot(p, float3(0.814921382f, -0.303026659f, 0.494042493f)),
        dot(p, float3(0.096597072f, 0.911518454f, 0.399753815f))
    );
    float3 r = float3(
        dot(p, float3(-0.571464913f, 0.814921382f, 0.096597072f)),
        dot(p, float3(-0.278044873f, -0.303026659f, 0.911518454f)),
        dot(p, float3(0.772087367f, 0.494042493f, 0.399753815f))
    );
    return dot(cos(q), sin(phi * r));
}

float2 cube_face_uv(float3 p, float3 normal) {
    float3 an = abs(normal);
    if (an.z >= an.x && an.z >= an.y) {
        return p.xy;
    }
    if (an.x >= an.y) {
        return p.zy;
    }
    return p.xz;
}

float4 shade_raymarch_cube(PSInput input) {
    float3 p = input.material.yzw;
    float3 normal = normalize(input.material_extra.xyz);
    float time = input.material_extra.w;
    float2 uv = cube_face_uv(p, normal);
    float face_edge = max(abs(uv.x), abs(uv.y));
    float face_rim = smooth01(0.72f, 1.0f, face_edge);

    float3 ray_pos = float3(uv * 1.18f, -1.0f - 0.5f * sin(time * 0.10f));
    float3 ray_dir = normalize(float3(uv * 1.55f, 1.42f));
    float3 energy = float3(0.0f, 0.0f, 0.0f);

    [unroll]
    for (int index = 0; index < 10; ++index) {
        float fi = (float)index;
        float3 warped = ray_pos;
        warped.xy = rotate2d(sin(warped.xy * 0.25f), time * 0.5f + warped.z * 2.0f);
        float step_size = 0.001f + abs(gold_dot_noise(warped * 20.0f) / 20.0f - gold_dot_noise(warped)) * 0.70f;
        step_size += abs(ray_pos.y * 0.20f + sin(ray_pos.z * 2.0f + abs(ray_pos.x) * 0.50f)) * 0.50f;
        ray_pos += ray_dir * step_size;
        float3 wave = max(float3(0.0f, 0.0f, 0.0f), 1.0f + 1.5f * sin(float3(fi, fi, fi) + length(ray_pos.xy * 0.1f) + 2.0f + float3(3.0f, 1.5f, 0.5f)));
        energy += wave / step_size;
    }

    float3 color = aces_tonemap(energy * energy / 500.0f);
    color = lerp(color, color.brg * float3(1.35f, 0.95f, 1.55f), 0.28f + 0.22f * sin(time + p.x * 4.0f));
    color += float3(0.30f, 0.86f, 1.0f) * face_rim * 0.24f;
    color += float3(1.0f, 0.42f, 0.95f) * smooth01(0.86f, 1.0f, glass_hash(floor(p * 18.0f + float3(time * 2.0f, time * 2.0f, time * 2.0f)))) * 0.10f;
    color = emissive_grade(color);

    float light = 1.0f;
    float alpha = 1.0f;
    return float4(color * light * alpha, alpha);
}

float4 shade_screen_shard(PSInput input) {
    float2 uv = saturate(input.material.yz);
    float3 normal = normalize(input.material_extra.xyz);
    float key = saturate(dot(normal, normalize(float3(-0.34f, -0.46f, 0.82f))));
    float fill = saturate(dot(normal, normalize(float3(0.48f, 0.22f, 0.66f))));
    float shade = clamp(0.58f + key * 0.36f + fill * 0.10f, 0.42f, 1.08f);
    float3 color = shatter_texture.Sample(shatter_sampler, uv).rgb * shade;
    float edge_darkening = smooth01(0.0f, 0.32f, abs(normal.z));
    color = lerp(color * 0.72f, color, edge_darkening);
    return float4(color, 1.0f);
}

float4 ps_main(PSInput input) : SV_TARGET {
    if (input.material.x > 6.5f) {
        return shade_screen_shard(input);
    }
    if (input.material.x > 5.5f) {
        return shade_raymarch_cube(input);
    }
    if (input.material.x > 4.5f) {
        return shade_forcefield_orb(input);
    }
    if (input.material.x > 3.5f) {
        return shade_soap_bubble(input);
    }
    if (input.material.x > 2.5f) {
        return shade_portal_orb(input);
    }
    if (input.material.x > 1.5f) {
        return shade_plasma_orb(input);
    }
    if (input.material.x > 0.5f) {
        return shade_glass_marble(input);
    }
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
        let texcoord_name = CString::new("TEXCOORD")?;
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
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: PCSTR(texcoord_name.as_ptr().cast()),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 28,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: PCSTR(texcoord_name.as_ptr().cast()),
                SemanticIndex: 1,
                Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 44,
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
            WindowEvent::ModifiersChanged(modifiers) => {
                self.keyboard_modifiers = modifiers.state();
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
                    TrayAction::ShowControlUi => AppAction::ShowControlUi,
                    TrayAction::SpawnObject => AppAction::SpawnObject,
                    TrayAction::SpawnCrystal => AppAction::SpawnCrystal,
                    TrayAction::SpawnDvdLogo => AppAction::SpawnDvdLogo,
                    TrayAction::SpawnStressCubes => AppAction::SpawnStressCubes,
                    TrayAction::Reset => AppAction::Reset,
                    TrayAction::ToggleSettings => AppAction::ToggleSettings,
                    TrayAction::ToggleWeather => AppAction::ToggleWeather,
                    TrayAction::ToggleSand => AppAction::ToggleSand,
                    TrayAction::ToggleMeasureTool => AppAction::ToggleMeasureTool,
                    TrayAction::ToggleSpotlight => AppAction::ToggleSpotlight,
                    TrayAction::ToggleLassoTool => AppAction::ToggleLassoTool,
                    TrayAction::ToggleShatterGun => AppAction::ToggleShatterGun,
                    TrayAction::ShatterScreen => AppAction::ShatterScreen,
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
    ShowControlUi,
    ToggleDebug,
    SpawnObject,
    SpawnCrystal,
    SpawnDvdLogo,
    SpawnStressCubes,
    SpawnRobotBuddy,
    ToggleSlingshotGame,
    ToggleBasketballGame,
    Reset,
    ToggleSettings,
    ToggleWeather,
    ToggleSand,
    ToggleMeasureTool,
    ToggleSpotlight,
    ToggleLassoTool,
    ToggleShatterGun,
    ShatterScreen,
    RequestImport,
    Exit,
}

#[derive(Debug)]
struct ShatterGunTool {
    active: bool,
    shots_fired: u32,
    last_fire_at: f64,
    render_cells: Vec<SandRenderCell>,
}

impl Default for ShatterGunTool {
    fn default() -> Self {
        Self {
            active: false,
            shots_fired: 0,
            last_fire_at: -10.0,
            render_cells: Vec::with_capacity(96),
        }
    }
}

impl ShatterGunTool {
    fn toggle(&mut self, cursor: Vector2, now_seconds: f64) -> bool {
        if self.active {
            self.deactivate();
        } else {
            self.active = true;
            self.last_fire_at = now_seconds - 10.0;
            self.rebuild_render_cells(cursor, now_seconds);
        }
        self.active
    }

    fn deactivate(&mut self) {
        self.active = false;
        self.render_cells.clear();
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn fire(&mut self, cursor: Vector2, now_seconds: f64) {
        self.shots_fired = self.shots_fired.saturating_add(1);
        self.last_fire_at = now_seconds;
        self.rebuild_render_cells(cursor, now_seconds);
    }

    fn rebuild_render_cells(&mut self, cursor: Vector2, now_seconds: f64) {
        self.render_cells.clear();
        if !self.active {
            return;
        }

        let flash = now_seconds - self.last_fire_at < 0.18;
        let reticle = if flash {
            AppColor::from_argb(245, 255, 226, 98)
        } else {
            AppColor::from_argb(210, 128, 222, 241)
        };
        push_tool_rect(&mut self.render_cells, cursor.x - 18.0, cursor.y - 1.0, 10, 2, reticle);
        push_tool_rect(&mut self.render_cells, cursor.x + 8.0, cursor.y - 1.0, 10, 2, reticle);
        push_tool_rect(&mut self.render_cells, cursor.x - 1.0, cursor.y - 18.0, 2, 10, reticle);
        push_tool_rect(&mut self.render_cells, cursor.x - 1.0, cursor.y + 8.0, 2, 10, reticle);
        push_lasso_dot(&mut self.render_cells, cursor, if flash { 7 } else { 5 }, AppColor::from_argb(120, 255, 255, 255));

        let x = cursor.x + 22.0;
        let y = cursor.y - 38.0;
        let shadow = AppColor::from_argb(150, 4, 7, 10);
        let body = AppColor::from_argb(242, 97, 232, 155);
        let body_dark = AppColor::from_argb(242, 27, 123, 93);
        let accent = AppColor::from_argb(245, 255, 171, 77);
        let metal = AppColor::from_argb(238, 219, 218, 212);
        push_tool_rect(&mut self.render_cells, x + 3.0, y + 9.0, 52, 22, shadow);
        push_tool_rect(&mut self.render_cells, x + 0.0, y + 6.0, 34, 17, body);
        push_tool_rect(&mut self.render_cells, x + 32.0, y + 8.0, 28, 9, body);
        push_tool_rect(&mut self.render_cells, x + 57.0, y + 10.0, 9, 5, metal);
        push_tool_rect(&mut self.render_cells, x + 7.0, y + 21.0, 13, 24, body_dark);
        push_tool_rect(&mut self.render_cells, x + 20.0, y + 22.0, 10, 9, accent);
        push_tool_rect(&mut self.render_cells, x + 5.0, y + 3.0, 16, 5, accent);
        push_tool_rect(&mut self.render_cells, x + 34.0, y + 17.0, 8, 4, body_dark);
        if flash {
            push_tool_rect(&mut self.render_cells, cursor.x + 10.0, cursor.y - 3.0, 18, 6, AppColor::from_argb(235, 255, 171, 77));
            push_tool_rect(&mut self.render_cells, cursor.x + 15.0, cursor.y - 8.0, 8, 16, AppColor::from_argb(225, 255, 226, 98));
        }
    }

    fn render_cells(&self) -> &[SandRenderCell] {
        &self.render_cells
    }
}

#[derive(Debug)]
struct PortalPairTool {
    placing: bool,
    first: Option<PortalAnchor>,
    second: Option<PortalAnchor>,
    was_left_down: bool,
    was_right_down: bool,
    cooldown_until: HashMap<u64, f64>,
    render_cells: Vec<SandRenderCell>,
}

impl Default for PortalPairTool {
    fn default() -> Self {
        Self {
            placing: false,
            first: None,
            second: None,
            was_left_down: false,
            was_right_down: false,
            cooldown_until: HashMap::new(),
            render_cells: Vec::with_capacity(4096),
        }
    }
}

impl PortalPairTool {
    fn start_placement(&mut self) {
        self.placing = true;
        self.first = None;
        self.second = None;
        self.was_left_down = false;
        self.was_right_down = false;
        self.cooldown_until.clear();
        self.render_cells.clear();
    }

    fn stop_placement(&mut self) {
        self.placing = false;
        self.was_left_down = false;
        self.was_right_down = false;
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn needs_interactive(&self) -> bool {
        self.placing
    }

    fn has_portal_pair(&self) -> bool {
        self.first.is_some() && self.second.is_some()
    }

    fn update_input(
        &mut self,
        cursor: Vector2,
        bounds: RectF,
        is_left_down: bool,
        is_right_down: bool,
    ) -> Option<String> {
        if is_right_down && !self.was_right_down {
            self.clear();
            return Some("Portal placement cancelled.".to_string());
        }

        let mut message = None;
        if is_left_down && !self.was_left_down {
            let anchor = PortalAnchor::nearest(cursor, bounds);
            if self.first.is_none() {
                self.first = Some(anchor);
                message = Some("Portal A placed. Click another wall for Portal B.".to_string());
            } else {
                self.second = Some(anchor);
                self.placing = false;
                self.cooldown_until.clear();
                message = Some("Portal pair linked. Objects can now travel between the mouths.".to_string());
            }
        }

        self.was_left_down = is_left_down;
        self.was_right_down = is_right_down;
        message
    }

    fn teleport_candidates(
        &mut self,
        objects: &[ObjectState],
        bounds: RectF,
        dt: f32,
        now_seconds: f64,
    ) -> Vec<PortalTeleport> {
        self.cooldown_until.retain(|_, until| *until > now_seconds);
        let Some(first) = self.first else {
            return Vec::new();
        };
        let Some(second) = self.second else {
            return Vec::new();
        };

        let mut teleports = Vec::new();
        for object in objects {
            if !self.can_teleport_object(object, now_seconds) {
                continue;
            }
            let teleport = portal_teleport_from_object(object, first, second, bounds, dt)
                .or_else(|| portal_teleport_from_object(object, second, first, bounds, dt));
            if let Some(teleport) = teleport {
                self.cooldown_until
                    .insert(object.id, now_seconds + PORTAL_COOLDOWN_SECONDS);
                teleports.push(teleport);
            }
        }
        teleports
    }

    fn can_teleport_object(&self, object: &ObjectState, now_seconds: f64) -> bool {
        object.body.collidable
            && !object.is_dragging
            && !object.body.is_dragging
            && object.depth_z >= -1.0
            && object.body.velocity.length_squared() > 45.0 * 45.0
            && !self
                .cooldown_until
                .get(&object.id)
                .is_some_and(|until| *until > now_seconds)
            && !matches!(
                object.visual_kind,
                ObjectVisualKind::BasketballHoop
                    | ObjectVisualKind::GamePlank
                    | ObjectVisualKind::ImportedModel
                    | ObjectVisualKind::ScreenShard
                    | ObjectVisualKind::QuadDrone
            )
    }

    fn rebuild_render_cells(&mut self, cursor: Vector2, bounds: RectF, now_seconds: f64) {
        self.render_cells.clear();
        let pulse = ((now_seconds * 5.4).sin() as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
        if let Some(first) = self.first {
            push_portal_anchor_cells(
                &mut self.render_cells,
                first,
                AppColor::from_argb(235, 102, 235, 255),
                bounds,
                pulse,
                false,
            );
        }
        if let Some(second) = self.second {
            push_portal_anchor_cells(
                &mut self.render_cells,
                second,
                AppColor::from_argb(235, 255, 170, 68),
                bounds,
                1.0 - pulse,
                false,
            );
        }
        if self.placing {
            let preview = PortalAnchor::nearest(cursor, bounds);
            push_portal_anchor_cells(
                &mut self.render_cells,
                preview,
                AppColor::from_argb(155, 242, 248, 255),
                bounds,
                pulse,
                true,
            );
            if let Some(first) = self.first {
                push_measure_line(
                    &mut self.render_cells,
                    first.center,
                    preview.center,
                    AppColor::from_argb(88, 164, 220, 255),
                    2,
                );
            }
        }
    }

    fn render_cells(&self) -> &[SandRenderCell] {
        &self.render_cells
    }
}

#[derive(Clone, Copy, Debug)]
struct PortalAnchor {
    wall: PortalWall,
    center: Vector2,
    half_len: f32,
}

impl PortalAnchor {
    fn nearest(cursor: Vector2, bounds: RectF) -> Self {
        let left_distance = (cursor.x - bounds.left()).abs();
        let right_distance = (bounds.right() - cursor.x).abs();
        let top_distance = (cursor.y - bounds.top()).abs();
        let bottom_distance = (bounds.bottom() - cursor.y).abs();
        let mut wall = PortalWall::Left;
        let mut distance = left_distance;
        for (candidate_wall, candidate_distance) in [
            (PortalWall::Right, right_distance),
            (PortalWall::Top, top_distance),
            (PortalWall::Bottom, bottom_distance),
        ] {
            if candidate_distance < distance {
                wall = candidate_wall;
                distance = candidate_distance;
            }
        }

        let half_len = match wall {
            PortalWall::Left | PortalWall::Right => PORTAL_HALF_LENGTH_PIXELS.min(bounds.height * 0.42),
            PortalWall::Top | PortalWall::Bottom => PORTAL_HALF_LENGTH_PIXELS.min(bounds.width * 0.42),
        };
        let edge_padding = half_len + 10.0;
        let center = match wall {
            PortalWall::Left => Vector2::new(bounds.left(), cursor.y.clamp(bounds.top() + edge_padding, bounds.bottom() - edge_padding)),
            PortalWall::Right => {
                Vector2::new(bounds.right(), cursor.y.clamp(bounds.top() + edge_padding, bounds.bottom() - edge_padding))
            },
            PortalWall::Top => Vector2::new(cursor.x.clamp(bounds.left() + edge_padding, bounds.right() - edge_padding), bounds.top()),
            PortalWall::Bottom => {
                Vector2::new(cursor.x.clamp(bounds.left() + edge_padding, bounds.right() - edge_padding), bounds.bottom())
            },
        };
        Self {
            wall,
            center,
            half_len,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PortalWall {
    Left,
    Right,
    Top,
    Bottom,
}

impl PortalWall {
    fn normal(self) -> Vector2 {
        match self {
            Self::Left => Vector2::new(1.0, 0.0),
            Self::Right => Vector2::new(-1.0, 0.0),
            Self::Top => Vector2::new(0.0, 1.0),
            Self::Bottom => Vector2::new(0.0, -1.0),
        }
    }

    fn tangent(self) -> Vector2 {
        match self {
            Self::Left | Self::Right => Vector2::new(0.0, 1.0),
            Self::Top | Self::Bottom => Vector2::new(1.0, 0.0),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PortalTeleport {
    id: u64,
    position: Vector2,
    velocity: Vector2,
}

fn portal_teleport_from_object(
    object: &ObjectState,
    source: PortalAnchor,
    destination: PortalAnchor,
    bounds: RectF,
    dt: f32,
) -> Option<PortalTeleport> {
    let center = object_center(object);
    if center.x < bounds.left() - object.body.width
        || center.x > bounds.right() + object.body.width
        || center.y < bounds.top() - object.body.height
        || center.y > bounds.bottom() + object.body.height
    {
        return None;
    }

    let normal = source.wall.normal();
    let tangent = source.wall.tangent();
    let velocity = object.body.velocity;
    let incoming_speed = -vector_dot(velocity, normal);
    if incoming_speed < 35.0 {
        return None;
    }

    let half_normal = object_half_extent_along(object, normal);
    let half_tangent = object_half_extent_along(object, tangent);
    let current_distance = vector_dot(center - source.center, normal) - half_normal;
    let predicted_center = center + velocity * dt.max(1.0 / 240.0);
    let predicted_distance = vector_dot(predicted_center - source.center, normal) - half_normal;
    if current_distance > PORTAL_EDGE_MARGIN_PIXELS && predicted_distance > PORTAL_EDGE_MARGIN_PIXELS {
        return None;
    }

    let tangent_offset = vector_dot(center - source.center, tangent);
    if tangent_offset.abs() > source.half_len + half_tangent {
        return None;
    }

    let exit_normal = destination.wall.normal();
    let exit_tangent = destination.wall.tangent();
    let half_exit_normal = object_half_extent_along(object, exit_normal);
    let half_exit_tangent = object_half_extent_along(object, exit_tangent);
    let max_exit_offset = (destination.half_len - half_exit_tangent.min(destination.half_len * 0.72)).max(0.0);
    let exit_offset = tangent_offset.clamp(-max_exit_offset, max_exit_offset);
    let exit_center =
        destination.center + exit_tangent * exit_offset + exit_normal * (half_exit_normal + PORTAL_EXIT_OFFSET_PIXELS);
    let tangent_velocity = vector_dot(velocity, tangent);
    let new_velocity = exit_normal * incoming_speed.max(180.0) + exit_tangent * tangent_velocity;

    Some(PortalTeleport {
        id: object.id,
        position: Vector2::new(
            exit_center.x - object.body.width * 0.5,
            exit_center.y - object.body.height * 0.5,
        ),
        velocity: clamp_vector(new_velocity, 2800.0),
    })
}

fn push_portal_anchor_cells(
    cells: &mut Vec<SandRenderCell>,
    anchor: PortalAnchor,
    color: AppColor,
    bounds: RectF,
    pulse: f32,
    preview: bool,
) {
    let alpha_scale = if preview { 0.56 } else { 1.0 };
    let glow_alpha = ((84.0 + pulse * 76.0) * alpha_scale).round() as u8;
    let core_alpha = ((182.0 + pulse * 52.0) * alpha_scale).round() as u8;
    let glow = AppColor::from_argb(glow_alpha, color.r, color.g, color.b);
    let core = AppColor::from_argb(core_alpha, color.r, color.g, color.b);
    let hot = AppColor::from_argb(
        ((214.0 + pulse * 32.0) * alpha_scale).round() as u8,
        255,
        252,
        224,
    );
    let len = (anchor.half_len * 2.0).round() as i32;

    match anchor.wall {
        PortalWall::Left | PortalWall::Right => {
            let x = if anchor.wall == PortalWall::Left {
                bounds.left()
            } else {
                bounds.right() - 12.0
            };
            let y = anchor.center.y - anchor.half_len;
            push_tool_rect(cells, x - 2.0, y - 8.0, 16, len + 16, glow);
            push_tool_rect(cells, x + 2.0, y, 8, len, core);
            push_tool_rect(cells, x + 5.0, anchor.center.y - 18.0, 3, 36, hot);
            push_lasso_dot(cells, anchor.center + anchor.wall.normal() * 16.0, 7, hot);
        },
        PortalWall::Top | PortalWall::Bottom => {
            let x = anchor.center.x - anchor.half_len;
            let y = if anchor.wall == PortalWall::Top {
                bounds.top()
            } else {
                bounds.bottom() - 12.0
            };
            push_tool_rect(cells, x - 8.0, y - 2.0, len + 16, 16, glow);
            push_tool_rect(cells, x, y + 2.0, len, 8, core);
            push_tool_rect(cells, anchor.center.x - 18.0, y + 5.0, 36, 3, hot);
            push_lasso_dot(cells, anchor.center + anchor.wall.normal() * 16.0, 7, hot);
        },
    }
}

#[derive(Debug)]
struct MeasureTool {
    active: bool,
    dragging: bool,
    has_measurement: bool,
    start: Vector2,
    end: Vector2,
    was_left_down: bool,
    was_right_down: bool,
    render_cells: Vec<SandRenderCell>,
}

impl Default for MeasureTool {
    fn default() -> Self {
        Self {
            active: false,
            dragging: false,
            has_measurement: false,
            start: Vector2::ZERO,
            end: Vector2::ZERO,
            was_left_down: false,
            was_right_down: false,
            render_cells: Vec::with_capacity(4096),
        }
    }
}

impl MeasureTool {
    fn toggle(&mut self) -> bool {
        self.active = !self.active;
        self.dragging = false;
        self.was_left_down = false;
        self.was_right_down = false;
        if !self.active {
            self.has_measurement = false;
            self.render_cells.clear();
        }
        self.active
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn update(&mut self, cursor: Vector2, is_left_down: bool, is_right_down: bool) {
        if is_right_down && !self.was_right_down {
            self.dragging = false;
            self.has_measurement = false;
            self.render_cells.clear();
        }

        if is_left_down && !self.was_left_down {
            self.dragging = true;
            self.has_measurement = true;
            self.start = cursor;
            self.end = cursor;
            self.rebuild_render_cells();
        } else if is_left_down && self.dragging {
            self.end = cursor;
            self.rebuild_render_cells();
        } else if !is_left_down && self.was_left_down && self.dragging {
            self.dragging = false;
            self.end = cursor;
            self.rebuild_render_cells();
        }

        self.was_left_down = is_left_down;
        self.was_right_down = is_right_down;
    }

    fn render_cells(&self) -> &[SandRenderCell] {
        &self.render_cells
    }

    fn delta(&self) -> Vector2 {
        self.end - self.start
    }

    fn distance(&self) -> f32 {
        self.delta().length_squared().sqrt()
    }

    fn angle_degrees(&self) -> f32 {
        let delta = self.delta();
        delta.y.atan2(delta.x).to_degrees()
    }

    fn rebuild_render_cells(&mut self) {
        self.render_cells.clear();
        if !self.has_measurement {
            return;
        }

        let corner = Vector2::new(self.end.x, self.start.y);
        push_measure_line(
            &mut self.render_cells,
            self.start,
            corner,
            AppColor::from_argb(96, 112, 195, 255),
            1,
        );
        push_measure_line(
            &mut self.render_cells,
            corner,
            self.end,
            AppColor::from_argb(96, 112, 195, 255),
            1,
        );
        push_measure_line(
            &mut self.render_cells,
            self.start,
            self.end,
            AppColor::from_argb(235, 255, 226, 98),
            2,
        );
        push_measure_handle(&mut self.render_cells, self.start, AppColor::from_argb(245, 118, 232, 180));
        push_measure_handle(&mut self.render_cells, self.end, AppColor::from_argb(245, 255, 128, 96));
    }
}

#[derive(Debug)]
struct LassoTool {
    active: bool,
    drawing: bool,
    path: Vec<Vector2>,
    captured_ids: Vec<u64>,
    rope_lengths: Vec<f32>,
    rope_anchor: Vector2,
    previous_anchor: Vector2,
    anchor_velocity: Vector2,
    was_left_down: bool,
    was_right_down: bool,
    render_cells: Vec<SandRenderCell>,
}

#[derive(Default)]
struct LassoInputOutcome {
    primary_id: Option<u64>,
    status_message: Option<String>,
}

impl Default for LassoTool {
    fn default() -> Self {
        Self {
            active: false,
            drawing: false,
            path: Vec::with_capacity(220),
            captured_ids: Vec::with_capacity(16),
            rope_lengths: Vec::with_capacity(16),
            rope_anchor: Vector2::ZERO,
            previous_anchor: Vector2::ZERO,
            anchor_velocity: Vector2::ZERO,
            was_left_down: false,
            was_right_down: false,
            render_cells: Vec::with_capacity(8192),
        }
    }
}

impl LassoTool {
    const MAX_PATH_POINTS: usize = 220;
    const MAX_CAPTURED_OBJECTS: usize = 18;
    const MIN_POINT_DISTANCE_SQUARED: f32 = 36.0;
    const MIN_POLYGON_AREA: f32 = 1100.0;
    const MIN_ROPE_LENGTH: f32 = 46.0;
    const MAX_ROPE_LENGTH: f32 = 420.0;
    const TENSION_STIFFNESS: f32 = 18.0;
    const TENSION_DAMPING: f32 = 0.82;
    const TANGENTIAL_COUPLING: f32 = 0.055;
    const MAX_TENSION_DELTA: f32 = 1900.0;

    fn activate(&mut self, cursor: Vector2) {
        self.active = true;
        self.drawing = false;
        self.path.clear();
        self.captured_ids.clear();
        self.rope_lengths.clear();
        self.rope_anchor = cursor;
        self.previous_anchor = cursor;
        self.anchor_velocity = Vector2::ZERO;
        self.was_left_down = false;
        self.was_right_down = false;
        self.render_cells.clear();
    }

    fn deactivate(&mut self) {
        self.release_capture();
        self.active = false;
        self.drawing = false;
        self.path.clear();
        self.render_cells.clear();
        self.was_left_down = false;
        self.was_right_down = false;
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn needs_interactive(&self) -> bool {
        self.active
    }

    fn has_capture(&self) -> bool {
        !self.captured_ids.is_empty()
    }

    fn captured_count(&self) -> usize {
        self.captured_ids.len()
    }

    fn path_len(&self) -> usize {
        self.path.len()
    }

    fn render_cells(&self) -> &[SandRenderCell] {
        &self.render_cells
    }

    fn update_input(
        &mut self,
        cursor: Vector2,
        is_left_down: bool,
        is_right_down: bool,
        _now_seconds: f64,
        objects: &[ObjectState],
        hit_tester: &HitTester,
    ) -> LassoInputOutcome {
        let mut outcome = LassoInputOutcome::default();

        if is_right_down && !self.was_right_down {
            self.release_capture();
            self.drawing = false;
            self.path.clear();
            self.render_cells.clear();
            outcome.status_message = Some("Lasso released.".to_string());
        } else if is_left_down && !self.was_left_down {
            self.release_capture();
            self.drawing = true;
            self.path.clear();
            self.push_path_point(cursor, true);
            self.rope_anchor = cursor;
            self.previous_anchor = cursor;
            self.anchor_velocity = Vector2::ZERO;
        } else if is_left_down && self.drawing {
            self.push_path_point(cursor, false);
        } else if !is_left_down && self.was_left_down && self.drawing {
            self.push_path_point(cursor, true);
            self.drawing = false;
            let captured = self.capture_objects(objects, hit_tester, cursor);
            outcome.primary_id = self.captured_ids.first().copied();
            outcome.status_message = Some(if captured == 0 {
                "Lasso missed. Drag a tighter loop or click an object.".to_string()
            } else if captured == 1 {
                "Lasso snared 1 object.".to_string()
            } else {
                format!("Lasso snared {captured} objects.")
            });
        }

        self.was_left_down = is_left_down;
        self.was_right_down = is_right_down;
        outcome
    }

    fn compute_velocity_deltas(&mut self, objects: &[ObjectState], cursor: Vector2, dt: f32) -> Vec<(u64, Vector2)> {
        if self.captured_ids.is_empty() {
            self.previous_anchor = cursor;
            self.rope_anchor = cursor;
            self.anchor_velocity = Vector2::ZERO;
            return Vec::new();
        }

        let dt = dt.clamp(1.0 / 240.0, 1.0 / 20.0);
        self.anchor_velocity = (cursor - self.previous_anchor) / dt;
        self.previous_anchor = cursor;
        self.rope_anchor = cursor;

        let mut deltas = Vec::with_capacity(self.captured_ids.len());
        for (index, id) in self.captured_ids.iter().copied().enumerate() {
            let Some(object) = objects.iter().find(|object| object.id == id) else {
                continue;
            };
            let current_center = object_center(object);
            let rope_length = self.rope_lengths.get(index).copied().unwrap_or(Self::MIN_ROPE_LENGTH);
            let anchor_to_object = current_center - self.rope_anchor;
            let distance = anchor_to_object.length_squared().sqrt();
            if distance <= f32::EPSILON {
                continue;
            }

            let unit = anchor_to_object / distance;
            let stretch = distance - rope_length;
            let relative_velocity = object.body.velocity - self.anchor_velocity;
            let radial_speed = (relative_velocity.x * unit.x) + (relative_velocity.y * unit.y);
            let tangent_anchor = self.anchor_velocity - unit * ((self.anchor_velocity.x * unit.x) + (self.anchor_velocity.y * unit.y));

            let mut delta = tangent_anchor * Self::TANGENTIAL_COUPLING;
            if stretch > 0.0 {
                let correction_speed = (stretch * Self::TENSION_STIFFNESS) + radial_speed.max(0.0) * Self::TENSION_DAMPING;
                delta -= unit * correction_speed;
            }

            delta = clamp_vector(delta, Self::MAX_TENSION_DELTA);
            if delta.length_squared() > 0.01 {
                deltas.push((id, delta));
            }
        }

        deltas
    }

    fn rebuild_render_cells(&mut self, cursor: Vector2, objects: &[ObjectState], elapsed_seconds: f64) {
        self.render_cells.clear();
        if !self.active {
            return;
        }

        let time = elapsed_seconds as f32;
        if self.drawing && !self.path.is_empty() {
            push_lasso_polyline(&mut self.render_cells, &self.path, time, false);
            if let Some(last) = self.path.last().copied() {
                push_lasso_rope_line(
                    &mut self.render_cells,
                    last,
                    cursor,
                    time,
                    0.4,
                    AppColor::from_argb(170, 255, 222, 112),
                    3,
                );
            }
            if self.path.len() > 2 {
                push_lasso_rope_line(
                    &mut self.render_cells,
                    cursor,
                    self.path[0],
                    time,
                    1.2,
                    AppColor::from_argb(82, 104, 226, 255),
                    2,
                );
            }
            push_lasso_knot(&mut self.render_cells, cursor, AppColor::from_argb(235, 255, 246, 174));
            return;
        }

        if self.has_capture() {
            push_lasso_knot(&mut self.render_cells, cursor, AppColor::from_argb(245, 255, 250, 196));
            for (index, id) in self.captured_ids.iter().copied().enumerate() {
                let Some(object) = objects.iter().find(|object| object.id == id) else {
                    continue;
                };
                let center = object_center(object);
                push_lasso_rope_line(
                    &mut self.render_cells,
                    cursor,
                    center,
                    time,
                    index as f32 * 0.73,
                    AppColor::from_argb(210, 255, 192, 72),
                    4,
                );
                push_lasso_object_ring(&mut self.render_cells, object, time + index as f32 * 0.2);
            }
        } else {
            push_lasso_idle_coil(&mut self.render_cells, cursor, time);
        }
    }

    fn push_path_point(&mut self, point: Vector2, force: bool) {
        if self.path.len() >= Self::MAX_PATH_POINTS {
            if force {
                if let Some(last) = self.path.last_mut() {
                    *last = point;
                }
            }
            return;
        }

        let should_push = force
            || self
                .path
                .last()
                .map(|last| (point - *last).length_squared() >= Self::MIN_POINT_DISTANCE_SQUARED)
                .unwrap_or(true);
        if should_push {
            self.path.push(point);
        }
    }

    fn capture_objects(&mut self, objects: &[ObjectState], hit_tester: &HitTester, cursor: Vector2) -> usize {
        self.captured_ids.clear();
        self.rope_lengths.clear();
        self.rope_anchor = cursor;
        self.previous_anchor = cursor;
        self.anchor_velocity = Vector2::ZERO;

        let mut ids = Vec::new();
        if self.path.len() >= 3 && polygon_area_abs(&self.path) >= Self::MIN_POLYGON_AREA {
            let mut hits: Vec<(i32, u64)> = objects
                .iter()
                .filter(|object| object_is_lassoable(object) && object_touches_lasso(object, &self.path))
                .map(|object| (object.z_index, object.id))
                .collect();
            hits.sort_by(|left, right| right.0.cmp(&left.0));
            ids.extend(hits.into_iter().map(|(_, id)| id));
        }

        if ids.is_empty() {
            if let Some(id) = hit_tester.hit_test_topmost(objects, cursor).map(|object| object.id) {
                ids.push(id);
            }
        }

        for id in ids.into_iter().take(Self::MAX_CAPTURED_OBJECTS) {
            let Some(object) = objects.iter().find(|object| object.id == id) else {
                continue;
            };
            let center = object_center(object);
            self.captured_ids.push(id);
            self.rope_lengths
                .push((center - cursor).length_squared().sqrt().clamp(Self::MIN_ROPE_LENGTH, Self::MAX_ROPE_LENGTH));
        }

        self.captured_ids.len()
    }

    fn release_capture(&mut self) {
        self.captured_ids.clear();
        self.rope_lengths.clear();
        self.anchor_velocity = Vector2::ZERO;
    }
}

#[derive(Debug)]
struct SpotlightTool {
    active: bool,
    render_cells: Vec<SandRenderCell>,
}

impl Default for SpotlightTool {
    fn default() -> Self {
        Self {
            active: false,
            render_cells: Vec::with_capacity(6000),
        }
    }
}

impl SpotlightTool {
    const ROW_STEP: i32 = 3;
    const SHADOW_LAYERS: [(f32, f32, u8); 5] = [
        (330.0, 255.0, 18),
        (282.0, 218.0, 22),
        (232.0, 178.0, 26),
        (184.0, 140.0, 30),
        (132.0, 100.0, 34),
    ];
    const GLOW_LAYERS: [(f32, f32, u8, u8, u8, u8); 4] = [
        (280.0, 212.0, 8, 255, 198, 112),
        (198.0, 150.0, 12, 255, 224, 154),
        (126.0, 94.0, 18, 255, 242, 200),
        (70.0, 52.0, 16, 255, 255, 234),
    ];

    fn toggle(&mut self, cursor: Vector2, bounds: RectF) -> bool {
        self.active = !self.active;
        if self.active {
            self.rebuild_render_cells(cursor, bounds);
        } else {
            self.render_cells.clear();
        }
        self.active
    }

    fn clear(&mut self) {
        self.active = false;
        self.render_cells.clear();
    }

    fn update(&mut self, cursor: Vector2, bounds: RectF) {
        if self.active {
            self.rebuild_render_cells(cursor, bounds);
        }
    }

    fn render_cells(&self) -> &[SandRenderCell] {
        &self.render_cells
    }

    fn rebuild_render_cells(&mut self, cursor: Vector2, bounds: RectF) {
        let width = bounds.width.ceil().max(1.0) as i32;
        let height = bounds.height.ceil().max(1.0) as i32;
        let cursor_x = cursor.x.clamp(0.0, width as f32);
        let cursor_y = cursor.y.clamp(0.0, height as f32);

        self.render_cells.clear();
        let row_count = (height / Self::ROW_STEP.max(1)) as usize + 1;
        self.render_cells.reserve(row_count * 14);

        for (radius_x, radius_y, alpha) in Self::SHADOW_LAYERS {
            push_spotlight_shadow_outside(
                &mut self.render_cells,
                width,
                height,
                cursor_x,
                cursor_y,
                radius_x,
                radius_y,
                Self::ROW_STEP,
                AppColor::from_argb(alpha, 0, 0, 0),
            );
        }

        for (radius_x, radius_y, alpha, red, green, blue) in Self::GLOW_LAYERS {
            push_spotlight_fill(
                &mut self.render_cells,
                width,
                height,
                cursor_x,
                cursor_y,
                radius_x,
                radius_y,
                Self::ROW_STEP,
                AppColor::from_argb(alpha, red, green, blue),
            );
        }
    }
}

fn push_spotlight_shadow_outside(
    cells: &mut Vec<SandRenderCell>,
    width: i32,
    height: i32,
    center_x: f32,
    center_y: f32,
    radius_x: f32,
    radius_y: f32,
    row_step: i32,
    color: AppColor,
) {
    let row_step = row_step.max(1);
    let mut y = 0;
    while y < height {
        let row_height = row_step.min(height - y).max(1);
        let sample_y = y as f32 + row_height as f32 * 0.5;
        let normalized_y = (sample_y - center_y) / radius_y.max(1.0);
        if normalized_y.abs() >= 1.0 {
            cells.push(SandRenderCell {
                x: 0,
                y,
                width,
                height: row_height,
                color,
            });
        } else {
            let clear_half_width = radius_x.max(1.0) * (1.0 - normalized_y * normalized_y).sqrt();
            let left_width = (center_x - clear_half_width).floor().clamp(0.0, width as f32) as i32;
            let right_start = (center_x + clear_half_width).ceil().clamp(0.0, width as f32) as i32;
            if left_width > 0 {
                cells.push(SandRenderCell {
                    x: 0,
                    y,
                    width: left_width,
                    height: row_height,
                    color,
                });
            }
            if right_start < width {
                cells.push(SandRenderCell {
                    x: right_start,
                    y,
                    width: width - right_start,
                    height: row_height,
                    color,
                });
            }
        }
        y += row_step;
    }
}

fn push_spotlight_fill(
    cells: &mut Vec<SandRenderCell>,
    width: i32,
    height: i32,
    center_x: f32,
    center_y: f32,
    radius_x: f32,
    radius_y: f32,
    row_step: i32,
    color: AppColor,
) {
    let row_step = row_step.max(1);
    let mut y = (center_y - radius_y).floor().max(0.0) as i32;
    let bottom = (center_y + radius_y).ceil().min(height as f32) as i32;
    while y < bottom {
        let row_height = row_step.min(bottom - y).max(1);
        let sample_y = y as f32 + row_height as f32 * 0.5;
        let normalized_y = (sample_y - center_y) / radius_y.max(1.0);
        if normalized_y.abs() < 1.0 {
            let half_width = radius_x.max(1.0) * (1.0 - normalized_y * normalized_y).sqrt();
            let left = (center_x - half_width).floor().clamp(0.0, width as f32) as i32;
            let right = (center_x + half_width).ceil().clamp(0.0, width as f32) as i32;
            if right > left {
                cells.push(SandRenderCell {
                    x: left,
                    y,
                    width: right - left,
                    height: row_height,
                    color,
                });
            }
        }
        y += row_step;
    }
}

fn push_measure_line(cells: &mut Vec<SandRenderCell>, start: Vector2, end: Vector2, color: AppColor, thickness: i32) {
    let delta = end - start;
    let steps = delta.x.abs().max(delta.y.abs()).ceil().max(1.0) as i32;
    let thickness = thickness.max(1);
    let offset = thickness / 2;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let x = (start.x + delta.x * t).round() as i32;
        let y = (start.y + delta.y * t).round() as i32;
        cells.push(SandRenderCell {
            x: x - offset,
            y: y - offset,
            width: thickness,
            height: thickness,
            color,
        });
    }
}

fn push_measure_handle(cells: &mut Vec<SandRenderCell>, center: Vector2, color: AppColor) {
    let x = center.x.round() as i32;
    let y = center.y.round() as i32;
    push_measure_line(
        cells,
        Vector2::new((x - 8) as f32, y as f32),
        Vector2::new((x + 8) as f32, y as f32),
        color,
        2,
    );
    push_measure_line(
        cells,
        Vector2::new(x as f32, (y - 8) as f32),
        Vector2::new(x as f32, (y + 8) as f32),
        color,
        2,
    );
}

fn object_center(object: &ObjectState) -> Vector2 {
    Vector2::new(
        object.body.position.x + object.body.width * 0.5,
        object.body.position.y + object.body.height * 0.5,
    )
}

fn vector_dot(left: Vector2, right: Vector2) -> f32 {
    left.x * right.x + left.y * right.y
}

fn object_half_extent_along(object: &ObjectState, axis: Vector2) -> f32 {
    let axis_x = axis.x.abs();
    let axis_y = axis.y.abs();
    (object.body.width * 0.5 * axis_x) + (object.body.height * 0.5 * axis_y)
}

fn object_is_cleanup_bin_collectable(object: &ObjectState) -> bool {
    object.body.collidable
        && object.body.width <= 150.0
        && object.body.height <= 150.0
        && !matches!(
            object.visual_kind,
            ObjectVisualKind::RobotBuddy
                | ObjectVisualKind::QuadDrone
                | ObjectVisualKind::Fan
                | ObjectVisualKind::Snail
                | ObjectVisualKind::FoxBuddy
                | ObjectVisualKind::GamePlank
                | ObjectVisualKind::GameTarget
                | ObjectVisualKind::BasketballHoop
                | ObjectVisualKind::ImportedModel
                | ObjectVisualKind::ScreenShard
        )
}

fn object_is_lassoable(object: &ObjectState) -> bool {
    object.body.collidable && object.depth_z >= -1.0
}

fn object_touches_lasso(object: &ObjectState, polygon: &[Vector2]) -> bool {
    let body = object.body;
    let left = body.position.x;
    let top = body.position.y;
    let right = body.position.x + body.width;
    let bottom = body.position.y + body.height;
    let center = object_center(object);
    let sample_points = [
        center,
        Vector2::new(left, top),
        Vector2::new(right, top),
        Vector2::new(right, bottom),
        Vector2::new(left, bottom),
    ];
    sample_points
        .iter()
        .any(|point| point_in_polygon(*point, polygon))
}

fn point_in_polygon(point: Vector2, polygon: &[Vector2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }

    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let a = polygon[current];
        let b = polygon[previous];
        let crosses = (a.y > point.y) != (b.y > point.y);
        if crosses {
            let denominator = b.y - a.y;
            if denominator.abs() > f32::EPSILON {
                let x_intersection = (b.x - a.x) * (point.y - a.y) / denominator + a.x;
                if point.x < x_intersection {
                    inside = !inside;
                }
            }
        }
        previous = current;
    }
    inside
}

fn polygon_area_abs(points: &[Vector2]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }

    let mut area = 0.0;
    for index in 0..points.len() {
        let next = points[(index + 1) % points.len()];
        let current = points[index];
        area += current.x * next.y - next.x * current.y;
    }
    (area * 0.5).abs()
}

fn push_lasso_polyline(cells: &mut Vec<SandRenderCell>, points: &[Vector2], time: f32, closed: bool) {
    for pair in points.windows(2) {
        push_lasso_rope_line(
            cells,
            pair[0],
            pair[1],
            time,
            pair[0].x * 0.013 + pair[0].y * 0.009,
            AppColor::from_argb(205, 255, 194, 76),
            4,
        );
    }
    if closed && points.len() > 2 {
        push_lasso_rope_line(
            cells,
            *points.last().expect("checked len"),
            points[0],
            time,
            1.8,
            AppColor::from_argb(190, 104, 226, 255),
            3,
        );
    }
}

fn push_lasso_rope_line(
    cells: &mut Vec<SandRenderCell>,
    start: Vector2,
    end: Vector2,
    time: f32,
    phase: f32,
    color: AppColor,
    thickness: i32,
) {
    let delta = end - start;
    let length = delta.length_squared().sqrt();
    if length < 1.0 {
        push_lasso_dot(cells, start, thickness.max(2), color);
        return;
    }

    let direction = delta / length;
    let normal = Vector2::new(-direction.y, direction.x);
    let steps = (length / 5.0).ceil().clamp(1.0, 260.0) as i32;
    let shadow = AppColor::from_argb(105, 4, 7, 10);
    let highlight = AppColor::from_argb(color.a.saturating_add(22), 255, 252, 212);

    for pass in 0..3 {
        let (pass_color, pass_thickness, pass_offset) = match pass {
            0 => (shadow, thickness + 4, 0.0),
            1 => (color, thickness, 0.0),
            _ => (highlight, 1, -1.2),
        };
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let taper = (t * 3.1415927).sin().max(0.18);
            let wave = ((t * 9.0 + time * 2.8 + phase) * 6.2831855).sin() * 3.2 * taper + pass_offset;
            let point = start + delta * t + normal * wave;
            push_lasso_dot(cells, point, pass_thickness, pass_color);
        }
    }
}

fn push_lasso_object_ring(cells: &mut Vec<SandRenderCell>, object: &ObjectState, time: f32) {
    let center = object_center(object);
    let radius_x = (object.body.width * 0.62).max(24.0);
    let radius_y = (object.body.height * 0.62).max(24.0);
    let mut previous = None;
    let segment_count = 42;
    for index in 0..=segment_count {
        let theta = index as f32 / segment_count as f32 * 6.2831855 + time.sin() * 0.08;
        let wobble = 1.0 + (theta * 3.0 + time * 3.2).sin() * 0.045;
        let point = Vector2::new(center.x + theta.cos() * radius_x * wobble, center.y + theta.sin() * radius_y * wobble);
        if let Some(previous) = previous {
            push_lasso_rope_line(
                cells,
                previous,
                point,
                time,
                theta,
                AppColor::from_argb(188, 255, 128, 86),
                3,
            );
        }
        previous = Some(point);
    }
}

fn push_lasso_idle_coil(cells: &mut Vec<SandRenderCell>, cursor: Vector2, time: f32) {
    let mut previous = None;
    let segment_count = 58;
    for index in 0..=segment_count {
        let t = index as f32 / segment_count as f32;
        let theta = t * 6.2831855 * 1.85 + time * 1.2;
        let radius = 11.0 + t * 18.0;
        let point = cursor + Vector2::new(theta.cos() * radius, theta.sin() * radius * 0.68);
        if let Some(previous) = previous {
            push_lasso_rope_line(
                cells,
                previous,
                point,
                time,
                theta,
                AppColor::from_argb(170, 255, 202, 94),
                3,
            );
        }
        previous = Some(point);
    }
    push_lasso_knot(cells, cursor, AppColor::from_argb(220, 255, 246, 190));
}

fn push_lasso_knot(cells: &mut Vec<SandRenderCell>, center: Vector2, color: AppColor) {
    push_lasso_dot(cells, center, 7, AppColor::from_argb(105, 4, 7, 10));
    push_lasso_dot(cells, center, 4, color);
    push_lasso_dot(cells, center + Vector2::new(1.0, -1.0), 1, AppColor::from_argb(245, 255, 255, 240));
}

fn push_lasso_dot(cells: &mut Vec<SandRenderCell>, point: Vector2, size: i32, color: AppColor) {
    let size = size.max(1);
    let offset = size / 2;
    cells.push(SandRenderCell {
        x: point.x.round() as i32 - offset,
        y: point.y.round() as i32 - offset,
        width: size,
        height: size,
        color,
    });
}

fn push_tool_rect(cells: &mut Vec<SandRenderCell>, x: f32, y: f32, width: i32, height: i32, color: AppColor) {
    if width <= 0 || height <= 0 {
        return;
    }
    cells.push(SandRenderCell {
        x: x.round() as i32,
        y: y.round() as i32,
        width,
        height,
        color,
    });
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

#[derive(Clone, Copy)]
struct DroneCarry {
    object_id: u64,
    picked_up_at: f64,
}

#[derive(Clone, Copy)]
struct DroneDropCooldown {
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
struct BasketballGame {
    active: bool,
    aiming: bool,
    ready: bool,
    score: u32,
    shots: u32,
    ball_id: Option<u64>,
    hoop_id: Option<u64>,
    tee: Vector2,
    hoop_center: Vector2,
    grab_offset: Vector2,
    launched_at: f64,
    scored_this_shot: bool,
    /// Ball center y from the previous frame, used to detect the ball dropping
    /// through the rim plane.
    last_ball_y: f32,
    /// Deepest depth reached during the current shot; distinguishes swishes
    /// from bank shots.
    min_depth_this_shot: f32,
    last_score_at: f64,
}

impl BasketballGame {
    fn new(bounds: RectF) -> Self {
        let geometry = hoop_geometry(BASKETBALL_HOOP_WIDTH, BASKETBALL_HOOP_HEIGHT);
        let rim_target_y = bounds.height * 0.40;
        Self {
            active: true,
            ready: true,
            tee: Vector2::new(bounds.width * 0.5, bounds.bottom() - 170.0),
            hoop_center: Vector2::new(bounds.width * 0.5, rim_target_y - geometry.rim_center_y),
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RainDrop {
    x: f32,
    y: f32,
    speed: f32,
    length: i32,
    shade: u8,
}

#[derive(Debug)]
struct WeatherWorld {
    active: bool,
    rng_state: u64,
    spawn_credit: f32,
    drops: Vec<RainDrop>,
    render_cells: Vec<SandRenderCell>,
}

impl Default for WeatherWorld {
    fn default() -> Self {
        Self {
            active: false,
            rng_state: 0xC0FFEE5EED1234AB,
            spawn_credit: 0.0,
            drops: Vec::with_capacity(900),
            render_cells: Vec::with_capacity(900),
        }
    }
}

impl WeatherWorld {
    const MAX_DROPS: usize = 900;
    const SPAWN_RATE_PER_SECOND: f32 = 360.0;
    const WIND_PIXELS_PER_SECOND: f32 = -86.0;

    fn toggle(&mut self, bounds: RectF) -> bool {
        self.active = !self.active;
        if self.active && self.drops.is_empty() {
            for _ in 0..180 {
                self.spawn_drop(bounds, true);
            }
            self.rebuild_render_cells(bounds);
        } else if !self.active {
            self.render_cells.clear();
        }
        self.active
    }

    fn clear(&mut self) {
        self.active = false;
        self.spawn_credit = 0.0;
        self.drops.clear();
        self.render_cells.clear();
    }

    fn drop_count(&self) -> usize {
        self.drops.len()
    }

    fn render_cells(&self) -> &[SandRenderCell] {
        &self.render_cells
    }

    fn step(&mut self, dt: f32, bounds: RectF) {
        let dt = dt.clamp(0.0, 1.0 / 20.0);
        self.spawn_credit += Self::SPAWN_RATE_PER_SECOND * dt;
        while self.spawn_credit >= 1.0 && self.drops.len() < Self::MAX_DROPS {
            self.spawn_credit -= 1.0;
            self.spawn_drop(bounds, false);
        }

        let floor = bounds.height + 28.0;
        let left_edge = -32.0;
        let mut index = 0;
        while index < self.drops.len() {
            let drop = &mut self.drops[index];
            drop.x += Self::WIND_PIXELS_PER_SECOND * dt;
            drop.y += drop.speed * dt;
            if drop.y > floor || drop.x < left_edge {
                self.drops.swap_remove(index);
            } else {
                index += 1;
            }
        }

        self.rebuild_render_cells(bounds);
    }

    fn spawn_drop(&mut self, bounds: RectF, scatter_y: bool) {
        let width = bounds.width.max(1.0);
        let x = self.random_range(0.0, width + 80.0);
        let y = if scatter_y {
            self.random_range(-bounds.height.max(1.0), 0.0)
        } else {
            self.random_range(-80.0, -4.0)
        };
        let speed = self.random_range(640.0, 1120.0);
        let length = self.random_range(9.0, 22.0).round() as i32;
        let shade = self.next_u32() as u8;
        self.drops.push(RainDrop {
            x,
            y,
            speed,
            length,
            shade,
        });
    }

    fn rebuild_render_cells(&mut self, _bounds: RectF) {
        self.render_cells.clear();
        self.render_cells.reserve(self.drops.len());
        for drop in &self.drops {
            let color = match drop.shade & 3 {
                0 => AppColor::from_argb(118, 94, 180, 255),
                1 => AppColor::from_argb(104, 120, 210, 255),
                2 => AppColor::from_argb(96, 160, 226, 255),
                _ => AppColor::from_argb(112, 178, 232, 255),
            };
            self.render_cells.push(SandRenderCell {
                x: drop.x.round() as i32,
                y: drop.y.round() as i32,
                width: 1,
                height: drop.length,
                color,
            });
        }
    }

    fn random_range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_unit()
    }

    fn next_unit(&mut self) -> f32 {
        let value = self.next_u32();
        value as f32 / u32::MAX as f32
    }

    fn next_u32(&mut self) -> u32 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.rng_state >> 32) as u32
    }
}

#[derive(Debug)]
struct SandWorld {
    active: bool,
    cell_size: i32,
    width: usize,
    height: usize,
    frame: u64,
    occupied_count: usize,
    settled_frames: u8,
    render_dirty: bool,
    min_x: usize,
    max_x: usize,
    min_y: usize,
    max_y: usize,
    cells: Vec<u8>,
    render_cells: Vec<SandRenderCell>,
}

impl Default for SandWorld {
    fn default() -> Self {
        Self {
            active: false,
            cell_size: SAND_CELL_SIZE_PIXELS,
            width: 0,
            height: 0,
            frame: 0,
            occupied_count: 0,
            settled_frames: 0,
            render_dirty: false,
            min_x: 0,
            max_x: 0,
            min_y: 0,
            max_y: 0,
            cells: Vec::new(),
            render_cells: Vec::new(),
        }
    }
}

impl SandWorld {
    fn toggle(&mut self, bounds: RectF) -> bool {
        self.active = !self.active;
        self.ensure_grid(bounds);
        if !self.active {
            self.render_cells.clear();
        } else {
            self.render_dirty = true;
            self.settled_frames = 0;
        }
        self.active
    }

    fn clear(&mut self) {
        self.cells.fill(0);
        self.render_cells.clear();
        self.frame = 0;
        self.occupied_count = 0;
        self.settled_frames = 0;
        self.render_dirty = false;
        self.reset_bounds();
        self.active = false;
    }

    fn occupied_count(&self) -> usize {
        self.occupied_count
    }

    fn render_cells(&self) -> &[SandRenderCell] {
        &self.render_cells
    }

    fn emit_at(&mut self, cursor: Vector2, bounds: RectF) {
        self.ensure_grid(bounds);
        if self.width == 0 || self.height == 0 {
            return;
        }
        let (center_x, center_y) = self.cursor_cell(cursor);
        let radius = self.pixel_radius_to_cells(SAND_EMIT_RADIUS_PIXELS);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let x = center_x + dx;
                let y = center_y + dy;
                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                    continue;
                }
                let idx = self.index(x as usize, y as usize);
                if self.cells[idx] == 0 {
                    self.cells[idx] = 1 + (((x * 17 + y * 31 + self.frame as i32) & 3) as u8);
                    self.occupied_count += 1;
                    self.include_cell(x as usize, y as usize);
                    self.render_dirty = true;
                    self.settled_frames = 0;
                }
            }
        }
    }

    fn erase_at(&mut self, cursor: Vector2, bounds: RectF) {
        self.ensure_grid(bounds);
        if self.width == 0 || self.height == 0 {
            return;
        }
        let (center_x, center_y) = self.cursor_cell(cursor);
        let radius = self.pixel_radius_to_cells(SAND_ERASE_RADIUS_PIXELS);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let x = center_x + dx;
                let y = center_y + dy;
                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                    continue;
                }
                let idx = self.index(x as usize, y as usize);
                if self.cells[idx] != 0 {
                    self.cells[idx] = 0;
                    self.occupied_count = self.occupied_count.saturating_sub(1);
                    self.render_dirty = true;
                    self.settled_frames = 0;
                }
            }
        }
    }

    fn cursor_cell(&self, cursor: Vector2) -> (i32, i32) {
        (
            (cursor.x / self.cell_size as f32).round() as i32,
            (cursor.y / self.cell_size as f32).round() as i32,
        )
    }

    fn pixel_radius_to_cells(&self, pixels: i32) -> i32 {
        ((pixels.max(1) as f32 / self.cell_size.max(1) as f32).ceil() as i32).max(1)
    }

    fn step(&mut self, bounds: RectF) {
        self.ensure_grid(bounds);
        if self.width == 0 || self.height == 0 {
            return;
        }
        if self.occupied_count == 0 {
            self.render_cells.clear();
            self.render_dirty = false;
            self.settled_frames = 0;
            self.reset_bounds();
            return;
        }
        if self.settled_frames > 8 && !self.render_dirty {
            return;
        }

        let mut moved = false;
        for _ in 0..2 {
            self.frame = self.frame.wrapping_add(1);
            let scan_min_x = self.min_x.saturating_sub(1);
            let scan_max_x = (self.max_x + 1).min(self.width - 1);
            let scan_min_y = self.min_y.saturating_sub(1);
            let scan_max_y = (self.max_y + 2).min(self.height.saturating_sub(2));
            if scan_min_y > scan_max_y || scan_min_x > scan_max_x {
                continue;
            }

            for y in (scan_min_y..=scan_max_y).rev() {
                let left_to_right = ((y as u64 + self.frame) & 1) == 0;
                let scan_width = scan_max_x - scan_min_x + 1;
                for offset in 0..scan_width {
                    let x = if left_to_right {
                        scan_min_x + offset
                    } else {
                        scan_max_x - offset
                    };
                    let idx = self.index(x, y);
                    let grain = self.cells[idx];
                    if grain == 0 {
                        continue;
                    }
                    let below = self.index(x, y + 1);
                    if self.cells[below] == 0 {
                        self.cells[below] = grain;
                        self.cells[idx] = 0;
                        self.include_cell(x, y + 1);
                        moved = true;
                        continue;
                    }

                    let prefer_left = ((x as u64 * 13 + y as u64 * 7 + self.frame) & 1) == 0;
                    let first = if prefer_left { -1 } else { 1 };
                    let second = -first;
                    if self.try_slide(idx, x, y, first, grain) || self.try_slide(idx, x, y, second, grain) {
                        moved = true;
                        continue;
                    }
                }
            }
        }

        if moved {
            self.settled_frames = 0;
            self.render_dirty = true;
        } else {
            self.settled_frames = self.settled_frames.saturating_add(1);
        }

        if self.render_dirty {
            self.rebuild_render_cells();
        }
    }

    fn try_slide(&mut self, from: usize, x: usize, y: usize, dx: i32, grain: u8) -> bool {
        let next_x = x as i32 + dx;
        if next_x < 0 || next_x >= self.width as i32 {
            return false;
        }
        let to = self.index(next_x as usize, y + 1);
        if self.cells[to] != 0 {
            return false;
        }
        self.cells[to] = grain;
        self.cells[from] = 0;
        self.include_cell(next_x as usize, y + 1);
        true
    }

    fn ensure_grid(&mut self, bounds: RectF) {
        let width = ((bounds.width.max(1.0) / self.cell_size as f32).ceil() as usize).max(1);
        let height = ((bounds.height.max(1.0) / self.cell_size as f32).ceil() as usize).max(1);
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        self.cells = vec![0; width * height];
        self.render_cells.clear();
        self.occupied_count = 0;
        self.settled_frames = 0;
        self.render_dirty = false;
        self.reset_bounds();
    }

    fn rebuild_render_cells(&mut self) {
        self.render_cells.clear();
        if self.occupied_count == 0 {
            self.render_dirty = false;
            self.reset_bounds();
            return;
        }

        let mut found_count = 0usize;
        let mut found_any = false;
        let mut next_min_x = usize::MAX;
        let mut next_min_y = usize::MAX;
        let mut next_max_x = 0usize;
        let mut next_max_y = 0usize;
        let scan_min_x = self.min_x.min(self.width - 1);
        let scan_max_x = self.max_x.min(self.width - 1);
        let scan_min_y = self.min_y.min(self.height - 1);
        let scan_max_y = self.max_y.min(self.height - 1);
        self.render_cells
            .reserve((scan_max_y.saturating_sub(scan_min_y) + 1).max(64));

        for y in scan_min_y..=scan_max_y {
            let mut x = scan_min_x;
            while x <= scan_max_x {
                let grain = self.cells[self.index(x, y)];
                if grain == 0 {
                    x += 1;
                    continue;
                }

                let start_x = x;
                let mut run_len = 0usize;
                while x <= scan_max_x && self.cells[self.index(x, y)] != 0 {
                    run_len += 1;
                    x += 1;
                }
                let end_x = x - 1;
                found_count += run_len;
                found_any = true;
                next_min_x = next_min_x.min(start_x);
                next_max_x = next_max_x.max(end_x);
                next_min_y = next_min_y.min(y);
                next_max_y = next_max_y.max(y);

                self.render_cells.push(SandRenderCell {
                    x: start_x as i32 * self.cell_size,
                    y: y as i32 * self.cell_size,
                    width: run_len as i32 * self.cell_size,
                    height: self.cell_size,
                    color: sand_span_color(start_x, y, run_len, grain),
                });
            }
        }

        if found_any {
            self.min_x = next_min_x;
            self.max_x = next_max_x;
            self.min_y = next_min_y;
            self.max_y = next_max_y;
            self.occupied_count = found_count;
        } else {
            self.occupied_count = 0;
            self.reset_bounds();
        }
        self.render_dirty = false;
    }

    fn index(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    fn include_cell(&mut self, x: usize, y: usize) {
        if self.occupied_count <= 1 && self.render_cells.is_empty() {
            self.min_x = x;
            self.max_x = x;
            self.min_y = y;
            self.max_y = y;
            return;
        }
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_y = self.min_y.min(y);
        self.max_y = self.max_y.max(y);
    }

    fn reset_bounds(&mut self) {
        self.min_x = 0;
        self.max_x = 0;
        self.min_y = 0;
        self.max_y = 0;
    }
}

fn sand_span_color(start_x: usize, y: usize, len: usize, grain: u8) -> AppColor {
    let shade = 1 + (((start_x as u64 * 17 + y as u64 * 31 + len as u64 * 7 + grain as u64) & 3) as u8);
    sand_color(shade)
}

fn sand_color(grain: u8) -> AppColor {
    match grain {
        1 => AppColor::from_rgb(236, 197, 98),
        2 => AppColor::from_rgb(224, 176, 76),
        3 => AppColor::from_rgb(247, 215, 126),
        _ => AppColor::from_rgb(201, 150, 64),
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
