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
    private readonly AmbientLight _ambientLight = new(Color.FromRgb(112, 128, 160));
    private readonly DirectionalLight _keyLight = new(Color.FromRgb(255, 255, 255), new Vector3D(0.0, 0.0, 1.0));
    private readonly DirectionalLight _fillLight = new(Color.FromRgb(120, 162, 255), new Vector3D(-0.12, -0.06, 0.92));
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

    public void Render(IReadOnlyList<ObjectState> objects, in RectF bounds, double elapsedSeconds)
    {
        EnsureCamera(bounds);
        UpdateLights(bounds, elapsedSeconds);

        for (var i = 0; i < objects.Count; i++)
        {
            var obj = objects[i];
            EnsureObjectVisual(obj);
            var visual = _visuals[obj.Id];

            var centerX = obj.Body.Position.X + (obj.Body.Width * 0.5f);
            var centerY = obj.Body.Position.Y + (obj.Body.Height * 0.5f);
            var transform = ComputeVisualTransform(obj, centerX, centerY, bounds, elapsedSeconds);
            visual.Update(
                transform.CenterX,
                transform.CenterY,
                transform.CenterZ,
                transform.RotationX,
                transform.RotationY,
                transform.RotationZ,
                transform.ScaleX,
                transform.ScaleY);
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

    private static VisualTransform ComputeVisualTransform(
        ObjectState obj,
        double centerX,
        double centerY,
        in RectF bounds,
        double elapsedSeconds)
    {
        var centerZ = 0.0;
        var rotationX = obj.RotationX;
        var rotationY = obj.RotationY;
        var rotationZ = obj.RotationZ;
        var scaleX = ComputeStretchX(obj);
        var scaleY = ComputeImpactScale(obj, bounds);
        var phase = GetPhase(obj.Id);

        switch (obj.VisualKind)
        {
            case ObjectVisualKind.Crystal:
            {
                var shimmer = Math.Sin((elapsedSeconds * 3.8) + phase);
                centerZ += 22.0 + (shimmer * 12.0);
                scaleX *= 1.0 + (shimmer * 0.035);
                scaleY *= 1.0 + (Math.Cos((elapsedSeconds * 3.1) + phase) * 0.055);
                rotationY += Math.Sin((elapsedSeconds * 1.1) + phase) * 7.0;
                break;
            }
            case ObjectVisualKind.Satellite:
            {
                var wobble = Math.Sin((elapsedSeconds * 2.2) + phase);
                centerZ += 30.0 + (wobble * 7.0);
                rotationZ += wobble * 11.0;
                rotationY += Math.Cos((elapsedSeconds * 1.4) + phase) * 9.0;
                break;
            }
            case ObjectVisualKind.Dice:
            {
                centerZ += 8.0;
                break;
            }
        }

        return new VisualTransform(centerX, centerY, centerZ, rotationX, rotationY, rotationZ, scaleX, scaleY);
    }

    private void UpdateLights(in RectF bounds, double elapsedSeconds)
    {
        _ambientLight.Color = LerpColor(Color.FromRgb(106, 118, 148), Color.FromRgb(132, 156, 194), 0.5 + (Math.Sin(elapsedSeconds * 0.9) * 0.5));
        _keyLight.Direction = new Vector3D(Math.Sin(elapsedSeconds * 0.35) * 0.03, Math.Cos(elapsedSeconds * 0.28) * 0.02, 1.0);
        _fillLight.Direction = new Vector3D(-0.12 + (Math.Sin(elapsedSeconds * 0.42) * 0.03), -0.06 + (Math.Cos(elapsedSeconds * 0.31) * 0.02), 0.92);
    }

    private static ISceneObjectVisual3D CreateVisual(ObjectState state)
    {
        return state.VisualKind switch
        {
            ObjectVisualKind.Dice => new DiceVisual3D(Math.Min(state.Body.Width, state.Body.Height)),
            ObjectVisualKind.Crystal => new CrystalVisual3D(Math.Min(state.Body.Width, state.Body.Height), state.BaseColor),
            ObjectVisualKind.Satellite => new SatelliteVisual3D(Math.Min(state.Body.Width, state.Body.Height), state.BaseColor),
            ObjectVisualKind.ImportedModel when !string.IsNullOrWhiteSpace(state.ModelSourcePath)
                => new ImportedModelVisual3D(state.ModelSourcePath, Math.Min(state.Body.Width, state.Body.Height), state.BaseColor),
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

    private ModelVisual3D BuildLightVisual()
    {
        var lightGroup = new Model3DGroup();
        lightGroup.Children.Add(_ambientLight);
        lightGroup.Children.Add(_keyLight);
        lightGroup.Children.Add(_fillLight);
        return new ModelVisual3D { Content = lightGroup };
    }

    private static double GetPhase(Guid id)
    {
        return (id.GetHashCode() & 0xFFFF) / 65535.0 * Math.PI * 2.0;
    }

    private static Color LerpColor(Color from, Color to, double amount)
    {
        return Color.FromRgb(
            LerpByte(from.R, to.R, amount),
            LerpByte(from.G, to.G, amount),
            LerpByte(from.B, to.B, amount));
    }

    private static byte LerpByte(byte from, byte to, double amount)
    {
        return (byte)Math.Clamp(Math.Round(from + ((to - from) * amount)), 0, 255);
    }

    private readonly record struct VisualTransform(
        double CenterX,
        double CenterY,
        double CenterZ,
        double RotationX,
        double RotationY,
        double RotationZ,
        double ScaleX,
        double ScaleY);
}
