# ScreenOverlayPhysics

Transparent WPF desktop overlay with 2D rigid-body-style motion and 3D-rendered objects.

## Requirements

- Windows
- .NET 9 SDK

## Run

```powershell
dotnet run --project .\ScreenOverlayPhysics.csproj
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
- `F2`: spawn another cube
- `F3`: reset scene
- `F4`: open runtime settings
- `F5`: force interactive overlay mode
<<<<<<< Updated upstream
=======
- `F6`: import a 3D model
- `F7`: spawn a crystal
- `F8`: spawn a DVD logo
- `F9`: spawn a batch of boxes
- `F10`: toggle slingshot game
- `F11`: spawn robot buddy
- `F12`: toggle basketball
- `B`: equip the shatter gun; click to shoot textured screen shards
>>>>>>> Stashed changes
- `Esc`: quit

## Project map

- `MainWindow.xaml.cs`: overlay/input loop, hotkeys, settings/tray integration
- `Scene/SceneController.cs`: scene bootstrap, object spawning, reset flow, runtime physics config sync
- `Rendering/SceneRenderer.cs`: camera, visual creation, per-frame visual updates, 3D hit testing
- `Rendering/CubeVisual3D.cs`: colored cube visual
- `Rendering/DiceVisual3D.cs`: dice visual with pip-marked faces
- `Physics/PhysicsWorld.cs`: frame integration pipeline
- `Physics/CollisionSolver.cs`: screen-bound and object-object collision resolution
- `Models/ObjectState.cs`: per-object render/physics metadata

## Common modifications

### Add or remove startup objects

Edit `InitialScene` in `Scene/SceneController.cs`.

- `ObjectVisualKind.Cube` creates a standard colored cube
- `ObjectVisualKind.Dice` creates the dice visual
- `Color` is optional for cubes and explicit for special cases like the dice

### Change spawn behavior

Edit:

- `SpawnObject()` in `Scene/SceneController.cs` for default size/mass/restitution/damping
- `CubePalette` in `Scene/SceneController.cs` for the `F2` spawn color cycle

### Add a new visual type

1. Add a new `ObjectVisualKind` value in `Models/ObjectState.cs`
2. Implement `ISceneObjectVisual3D` or derive from `SceneObjectVisual3DBase` in `Rendering/`
3. Register the visual in `CreateVisual()` in `Rendering/SceneRenderer.cs`
4. Seed or spawn it from `Scene/SceneController.cs`

### Change collision behavior

Edit:

- `PhysicsWorld.Step()` to change solver order or integration flow
- `CollisionSolver.ResolveObjectPair()` to change cube-cube response
- `CollisionSolver.SolveScreenBounds()` to change wall/floor behavior
