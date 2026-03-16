using System;
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

    public ObjectState SpawnRandomCrystal(Vector2 position)
    {
        return SpawnObject(position, RandomCrystalColor(), ObjectVisualKind.Crystal);
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
        state.Body.GravityScale = visualKind == ObjectVisualKind.Satellite ? 0f : 1f;
        state.Body.Shape = visualKind == ObjectVisualKind.Crystal ? CollisionShape.Circle : CollisionShape.Box;
        state.Body.CollisionScale = visualKind == ObjectVisualKind.Crystal ? 0.82f : 1f;
        state.Body.IsSleeping = false;
        state.Body.SleepTimerSeconds = 0f;

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
        state.Body.GravityScale = 1f;
        state.Body.Shape = CollisionShape.Box;
        state.Body.CollisionScale = 1f;
        state.Body.IsSleeping = false;
        state.Body.SleepTimerSeconds = 0f;

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

    private static Color RandomCrystalColor()
    {
        var hue = Random.Shared.NextDouble() * 360.0;
        return ColorFromHsv(hue, 0.55, 1.0);
    }

    private static Color ColorFromHsv(double hue, double saturation, double value)
    {
        hue = ((hue % 360.0) + 360.0) % 360.0;
        var chroma = value * saturation;
        var segment = hue / 60.0;
        var x = chroma * (1.0 - Math.Abs((segment % 2.0) - 1.0));
        double red;
        double green;
        double blue;

        if (segment < 1.0)
        {
            red = chroma;
            green = x;
            blue = 0.0;
        }
        else if (segment < 2.0)
        {
            red = x;
            green = chroma;
            blue = 0.0;
        }
        else if (segment < 3.0)
        {
            red = 0.0;
            green = chroma;
            blue = x;
        }
        else if (segment < 4.0)
        {
            red = 0.0;
            green = x;
            blue = chroma;
        }
        else if (segment < 5.0)
        {
            red = x;
            green = 0.0;
            blue = chroma;
        }
        else
        {
            red = chroma;
            green = 0.0;
            blue = x;
        }

        var match = value - chroma;
        return Color.FromRgb(
            ToByte(red + match),
            ToByte(green + match),
            ToByte(blue + match));
    }

    private static byte ToByte(double value)
    {
        return (byte)Math.Clamp(Math.Round(value * 255.0), 0.0, 255.0);
    }
}
