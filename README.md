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
- `Esc`: quit

## Project map

- `native/app/src/main.rs`: native app shell, hotkeys, tray actions, HUD/game state
- `native/scene_logic/src/lib.rs`: object spawning, drag logic, scene reset flow
- `native/physics_core/src/lib.rs`: physics world integration and Box3D bridge
- `native/renderer/src/lib.rs`: mesh generation and overlay rendering
- `native/native_shell/src/lib.rs`: platform window/input/tray helpers
- `native/core_types/src/lib.rs`: shared object, body, color, and config types
- `Assets/`: runtime and compile-time visual assets used by the native crates
