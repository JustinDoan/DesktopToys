use std::{borrow::Cow, error::Error, sync::Arc};

use anyhow::{Context, Result};
use core_types::{AppColor, AppConfig, RectF, Vector2};
use native_shell::{
    configure_overlay_window, overlay_window_attributes, pick_model_file, set_overlay_input_mode, show_error_dialog,
    sync_window_to_monitor,
    GlobalInputPoller, OverlayInputMode, TrayAction, TrayController,
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

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = NativeApp::default();
    event_loop.run_app(&mut app).map_err(Into::into)
}

struct NativeApp {
    window: Option<Arc<Window>>,
    window_id: Option<WindowId>,
    gpu: Option<GpuState>,
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
    force_interactive_for_debug: bool,
    is_rotation_dragging: bool,
    last_drag_attempt: String,
    last_rotation_cursor: Vector2,
    hud: HudState,
    status_message: Option<String>,
    next_input_retry_seconds: f64,
    settings_panel: SettingsPanel,
    import_panel: Option<ImportPanel>,
    fallback_left_down: bool,
    fallback_right_down: bool,
}

const FLOOR_MARGIN_PIXELS: f32 = 18.0;

impl Default for NativeApp {
    fn default() -> Self {
        let config = AppConfig::default();
        Self {
            window: None,
            window_id: None,
            gpu: None,
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
            force_interactive_for_debug: false,
            is_rotation_dragging: false,
            last_drag_attempt: "none".to_string(),
            last_rotation_cursor: Vector2::ZERO,
            hud: HudState::default(),
            status_message: None,
            next_input_retry_seconds: 0.0,
            settings_panel: SettingsPanel::default(),
            import_panel: None,
            fallback_left_down: false,
            fallback_right_down: false,
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

        let window = Arc::new(
            event_loop
                .create_window(overlay_window_attributes("ScreenOverlayPhysics Native", self.bounds))
                .context("Failed to create native overlay window")?,
        );
        configure_overlay_window(&window)?;
        self.bounds = sync_window_to_monitor(&window);
        self.overlay_mode = if self.scene.config().start_in_pass_through {
            OverlayInputMode::PassThrough
        } else {
            OverlayInputMode::Interactive
        };
        self.pending_mode = self.overlay_mode;
        let _ = set_overlay_input_mode(&window, self.overlay_mode);

        let size = window.inner_size();
        let gpu = pollster::block_on(GpuState::new(window.clone(), size.width, size.height))
            .context("Failed to create GPU renderer")?;

        self.window_id = Some(window.id());
        self.window = Some(window);
        self.gpu = Some(gpu);
        self.scene.initialize(self.scene_bounds());
        self.selected_id = self.scene.objects().last().map(|object| object.id);
        self.settings_panel = SettingsPanel::from_config(*self.scene.config());
        self.tray = TrayController::new().ok();
        if self.tray.is_none() {
            self.status_message = Some("Tray icon unavailable on this host.".to_string());
        }
        if let Some(gpu) = &self.gpu {
            self.push_status_message(format!("Renderer backend: {}", gpu.backend_label));
        }
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
                    }
                },
            }
        } else {
            native_shell::GlobalPointerState {
                screen_position: (0, 0),
                local_position: self.cursor_local,
                left_down: self.fallback_left_down,
                right_down: self.fallback_right_down,
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
            self.update_rotation_drag(pointer.right_down);
        }

        self.handle_global_mouse_buttons(now, pointer.left_down, pointer.right_down);
        self.scene.step(dt, self.scene_bounds());
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
            if set_overlay_input_mode(window, desired_mode).is_ok() {
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

        if is_left_down && !self.was_left_down {
            let began = self.drag_controller.begin_drag(
                self.scene.objects_mut(),
                self.cursor_local,
                now_seconds,
                &self.hit_tester,
            );
            self.last_drag_attempt = if let Some(id) = began {
                self.selected_id = Some(id);
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

    fn end_drag_and_apply_spin(&mut self, now_seconds: f64) {
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

        object.angular_velocity_y += throw_velocity.x as f64 * 0.22;
        object.angular_velocity_x += throw_velocity.y as f64 * 0.16;
        object.angular_velocity_z += throw_velocity.x as f64 * 0.08;
        self.is_rotation_dragging = false;
    }

    fn update_rotation_drag(&mut self, is_right_down: bool) {
        let Some(selected_id) = self.selected_id else {
            self.is_rotation_dragging = false;
            return;
        };

        let Some(object) = self.scene.objects_mut().iter_mut().find(|object| object.id == selected_id) else {
            self.is_rotation_dragging = false;
            return;
        };

        if self.drag_controller.dragged_id() != Some(selected_id) || !is_right_down {
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
        let rotation_sensitivity = 0.72f64;
        object.rotation_y += delta.x as f64 * rotation_sensitivity;
        object.rotation_x += delta.y as f64 * rotation_sensitivity;
        object.angular_velocity_y = delta.x as f64 * rotation_sensitivity * 40.0;
        object.angular_velocity_x = delta.y as f64 * rotation_sensitivity * 40.0;
        object.angular_velocity_z = delta.x as f64 * rotation_sensitivity * 8.0;
        self.last_drag_attempt = format!("rotate:{:.1},{:.1}", delta.x, delta.y);
    }

    fn sync_panels(&mut self) {
        let mut panels = Vec::new();

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
        let Some(gpu) = &mut self.gpu else {
            return Ok(());
        };

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
        gpu.render(&vertices)
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
            AppAction::Reset => {
                self.scene.reset(self.scene_bounds());
                self.selected_id = self.scene.objects().last().map(|object| object.id);
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

        if let Some(import_panel) = &mut self.import_panel {
            if import_panel.handle_key(key_code) {
                return;
            }
            if key_code == KeyCode::Enter {
                let default_spawn = self.default_spawn_position();
                let panel = self.import_panel.take().unwrap();
                let id = self.scene.spawn_imported_model(
                    default_spawn,
                    panel.path.clone(),
                    panel.scale_multiplier,
                    AppColor::from_rgb(panel.tint_r, panel.tint_g, panel.tint_b),
                );
                self.selected_id = Some(id);
                self.status_message = Some(format!("Imported model: {}", panel.path));
                return;
            }
            if key_code == KeyCode::Escape {
                self.import_panel = None;
                return;
            }
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
            KeyCode::Escape => self.handle_action(AppAction::Exit, event_loop),
            _ => {},
        }
    }

    fn default_spawn_position(&self) -> Vector2 {
        let bounds = self.scene_bounds();
        Vector2::new(bounds.width * 0.5, 40.0)
    }
}

struct GpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    backend_label: String,
}

impl GpuState {
    async fn new(window: Arc<Window>, width: u32, height: u32) -> Result<Self> {
        let backends = if cfg!(target_os = "macos") {
            wgpu::Backends::METAL
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
        let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            wgpu::CompositeAlphaMode::PreMultiplied
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

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(input.position, 1.0);
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

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("screen-overlay-physics-pipeline-layout"),
            bind_group_layouts: &[],
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
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
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
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
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

fn gpu_vertices_as_bytes(vertices: &[GpuVertex]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            vertices.as_ptr() as *const u8,
            std::mem::size_of_val(vertices),
        )
    }
}

impl ApplicationHandler for NativeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
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
        if let Some(tray) = &self.tray {
            if let Some(action) = tray.poll_action() {
                let mapped = match action {
                    TrayAction::ToggleDebug => AppAction::ToggleDebug,
                    TrayAction::SpawnObject => AppAction::SpawnObject,
                    TrayAction::SpawnCrystal => AppAction::SpawnCrystal,
                    TrayAction::Reset => AppAction::Reset,
                    TrayAction::ToggleSettings => AppAction::ToggleSettings,
                    TrayAction::ImportModel => AppAction::RequestImport,
                    TrayAction::Exit => AppAction::Exit,
                };
                self.handle_action(mapped, event_loop);
            }
        }

        self.update();
    }
}

#[derive(Clone, Copy)]
enum AppAction {
    ToggleDebug,
    SpawnObject,
    SpawnCrystal,
    Reset,
    ToggleSettings,
    ToggleForceInteractive,
    RequestImport,
    Exit,
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
