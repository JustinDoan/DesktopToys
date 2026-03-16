using System.Windows.Media;
using System.Windows.Media.Media3D;

namespace ScreenOverlayPhysics.Rendering;

public sealed class CrystalVisual3D : SceneObjectVisual3DBase
{
    public CrystalVisual3D(double size, Color baseColor)
        : base(BuildCrystalModel(size, baseColor))
    {
    }

    private static Model3DGroup BuildCrystalModel(double size, Color baseColor)
    {
        var group = new Model3DGroup();
        var radius = size * 0.34;
        var top = new Point3D(0, -size * 0.5, 0);
        var bottom = new Point3D(0, size * 0.5, 0);
        var front = new Point3D(0, 0, radius);
        var right = new Point3D(radius, 0, 0);
        var back = new Point3D(0, 0, -radius);
        var left = new Point3D(-radius, 0, 0);

        var outerMaterial = CreateLayeredMaterial(
            ScaleColor(baseColor, 0.78),
            ScaleColor(baseColor, 0.34));
        var highlightMaterial = CreateLayeredMaterial(
            ScaleColor(baseColor, 1.12),
            ScaleColor(baseColor, 0.55));
        var shadowMaterial = CreateLayeredMaterial(
            ScaleColor(baseColor, 0.56),
            ScaleColor(baseColor, 0.24));

        AddOctahedron(group, top, bottom, front, right, back, left, outerMaterial, highlightMaterial, shadowMaterial);

        var innerScale = 0.46;
        AddOctahedron(
            group,
            ScalePoint(top, innerScale),
            ScalePoint(bottom, innerScale),
            ScalePoint(front, innerScale),
            ScalePoint(right, innerScale),
            ScalePoint(back, innerScale),
            ScalePoint(left, innerScale),
            CreateLayeredMaterial(Color.FromRgb(255, 255, 255), ScaleColor(baseColor, 0.95)),
            CreateLayeredMaterial(ScaleColor(baseColor, 1.25), ScaleColor(baseColor, 1.35)),
            CreateLayeredMaterial(ScaleColor(baseColor, 0.90), ScaleColor(baseColor, 0.82)));

        return group;
    }

    private static void AddOctahedron(
        Model3DGroup group,
        Point3D top,
        Point3D bottom,
        Point3D front,
        Point3D right,
        Point3D back,
        Point3D left,
        Material sideMaterial,
        Material highlightMaterial,
        Material shadowMaterial)
    {
        group.Children.Add(CreateTriangleModel(top, front, right, highlightMaterial));
        group.Children.Add(CreateTriangleModel(top, right, back, sideMaterial));
        group.Children.Add(CreateTriangleModel(top, back, left, shadowMaterial));
        group.Children.Add(CreateTriangleModel(top, left, front, sideMaterial));
        group.Children.Add(CreateTriangleModel(bottom, right, front, highlightMaterial));
        group.Children.Add(CreateTriangleModel(bottom, back, right, sideMaterial));
        group.Children.Add(CreateTriangleModel(bottom, left, back, shadowMaterial));
        group.Children.Add(CreateTriangleModel(bottom, front, left, sideMaterial));
    }

    private static Point3D ScalePoint(Point3D point, double scale)
    {
        return new Point3D(point.X * scale, point.Y * scale, point.Z * scale);
    }
}
