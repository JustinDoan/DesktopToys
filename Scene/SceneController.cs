using System.Collections.Generic;
using System.Windows.Media;
using ScreenOverlayPhysics.Models;
using ScreenOverlayPhysics.Physics;
using ScreenOverlayPhysics.Rendering;

namespace ScreenOverlayPhysics.Scene;

public sealed class SceneController
{
    private readonly record struct InitialObjectSpec(float XFactor, float YOffset, ObjectVisualKind VisualKind, Color? Color = null);

    private static readonly Color[] CubePalette =
    [
        Color.FromRgb(127, 202, 255),
        Color.FromRgb(255, 143, 163),
        Color.FromRgb(255, 192, 104),
        Color.FromRgb(145, 224, 154),
        Color.FromRgb(183, 153, 255)
    ];

    private static readonly InitialObjectSpec[] InitialScene =
    [
        new(0.00f, 0f, ObjectVisualKind.Cube),
        new(1.00f, 14f, ObjectVisualKind.Cube),
        new(2.00f, 28f, ObjectVisualKind.Cube),
        new(3.00f, 42f, ObjectVisualKind.Cube),
        new(1.80f, -126f, ObjectVisualKind.Dice, Color.FromRgb(245, 245, 240))
    ];

    private readonly AppConfig _config;
    private readonly PhysicsWorld _physicsWorld;
    private readonly SceneRenderer _sceneRenderer;
    private int _nextCubeColorIndex;

    public SceneController(AppConfig config, PhysicsWorld physicsWorld, SceneRenderer sceneRenderer)
    {
        _config = config;
        _physicsWorld = physicsWorld;
        _sceneRenderer = sceneRenderer;
    }

    public IReadOnlyList<ObjectState> Objects => _physicsWorld.Objects;

    public void Initialize(in RectF bounds)
    {
        _nextCubeColorIndex = 0;
        SpawnInitialObjects(bounds);
    }

    public void SetGravity(float gravityY)
    {
        _physicsWorld.SetGravity(new Vector2(0f, gravityY));
    }

    public void Step(float dt, in RectF bounds)
    {
        _physicsWorld.Step(dt, bounds, _config.SleepThreshold, _config.FloorSnapThreshold);
    }

    public void Render(in RectF bounds)
    {
        _sceneRenderer.Render(_physicsWorld.Objects, bounds);
    }

    public ObjectState SpawnCube(Vector2 position)
    {
        return SpawnObject(position, null, ObjectVisualKind.Cube);
    }

    public ObjectState SpawnObject(Vector2 position, Color? color, ObjectVisualKind visualKind)
    {
        var state = new ObjectState
        {
            ZIndex = _physicsWorld.Objects.Count + 1,
            BaseColor = color ?? NextCubeColor(),
            VisualKind = visualKind
        };

        state.Body.Width = 84f;
        state.Body.Height = 84f;
        state.Body.Position = position;
        state.Body.Mass = 1f;
        state.Body.Restitution = _config.Restitution;
        state.Body.LinearDamping = _config.LinearDamping;

        _physicsWorld.Add(state);
        _sceneRenderer.EnsureObjectVisual(state);
        return state;
    }

    public void Reset(in RectF bounds)
    {
        _physicsWorld.Clear();
        _sceneRenderer.Clear();
        _nextCubeColorIndex = 0;
        SpawnInitialObjects(bounds);
    }

    public void ApplyRuntimePhysicsConfig()
    {
        for (var i = 0; i < _physicsWorld.Objects.Count; i++)
        {
            _physicsWorld.Objects[i].Body.Restitution = _config.Restitution;
            _physicsWorld.Objects[i].Body.LinearDamping = _config.LinearDamping;
        }
    }

    private void SpawnInitialObjects(in RectF bounds)
    {
        var startX = bounds.Width * 0.32f;
        var spacing = 104f;
        var startY = bounds.Height * 0.14f;

        for (var i = 0; i < InitialScene.Length; i++)
        {
            var spec = InitialScene[i];
            SpawnObject(
                new Vector2(startX + (spacing * spec.XFactor), startY + spec.YOffset),
                spec.Color,
                spec.VisualKind);
        }
    }

    private Color NextCubeColor()
    {
        var color = CubePalette[_nextCubeColorIndex % CubePalette.Length];
        _nextCubeColorIndex++;
        return color;
    }
}
