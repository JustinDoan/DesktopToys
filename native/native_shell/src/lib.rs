use std::path::PathBuf;

use anyhow::Result;
use core_types::{RectF, Vector2};
#[cfg(target_os = "macos")]
use core_foundation::{
    base::{Boolean, TCFType},
    boolean::CFBoolean,
    dictionary::{CFDictionary, CFDictionaryRef},
    string::{CFString, CFStringRef},
};
#[cfg(not(target_os = "macos"))]
use device_query::{DeviceState, Keycode};
#[cfg(not(target_os = "macos"))]
use device_query::DeviceQuery;
#[cfg(not(target_os = "macos"))]
use std::panic::AssertUnwindSafe;
#[cfg(target_os = "macos")]
use objc2_app_kit::NSScreen;
#[cfg(target_os = "macos")]
use objc2_foundation::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2::{rc::Retained, ClassType};
#[cfg(target_os = "macos")]
use objc2_app_kit::NSView;
#[cfg(target_os = "macos")]
use readmouse::Mouse;
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageLevel};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize, Position, Size},
    error::ExternalError,
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::{Window, WindowAttributes, WindowLevel},
};
#[cfg(target_os = "macos")]
use winit::monitor::MonitorHandle;

#[cfg(target_os = "linux")]
use winit::platform::x11::WindowAttributesExtX11;
#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOMOVE,
        SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TRANSPARENT,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayInputMode {
    PassThrough,
    Interactive,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GlobalImportKeys {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub enter: bool,
    pub escape: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GlobalPointerState {
    pub screen_position: (i32, i32),
    pub local_position: Vector2,
    pub left_down: bool,
    pub right_down: bool,
    pub spawn_object_down: bool,
    pub spawn_crystal_down: bool,
    pub reset_down: bool,
    pub weather_toggle_down: bool,
    pub sand_toggle_down: bool,
    pub spawn_stress_down: bool,
    pub slingshot_toggle_down: bool,
    pub robot_buddy_down: bool,
    pub basketball_toggle_down: bool,
    pub import_keys: GlobalImportKeys,
}

pub fn overlay_window_attributes(title: &str, bounds: RectF) -> WindowAttributes {
    let attributes = Window::default_attributes()
        .with_title(title)
        .with_decorations(false)
        .with_resizable(false)
        .with_position(Position::Physical(PhysicalPosition::new(bounds.x as i32, bounds.y as i32)))
        .with_inner_size(Size::Physical(PhysicalSize::new(
            bounds.width.max(1.0) as u32,
            bounds.height.max(1.0) as u32,
        )))
        .with_window_level(WindowLevel::AlwaysOnTop);

    #[cfg(not(target_os = "windows"))]
    let attributes = attributes.with_transparent(true);

    #[cfg(target_os = "linux")]
    let attributes = attributes.with_override_redirect(true);

    attributes
}

pub fn sync_window_to_monitor(window: &Window) -> RectF {
    let Some(monitor) = window.current_monitor() else {
        let size = window.inner_size();
        return RectF::new(0.0, 0.0, size.width as f32, size.height as f32);
    };

    #[cfg(target_os = "macos")]
    let bounds = macos_visible_monitor_bounds(&monitor).unwrap_or_else(|| {
        let position = monitor.position();
        let size = monitor.size();
        RectF::new(
            position.x as f32,
            position.y as f32,
            size.width as f32,
            size.height as f32,
        )
    });

    #[cfg(not(target_os = "macos"))]
    let bounds = {
        let position = monitor.position();
        let size = monitor.size();
        RectF::new(
            position.x as f32,
            position.y as f32,
            size.width as f32,
            size.height as f32,
        )
    };

    let position = PhysicalPosition::new(bounds.x.round() as i32, bounds.y.round() as i32);
    let size = PhysicalSize::new(bounds.width.max(1.0).round() as u32, bounds.height.max(1.0).round() as u32);
    window.set_outer_position(position);
    let _ = window.request_inner_size(Size::Physical(size));
    window.set_window_level(WindowLevel::AlwaysOnTop);

    bounds
}

pub fn set_overlay_input_mode(window: &Window, mode: OverlayInputMode) -> Result<(), ExternalError> {
    let interactive = mode == OverlayInputMode::Interactive;
    window.set_cursor_hittest(interactive)
}

#[cfg(target_os = "macos")]
pub fn configure_overlay_window(window: &Window) -> Result<()> {
    let handle = window
        .window_handle()
        .map_err(|error| anyhow::anyhow!("Failed to access native window handle: {error}"))?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return Ok(());
    };

    let ns_view = unsafe { Retained::retain(appkit.ns_view.as_ptr().cast::<NSView>()) }
        .ok_or_else(|| anyhow::anyhow!("AppKit view handle was null"))?;
    let ns_window = ns_view
        .window()
        .ok_or_else(|| anyhow::anyhow!("AppKit view was not attached to an NSWindow"))?;
    ns_window.setHasShadow(false);
    ns_window.setOpaque(false);
    Ok(())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub fn configure_overlay_window(_window: &Window) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn configure_overlay_window(window: &Window) -> Result<()> {
    let handle = window
        .window_handle()
        .map_err(|error| anyhow::anyhow!("Failed to access native window handle: {error}"))?;
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return Ok(());
    };

    let hwnd = HWND(win32.hwnd.get() as isize);
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next_ex_style = (ex_style
            | WS_EX_LAYERED.0 as isize
            | WS_EX_NOACTIVATE.0 as isize
            | WS_EX_TRANSPARENT.0 as isize
            | WS_EX_TOOLWINDOW.0 as isize)
            & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next_ex_style);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
        )?;
    }

    Ok(())
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
#[derive(Debug, Default)]
pub struct GlobalInputPoller {
    #[cfg(not(target_os = "macos"))]
    device_state: DeviceState,
}

