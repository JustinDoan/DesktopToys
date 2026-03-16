using System;
using System.Collections.Generic;
using System.Windows.Media;
using System.Windows.Media.Media3D;

namespace ScreenOverlayPhysics.Rendering;

public abstract class SceneObjectVisual3DBase : ISceneObjectVisual3D
{
    private static readonly Dictionary<string, Model3DGroup> ModelCache = [];
    private static readonly object ModelCacheLock = new();

    private readonly TranslateTransform3D _translate = new();
    private readonly ScaleTransform3D _scale = new(1, 1, 1);
    private readonly AxisAngleRotation3D _rotationX = new(new Vector3D(1, 0, 0), 0);
    private readonly AxisAngleRotation3D _rotationY = new(new Vector3D(0, 1, 0), 0);
    private readonly AxisAngleRotation3D _rotationZ = new(new Vector3D(0, 0, 1), 0);

    public ModelVisual3D Visual { get; }

    protected SceneObjectVisual3DBase(Model3D content)
    {
        var existingTransform = content.Transform;
        var transforms = new Transform3DGroup();
        if (existingTransform is not null && existingTransform != Transform3D.Identity)
        {
            transforms.Children.Add(existingTransform);
        }

        transforms.Children.Add(_scale);
        transforms.Children.Add(new RotateTransform3D(_rotationX));
        transforms.Children.Add(new RotateTransform3D(_rotationY));
        transforms.Children.Add(new RotateTransform3D(_rotationZ));
        transforms.Children.Add(_translate);
        content.Transform = transforms;

        Visual = new ModelVisual3D { Content = content };
    }

    public void Update(
        double centerX,
        double centerY,
        double centerZ,
        double rotationX,
        double rotationY,
        double rotationZ,
        double scaleX,
        double scaleY)
    {
        _rotationX.Angle = rotationX;
        _rotationY.Angle = rotationY;
        _rotationZ.Angle = rotationZ;
        _translate.OffsetX = centerX;
        _translate.OffsetY = centerY;
        _translate.OffsetZ = centerZ;
        _scale.ScaleX = scaleX;
        _scale.ScaleY = scaleY;
        _scale.ScaleZ = 1.0;
    }

    protected static GeometryModel3D CreateQuadModel(
        Point3D p0,
        Point3D p1,
        Point3D p2,
        Point3D p3,
        Material material)
    {
        var mesh = new MeshGeometry3D();
        mesh.Positions.Add(p0);
        mesh.Positions.Add(p1);
        mesh.Positions.Add(p2);
        mesh.Positions.Add(p3);

        mesh.TriangleIndices.Add(0);
        mesh.TriangleIndices.Add(1);
        mesh.TriangleIndices.Add(2);
        mesh.TriangleIndices.Add(0);
        mesh.TriangleIndices.Add(2);
        mesh.TriangleIndices.Add(3);

        return new GeometryModel3D(mesh, material) { BackMaterial = material };
    }

    protected static GeometryModel3D CreateTriangleModel(
        Point3D p0,
        Point3D p1,
        Point3D p2,
        Material material)
    {
        var mesh = new MeshGeometry3D();
        mesh.Positions.Add(p0);
        mesh.Positions.Add(p1);
        mesh.Positions.Add(p2);

        mesh.TriangleIndices.Add(0);
        mesh.TriangleIndices.Add(1);
        mesh.TriangleIndices.Add(2);

        return new GeometryModel3D(mesh, material) { BackMaterial = material };
    }

    protected static GeometryModel3D CreateTexturedQuadModel(
        Point3D p0,
        Point3D p1,
        Point3D p2,
        Point3D p3,
        Material material)
    {
        var model = CreateQuadModel(p0, p1, p2, p3, material);
        if (model.Geometry is MeshGeometry3D mesh)
        {
            mesh.TextureCoordinates.Add(new System.Windows.Point(0, 1));
            mesh.TextureCoordinates.Add(new System.Windows.Point(1, 1));
            mesh.TextureCoordinates.Add(new System.Windows.Point(1, 0));
            mesh.TextureCoordinates.Add(new System.Windows.Point(0, 0));
        }

        return model;
    }

    protected static Color ScaleColor(Color color, double factor)
    {
        static byte Scale(byte value, double amount)
        {
            return (byte)double.Clamp(System.Math.Round(value * amount), 0, 255);
        }

        return Color.FromRgb(
            Scale(color.R, factor),
            Scale(color.G, factor),
            Scale(color.B, factor));
    }

    protected static Material CreateDiffuseMaterial(Color color)
    {
        return new DiffuseMaterial(new SolidColorBrush(color));
    }

    protected static Material CreateEmissiveMaterial(Color color)
    {
        return new EmissiveMaterial(new SolidColorBrush(color));
    }

    protected static MaterialGroup CreateLayeredMaterial(Color diffuseColor, Color emissiveColor)
    {
        var material = new MaterialGroup();
        material.Children.Add(CreateDiffuseMaterial(diffuseColor));
        material.Children.Add(CreateEmissiveMaterial(emissiveColor));
        return material;
    }

    protected static Model3D GetOrCreateCachedModel(string key, Func<Model3DGroup> createModel)
    {
        lock (ModelCacheLock)
        {
            if (ModelCache.TryGetValue(key, out var existing))
            {
                return existing.Clone();
            }

            var created = createModel();
            if (!created.IsFrozen && created.CanFreeze)
            {
                created.Freeze();
            }

            ModelCache[key] = created;
            return created.Clone();
        }
    }
}
