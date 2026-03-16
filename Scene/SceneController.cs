using System.Collections.Generic;
using System.Windows.Media;
using ScreenOverlayPhysics.Models;
using ScreenOverlayPhysics.Physics;
using ScreenOverlayPhysics.Rendering;

namespace ScreenOverlayPhysics.Scene;

public sealed class SceneController
{
    private readonly record struct InitialObjectSpec(float XFactor, float YOffset, ObjectVisualKind VisualKind, Color? Color = null);
    private readonly record struct SpawnSpec(ObjectVisualKind VisualKind, Color? Color = null);

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
        new(1.00f, 14f, ObjectVisualKind.Crystal, Color.FromRgb(108, 241, 255)),
        new(2.00f, 28f, ObjectVisualKind.Satellite, Color.FromRgb(88, 160, 255)),
        new(3.00f, 42f, ObjectVisualKind.Cube),
        new(1.80f, -126f, ObjectVisualKind.Dice, Color.FromRgb(245, 245, 240)),
        new(2.85f, -92f, ObjectVisualKind.Crystal, Color.FromRgb(255, 112, 214))
    ];

    private static readonly SpawnSpec[] SpawnCatalog =
    [
        new(ObjectVisualKind.Cube),
        new(ObjectVisualKind.Crystal, Color.FromRgb(108, 241, 255)),
        new(ObjectVisualKind.Satellite, Color.FromRgb(88, 160, 255)),
        new(ObjectVisualKind.Dice, Color.FromRgb(245, 245, 240)),
        new(ObjectVisualKind.Crystal, Color.FromRgb(255, 112, 214))
    ];

    private readonly AppConfig _config;
    private readonly PhysicsWorld _physicsWorld;
    private readonly SceneRenderer _sceneRenderer;
    private int _nextCubeColorIndex;
    private int _nextSpawnCatalogIndex;

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
        _nextSpawnCatalogIndex = 0;
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

    public void Render(in RectF bounds, double elapsedSeconds)
    {
        _sceneRenderer.Render(_physicsWorld.Objects, bounds, elapsedSeconds);
    }

    public ObjectState SpawnNextObject(Vector2 position)
    {
        var spec = SpawnCatalog[_nextSpawnCatalogIndex % SpawnCatalog.Length];
        _nextSpawnCatalogIndex++;
        return SpawnObject(position, spec.Color, spec.VisualKind);
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

    public ObjectState SpawnImportedModel(Vector2 position, string sourcePath, float scaleMultiplier, Color tint)
    {
        var state = new ObjectState
        {
            ZIndex = _physicsWorld.Objects.Count + 1,
            BaseColor = tint,
            VisualKind = ObjectVisualKind.ImportedModel,
            ModelSourcePath = sourcePath,
            ModelScaleMultiplier = scaleMultiplier
        };

        var scaledSize = 84f * scaleMultiplier;
        state.Body.Width = scaledSize;
        state.Body.Height = scaledSize;
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
        _nextSpawnCatalogIndex = 0;
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