impl GlobalInputPoller {
    pub fn try_new() -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            if !macos_accessibility_is_trusted() {
                anyhow::bail!(
                    "Accessibility permission is required for global drag input. Enable it in System Settings > Privacy & Security > Accessibility."
                );
            }
            return Ok(Self {});
        }

        #[cfg(not(target_os = "macos"))]
        {
            let device_state = std::panic::catch_unwind(AssertUnwindSafe(DeviceState::new))
                .map_err(|_| anyhow::anyhow!("Global input is unavailable on this host."))?;
            std::panic::catch_unwind(AssertUnwindSafe(|| device_state.get_mouse()))
                .map_err(|_| anyhow::anyhow!("Global mouse access requires OS accessibility permissions."))?;
            Ok(Self { device_state })
        }
    }

    pub fn try_new_with_prompt() -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            if !macos_accessibility_is_trusted_with_prompt() {
                anyhow::bail!(
                    "Accessibility permission is required for global drag input. macOS should have opened the permission prompt."
                );
            }
            return Ok(Self {});
        }

        #[cfg(not(target_os = "macos"))]
        {
            Self::try_new()
        }
    }

    pub fn poll(&self, bounds: RectF) -> Result<GlobalPointerState> {
        #[cfg(target_os = "macos")]
        {
            if !macos_accessibility_is_trusted() {
                anyhow::bail!(
                    "Accessibility permission is no longer available for global drag input."
                );
            }
            let raw_x = Mouse::location().0 as f32;
            let raw_y = Mouse::location().1 as f32;
            let cursor = macos_global_pointer_state(bounds, raw_x, raw_y);

            return Ok(GlobalPointerState {
                screen_position: (cursor.0.round() as i32, cursor.1.round() as i32),
                local_position: Vector2::new(cursor.2, cursor.3),
                left_down: Mouse::Left.is_pressed(),
                right_down: Mouse::Right.is_pressed(),
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
            });
        }

        #[cfg(not(target_os = "macos"))]
        {
            let mouse = std::panic::catch_unwind(AssertUnwindSafe(|| self.device_state.get_mouse()))
                .map_err(|_| anyhow::anyhow!("Global mouse access requires OS accessibility permissions."))?;
            let keys = std::panic::catch_unwind(AssertUnwindSafe(|| self.device_state.get_keys()))
                .map_err(|_| anyhow::anyhow!("Global keyboard access requires OS accessibility permissions."))?;
            let local_x = (mouse.coords.0 as f32 - bounds.x).clamp(0.0, bounds.width.max(1.0));
            let local_y = (mouse.coords.1 as f32 - bounds.y).clamp(0.0, bounds.height.max(1.0));

            Ok(GlobalPointerState {
                screen_position: mouse.coords,
                local_position: Vector2::new(local_x, local_y),
                left_down: *mouse.button_pressed.get(1).unwrap_or(&false),
                right_down: *mouse.button_pressed.get(2).unwrap_or(&false),
                spawn_object_down: keys.contains(&Keycode::F2),
                spawn_crystal_down: keys.contains(&Keycode::F7),
                reset_down: keys.contains(&Keycode::F3),
                weather_toggle_down: keys.contains(&Keycode::F5),
                sand_toggle_down: keys.contains(&Keycode::F6),
                spawn_stress_down: keys.contains(&Keycode::F9),
                slingshot_toggle_down: keys.contains(&Keycode::F10),
                robot_buddy_down: keys.contains(&Keycode::F11),
                basketball_toggle_down: keys.contains(&Keycode::F12),
                import_keys: GlobalImportKeys {
                    up: keys.contains(&Keycode::Up),
                    down: keys.contains(&Keycode::Down),
                    left: keys.contains(&Keycode::Left),
                    right: keys.contains(&Keycode::Right),
                    enter: keys.contains(&Keycode::Enter),
                    escape: keys.contains(&Keycode::Escape),
                },
            })
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_accessibility_is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

#[cfg(target_os = "macos")]
fn macos_accessibility_is_trusted_with_prompt() -> bool {
    unsafe {
        let option_prompt = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options: CFDictionary<CFString, CFBoolean> =
            CFDictionary::from_CFType_pairs(&[(option_prompt, CFBoolean::true_value())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) != 0
    }
}

#[cfg(target_os = "macos")]
fn macos_visible_monitor_bounds(monitor: &MonitorHandle) -> Option<RectF> {
    let selected = macos_screen_for_monitor(monitor)?;
    let frame = unsafe { selected.convertRectToBacking(selected.frame()) };
    let visible = unsafe { selected.convertRectToBacking(selected.visibleFrame()) };
    let x_offset = visible.min().x - frame.min().x;
    let top_offset = frame.max().y - visible.max().y;
    let position = monitor.position();

    Some(RectF::new(
        position.x as f32 + x_offset as f32,
        position.y as f32 + top_offset as f32,
        visible.size.width as f32,
        visible.size.height as f32,
    ))
}

#[cfg(target_os = "macos")]
fn macos_global_pointer_state(bounds: RectF, raw_x_points: f32, raw_y_points: f32) -> (f32, f32, f32, f32) {
    let mtm = match MainThreadMarker::new() {
        Some(mtm) => mtm,
        None => {
            let local_x = (raw_x_points - bounds.x).clamp(0.0, bounds.width.max(1.0));
            let local_y = (raw_y_points - bounds.y).clamp(0.0, bounds.height.max(1.0));
            return (raw_x_points, raw_y_points, local_x, local_y);
        },
    };

    let mut selected_visible = None;
    for screen in NSScreen::screens(mtm).iter() {
        let visible = screen.visibleFrame();
        let min_x = visible.min().x as f32;
        let max_x = visible.max().x as f32;
        let min_y = visible.min().y as f32;
        let max_y = visible.max().y as f32;
        if raw_x_points >= min_x
            && raw_x_points <= max_x
            && raw_y_points >= min_y
            && raw_y_points <= max_y
        {
            selected_visible = Some((visible, screen.backingScaleFactor() as f32));
            break;
        }
    }

    let (visible, backing_scale) = selected_visible.unwrap_or_else(|| {
        let fallback = NSScreen::mainScreen(mtm).expect("main screen should exist");
        (fallback.visibleFrame(), fallback.backingScaleFactor() as f32)
    });
    let screen_x = raw_x_points * backing_scale;
    let screen_y = raw_y_points * backing_scale;
    let _ = visible;
    let local_x = (screen_x - bounds.x).clamp(0.0, bounds.width.max(1.0));
    let local_y = (screen_y - bounds.y).clamp(0.0, bounds.height.max(1.0));
    (screen_x, screen_y, local_x, local_y)
}

#[cfg(target_os = "macos")]
fn macos_screen_for_monitor(monitor: &MonitorHandle) -> Option<objc2::rc::Retained<NSScreen>> {
    let mtm = MainThreadMarker::new()?;
    let expected_size = monitor.size();
    let fallback = NSScreen::mainScreen(mtm);
    let screens = NSScreen::screens(mtm);
    screens
        .iter()
        .find(|screen| {
            let frame = unsafe { screen.convertRectToBacking(screen.frame()) };
            let width = frame.size.width.round() as u32;
            let height = frame.size.height.round() as u32;
            width == expected_size.width && height == expected_size.height
        })
        .map(|screen| screen.retain())
        .or(fallback)
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrusted() -> Boolean;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    ToggleDebug,
    SpawnObject,
    SpawnCrystal,
    SpawnDvdLogo,
    SpawnStressCubes,
    Reset,
    ToggleSettings,
    ToggleWeather,
    ToggleSand,
    ToggleMeasureTool,
    ImportModel,
    Exit,
}

pub struct TrayController {
    _tray_icon: TrayIcon,
    toggle_debug: MenuItem,
    spawn_object: MenuItem,
    spawn_crystal: MenuItem,
    spawn_dvd_logo: MenuItem,
    spawn_stress_cubes: MenuItem,
    reset: MenuItem,
    toggle_settings: MenuItem,
    toggle_weather: MenuItem,
    toggle_sand: MenuItem,
    toggle_measure_tool: MenuItem,
    import_model: MenuItem,
    exit: MenuItem,
}

impl TrayController {
    pub fn new() -> Result<Self> {
        let menu = Menu::new();
        let toggle_debug = MenuItem::new("Toggle Debug (F1)", true, None);
        let spawn_object = MenuItem::new("Spawn Object (F2)", true, None);
        let spawn_crystal = MenuItem::new("Spawn Crystal (F7)", true, None);
        let spawn_dvd_logo = MenuItem::new("Spawn DVD Logo (F8)", true, None);
        let spawn_stress_cubes = MenuItem::new("Spawn Stress Cubes (F9)", true, None);
        let reset = MenuItem::new("Reset (F3)", true, None);
        let toggle_settings = MenuItem::new("Settings (F4)", true, None);
        let toggle_weather = MenuItem::new("Toggle Rain (F5)", true, None);
        let toggle_sand = MenuItem::new("Toggle Sand (F6)", true, None);
        let toggle_measure_tool = MenuItem::new("Measure Tool", true, None);
        let import_model = MenuItem::new("Import Model", true, None);
        let exit = MenuItem::new("Exit", true, None);

        menu.append_items(&[
            &toggle_debug,
            &spawn_object,
            &spawn_crystal,
            &spawn_dvd_logo,
            &spawn_stress_cubes,
            &reset,
            &toggle_settings,
            &toggle_weather,
            &toggle_sand,
            &toggle_measure_tool,
            &import_model,
            &PredefinedMenuItem::separator(),
            &exit,
        ])?;

        let tray_icon = TrayIconBuilder::new()
            .with_tooltip("ScreenOverlayPhysics Native")
            .with_icon(make_app_icon()?)
            .with_menu(Box::new(menu))
            .build()?;

        Ok(Self {
            _tray_icon: tray_icon,
            toggle_debug,
            spawn_object,
            spawn_crystal,
            spawn_dvd_logo,
            spawn_stress_cubes,
            reset,
            toggle_settings,
            toggle_weather,
            toggle_sand,
            toggle_measure_tool,
            import_model,
            exit,
        })
    }

    pub fn poll_action(&self) -> Option<TrayAction> {
        let Ok(event) = MenuEvent::receiver().try_recv() else {
            return None;
        };

        if event.id == self.toggle_debug.id() {
            Some(TrayAction::ToggleDebug)
        } else if event.id == self.spawn_object.id() {
            Some(TrayAction::SpawnObject)
        } else if event.id == self.spawn_crystal.id() {
            Some(TrayAction::SpawnCrystal)
        } else if event.id == self.spawn_dvd_logo.id() {
            Some(TrayAction::SpawnDvdLogo)
        } else if event.id == self.spawn_stress_cubes.id() {
            Some(TrayAction::SpawnStressCubes)
        } else if event.id == self.reset.id() {
            Some(TrayAction::Reset)
        } else if event.id == self.toggle_settings.id() {
            Some(TrayAction::ToggleSettings)
        } else if event.id == self.toggle_weather.id() {
            Some(TrayAction::ToggleWeather)
        } else if event.id == self.toggle_sand.id() {
            Some(TrayAction::ToggleSand)
        } else if event.id == self.toggle_measure_tool.id() {
            Some(TrayAction::ToggleMeasureTool)
        } else if event.id == self.import_model.id() {
            Some(TrayAction::ImportModel)
        } else if event.id == self.exit.id() {
            Some(TrayAction::Exit)
        } else {
            None
        }
    }
}

pub fn pick_model_file() -> Option<PathBuf> {
    FileDialog::new()
        .set_title("Import 3D Model")
        .add_filter("3D Models", &["obj", "stl"])
        .pick_file()
}

pub fn show_error_dialog(title: &str, message: &str) {
    let _ = MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title(title)
        .set_description(message)
        .set_buttons(MessageButtons::Ok)
        .show();
}

fn make_app_icon() -> Result<Icon> {
    let mut rgba = vec![0u8; 16 * 16 * 4];
    for y in 0..16usize {
        for x in 0..16usize {
            let idx = (y * 16 + x) * 4;
            let border = x == 0 || y == 0 || x == 15 || y == 15;
            let highlight = (x + y) % 5 == 0;
            let (r, g, b, a) = if border {
                (180, 240, 255, 255)
            } else if highlight {
                (96, 170, 255, 255)
            } else {
                (28, 58, 118, 240)
            };

            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = a;
        }
    }

    Ok(Icon::from_rgba(rgba, 16, 16)?)
}
