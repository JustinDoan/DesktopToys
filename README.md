# ScreenOverlayPhysics Native

Transparent native Rust desktop overlay with physics-driven screen toys, Direct3D/DirectComposition on Windows, and a WGPU fallback on other platforms.

## Requirements

- Windows
- Rust toolchain

## Run

```powershell
cd native
cargo run -p app
```

## Overlay UI

The overlay UI lives in `control-ui/`. It is a Tauri 2 + Preact/Vite transparent always-on-top webview intended for HUD/widgets that live above the desktop like the 3D objects. The native renderer still owns physics, input passthrough, and 3D rendering. The control UI talks to the renderer over local IPC for spawn variants, runtime settings, pause/reset, tools, and tray-launched window wakeup.

```powershell
cd control-ui
npm install
npm run dev
```

To run the Tauri shell:

```powershell
cd control-ui
npm run tauri:dev
```

## Packaging

The release build packages the native renderer/tray app as a Tauri sidecar. The installed Tauri app launches the renderer, and the renderer tray can bring the control UI window forward.

Build the Windows installer:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1
```

The NSIS installer is written under `control-ui/src-tauri/target/release/bundle/nsis/`.

For one portable runtime `.exe`, the renderer and Tauri shell still need to be merged into one process or one binary must embed/extract the other at startup.

## Controls

- `F1`: toggle debug overlay
- `F2`: spawn the next catalog object
- `F3`: reset scene
- `F4`: open runtime settings
- `F5`: force interactive overlay mode
- `F6`: import a 3D model
- `F7`: spawn a crystal
- `F8`: spawn a DVD logo
- `F9`: spawn a batch of boxes
- `F10`: toggle slingshot game
- `F11`: spawn robot buddy
- `F12`: toggle basketball
- `B`: equip the shatter gun; click to shoot textured screen shards
- `Esc`: quit

## Window terrariums (prototype)

On Windows, hold `Shift` while dragging a physics object over the client area of another application. A cyan guide marks the window that will capture it. Release the object while still holding `Shift` to bind it to that window. Ordinary dragging never captures a window object:

- the object bounces inside the window's client bounds;
- moving the window carries the object with it;
- resizing the window pushes the object back inside;
- minimizing or hiding the window hides and pauses its objects until it returns;
- closing the window releases its objects back to the desktop; and
- dragging the object onto the desktop releases it, while dropping it over another window transfers it.

This first prototype constrains objects to real window geometry but does not yet clip rendering behind overlapping windows or title bars.

## Project map

- `native/app/src/main.rs`: native app shell, hotkeys, tray actions, HUD/game state
- `native/scene_logic/src/lib.rs`: object spawning, drag logic, scene reset flow
- `native/physics_core/src/lib.rs`: physics world integration and Box3D bridge
- `native/renderer/src/lib.rs`: mesh generation and overlay rendering
- `native/native_shell/src/lib.rs`: platform window/input/tray helpers
- `native/core_types/src/lib.rs`: shared object, body, color, and config types
- `Assets/`: runtime and compile-time visual assets used by the native crates
