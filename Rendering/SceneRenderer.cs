using System;
using System.Collections.Generic;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Media3D;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Rendering;

public sealed class SceneRenderer
{
    private readonly Viewport3D _viewport;
    private readonly Dictionary<Guid, ISceneObjectVisual3D> _visuals = [];
    private readonly Dictionary<ModelVisual3D, Guid> _objectIdsByVisual = [];
    private readonly ModelVisual3D _lightVisual;

    public SceneRenderer(Viewport3D viewport)
    {
        _viewport = viewport;
        _lightVisual = BuildLightVisual();
        _viewport.Children.Add(_lightVisual);
    }

    public void EnsureObjectVisual(ObjectState state)
    {
        if (_visuals.ContainsKey(state.Id))
        {
            return;
        }

        var visual = CreateVisual(state);

        _visuals.Add(state.Id, visual);
        _objectIdsByVisual[visual.Visual] = state.Id;
        _viewport.Children.Add(visual.Visual);
    }

    public void RemoveObjectVisual(Guid id)
    {
        if (!_visuals.TryGetValue(id, out var visual))
        {
            return;
        }

        _viewport.Children.Remove(visual.Visual);
        _objectIdsByVisual.Remove(visual.Visual);
        _visuals.Remove(id);
    }

    public void Clear()
    {
        _viewport.Children.Clear();
        _viewport.Children.Add(_lightVisual);
        _visuals.Clear();
        _objectIdsByVisual.Clear();
    }

    public void Render(IReadOnlyList<ObjectState> objects, in RectF bounds)
    {
        EnsureCamera(bounds);

        for (var i = 0; i < objects.Count; i++)
        {
            var obj = objects[i];
            EnsureObjectVisual(obj);
            var visual = _visuals[obj.Id];

            var centerX = obj.Body.Position.X + (obj.Body.Width * 0.5f);
            var centerY = obj.Body.Position.Y + (obj.Body.Height * 0.5f);
            var centerZ = 0.0;

            var impactScale = ComputeImpactScale(obj, bounds);
            var stretchX = ComputeStretchX(obj);
            visual.Update(centerX, centerY, centerZ, obj.RotationX, obj.RotationY, obj.RotationZ, stretchX, impactScale);
        }
    }

    public ObjectState? HitTest(IReadOnlyList<ObjectState> objects, Point point)
    {
        ObjectState? best = null;
        double? bestDistance = null;

        VisualTreeHelper.HitTest(
            _viewport,
            null,
            result =>
            {
                if (result is not RayHitTestResult rayHit ||
                    rayHit.VisualHit is not ModelVisual3D visual ||
                    !_objectIdsByVisual.TryGetValue(visual, out var objectId))
                {
                    return HitTestResultBehavior.Continue;
                }

                for (var i = 0; i < objects.Count; i++)
                {
                    if (objects[i].Id != objectId)
                    {
                        continue;
                    }

                    if (bestDistance is null || rayHit.DistanceToRayOrigin < bestDistance.Value)
                    {
                        best = objects[i];
                        bestDistance = rayHit.DistanceToRayOrigin;
                    }

                    break;
                }

                return HitTestResultBehavior.Continue;
            },
            new PointHitTestParameters(point));

        return best;
    }

    private static double ComputeImpactScale(ObjectState obj, in RectF bounds)
    {
        return obj.Body.Position.Y + obj.Body.Height >= bounds.Bottom - 1f && Math.Abs(obj.Body.Velocity.Y) > 300f
            ? 0.96
            : 1.0;
    }

    private static double ComputeStretchX(ObjectState obj)
    {
        return 1.0 + (Math.Min(1.0, Math.Abs(obj.Body.Velocity.X) / 2200f) * 0.03);
    }

    private static ISceneObjectVisual3D CreateVisual(ObjectState state)
    {
        return state.VisualKind switch
        {
            ObjectVisualKind.Dice => new DiceVisual3D(Math.Min(state.Body.Width, state.Body.Height)),
            _ => new CubeVisual3D(Math.Min(state.Body.Width, state.Body.Height), state.BaseColor)
        };
    }

    private void EnsureCamera(in RectF bounds)
    {
        if (_viewport.Camera is OrthographicCamera camera)
        {
            camera.Position = new Point3D(bounds.Width * 0.5, bounds.Height * 0.5, -1200);
            camera.LookDirection = new Vector3D(0, 0, 1200);
            camera.UpDirection = new Vector3D(0, -1, 0);
            camera.Width = bounds.Width;
            camera.NearPlaneDistance = 1;
            camera.FarPlaneDistance = 8000;
            return;
        }

        _viewport.Camera = new OrthographicCamera
        {
            Position = new Point3D(bounds.Width * 0.5, bounds.Height * 0.5, -1200),
            LookDirection = new Vector3D(0, 0, 1200),
            UpDirection = new Vector3D(0, -1, 0),
            Width = bounds.Width,
            NearPlaneDistance = 1,
            FarPlaneDistance = 8000
        };
    }

    private static ModelVisual3D BuildLightVisual()
    {
        var lightGroup = new Model3DGroup();
        lightGroup.Children.Add(new AmbientLight(Color.FromRgb(112, 128, 160)));
        lightGroup.Children.Add(new DirectionalLight(Color.FromRgb(255, 255, 255), new Vector3D(-0.4, -0.3, -1.0)));
        lightGroup.Children.Add(new DirectionalLight(Color.FromRgb(120, 162, 255), new Vector3D(0.5, 0.1, -1.0)));
        return new ModelVisual3D { Content = lightGroup };
    }
}
