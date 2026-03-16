# ScreenOverlayPhysics

Transparent WPF desktop overlay with 2D rigid-body-style motion and 3D-rendered objects.

## Requirements

- Windows
- .NET 9 SDK

## Run

```powershell
dotnet run --project .\ScreenOverlayPhysics.csproj
```

## Controls

- `F1`: toggle debug overlay
- `F2`: spawn another cube
- `F3`: reset scene
- `F4`: open runtime settings
- `F5`: force interactive overlay mode
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
