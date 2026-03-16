using System.Windows.Media;
using System.Windows.Media.Media3D;

namespace ScreenOverlayPhysics.Rendering;

public sealed class CrystalVisual3D : SceneObjectVisual3DBase
{
    public CrystalVisual3D(double size, Color baseColor)
        : base(GetOrCreateCachedModel(CacheKey(size, baseColor), () => BuildCrystalModel(size, baseColor)))
    {
    }

    private static string CacheKey(double size, Color baseColor)
    {
        return $"crystal:{size:0.###}:{baseColor.R:X2}{baseColor.G:X2}{baseColor.B:X2}";
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
        var highlightMesh = new MeshGeometry3D();
        var sideMesh = new MeshGeometry3D();
        var shadowMesh = new MeshGeometry3D();

        AddTriangle(highlightMesh, top, front, right);
        AddTriangle(sideMesh, top, right, back);
        AddTriangle(shadowMesh, top, back, left);
        AddTriangle(sideMesh, top, left, front);
        AddTriangle(highlightMesh, bottom, right, front);
        AddTriangle(sideMesh, bottom, back, right);
        AddTriangle(shadowMesh, bottom, left, back);
        AddTriangle(sideMesh, bottom, front, left);

        group.Children.Add(new GeometryModel3D(highlightMesh, highlightMaterial) { BackMaterial = highlightMaterial });
        group.Children.Add(new GeometryModel3D(sideMesh, sideMaterial) { BackMaterial = sideMaterial });
        group.Children.Add(new GeometryModel3D(shadowMesh, shadowMaterial) { BackMaterial = shadowMaterial });
    }

    private static void AddTriangle(MeshGeometry3D mesh, Point3D p0, Point3D p1, Point3D p2)
    {
        var start = mesh.Positions.Count;
        mesh.Positions.Add(p0);
        mesh.Positions.Add(p1);
        mesh.Positions.Add(p2);
        mesh.TriangleIndices.Add(start);
        mesh.TriangleIndices.Add(start + 1);
        mesh.TriangleIndices.Add(start + 2);
    }

    private static Point3D ScalePoint(Point3D point, double scale)
    {
        return new Point3D(point.X * scale, point.Y * scale, point.Z * scale);
    }
}
