using System.Windows.Media;
using System.Windows.Media.Media3D;

namespace ScreenOverlayPhysics.Rendering;

public sealed class SatelliteVisual3D : SceneObjectVisual3DBase
{
    public SatelliteVisual3D(double size, Color accentColor)
        : base(GetOrCreateCachedModel(CacheKey(size, accentColor), () => BuildSatelliteModel(size, accentColor)))
    {
    }

    private static string CacheKey(double size, Color accentColor)
    {
        return $"satellite:{size:0.###}:{accentColor.R:X2}{accentColor.G:X2}{accentColor.B:X2}";
    }

    private static Model3DGroup BuildSatelliteModel(double size, Color accentColor)
    {
        var group = new Model3DGroup();

        AddCuboid(group, -size * 0.18, -size * 0.18, -size * 0.18, size * 0.18, size * 0.18, size * 0.18, Color.FromRgb(168, 140, 74));
        AddCuboid(group, -size * 0.62, -size * 0.12, -size * 0.04, -size * 0.24, size * 0.12, size * 0.04, ScaleColor(accentColor, 0.82));
        AddCuboid(group, size * 0.24, -size * 0.12, -size * 0.04, size * 0.62, size * 0.12, size * 0.04, ScaleColor(accentColor, 0.82));
        AddCuboid(group, -size * 0.03, -size * 0.42, -size * 0.03, size * 0.03, -size * 0.18, size * 0.03, Color.FromRgb(146, 154, 165));
        AddCuboid(group, -size * 0.18, -size * 0.46, -size * 0.18, size * 0.18, -size * 0.40, size * 0.18, Color.FromRgb(210, 215, 223));
        AddCuboid(group, -size * 0.08, size * 0.21, -size * 0.08, size * 0.08, size * 0.38, size * 0.08, Color.FromRgb(66, 76, 92));
        AddCuboid(group, -size * 0.12, size * 0.34, -size * 0.12, size * 0.12, size * 0.52, size * 0.12, Color.FromRgb(210, 72, 72));

        return group;
    }

    private static void AddCuboid(
        Model3DGroup group,
        double minX,
        double minY,
        double minZ,
        double maxX,
        double maxY,
        double maxZ,
        Color color)
    {
        var material = CreateDiffuseMaterial(color);
        var p000 = new Point3D(minX, minY, minZ);
        var p001 = new Point3D(minX, minY, maxZ);
        var p010 = new Point3D(minX, maxY, minZ);
        var p011 = new Point3D(minX, maxY, maxZ);
        var p100 = new Point3D(maxX, minY, minZ);
        var p101 = new Point3D(maxX, minY, maxZ);
        var p110 = new Point3D(maxX, maxY, minZ);
        var p111 = new Point3D(maxX, maxY, maxZ);
        var mesh = new MeshGeometry3D();

        AddQuad(mesh, p001, p101, p111, p011);
        AddQuad(mesh, p100, p000, p010, p110);
        AddQuad(mesh, p000, p001, p011, p010);
        AddQuad(mesh, p101, p100, p110, p111);
        AddQuad(mesh, p000, p100, p101, p001);
        AddQuad(mesh, p011, p111, p110, p010);

        group.Children.Add(new GeometryModel3D(mesh, material) { BackMaterial = material });
    }

    private static void AddQuad(MeshGeometry3D mesh, Point3D p0, Point3D p1, Point3D p2, Point3D p3)
    {
        var start = mesh.Positions.Count;
        mesh.Positions.Add(p0);
        mesh.Positions.Add(p1);
        mesh.Positions.Add(p2);
        mesh.Positions.Add(p3);
        mesh.TriangleIndices.Add(start);
        mesh.TriangleIndices.Add(start + 1);
        mesh.TriangleIndices.Add(start + 2);
        mesh.TriangleIndices.Add(start);
        mesh.TriangleIndices.Add(start + 2);
        mesh.TriangleIndices.Add(start + 3);
    }
}
