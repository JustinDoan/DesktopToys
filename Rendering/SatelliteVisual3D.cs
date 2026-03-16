using System.Windows.Media;
using System.Windows.Media.Media3D;

namespace ScreenOverlayPhysics.Rendering;

public sealed class SatelliteVisual3D : SceneObjectVisual3DBase
{
    public SatelliteVisual3D(double size, Color accentColor)
        : base(BuildSatelliteModel(size, accentColor))
    {
    }

    private static Model3DGroup BuildSatelliteModel(double size, Color accentColor)
    {
        var group = new Model3DGroup();

        AddCuboid(
            group,
            -size * 0.18,
            -size * 0.18,
            -size * 0.18,
            size * 0.18,
            size * 0.18,
            size * 0.18,
            Color.FromRgb(196, 170, 92),
            Color.FromRgb(126, 102, 46));

        AddCuboid(
            group,
            -size * 0.62,
            -size * 0.12,
            -size * 0.04,
            -size * 0.24,
            size * 0.12,
            size * 0.04,
            ScaleColor(accentColor, 0.94),
            ScaleColor(accentColor, 0.62));

        AddCuboid(
            group,
            size * 0.24,
            -size * 0.12,
            -size * 0.04,
            size * 0.62,
            size * 0.12,
            size * 0.04,
            ScaleColor(accentColor, 0.94),
            ScaleColor(accentColor, 0.62));

        AddCuboid(
            group,
            -size * 0.03,
            -size * 0.42,
            -size * 0.03,
            size * 0.03,
            -size * 0.18,
            size * 0.03,
            Color.FromRgb(180, 188, 196),
            Color.FromRgb(118, 126, 135));

        AddCuboid(
            group,
            -size * 0.18,
            -size * 0.46,
            -size * 0.18,
            size * 0.18,
            -size * 0.40,
            size * 0.18,
            Color.FromRgb(236, 240, 245),
            Color.FromRgb(188, 194, 202));

        AddCuboid(
            group,
            -size * 0.08,
            size * 0.21,
            -size * 0.08,
            size * 0.08,
            size * 0.38,
            size * 0.08,
            Color.FromRgb(82, 94, 110),
            Color.FromRgb(52, 60, 74));

        AddCuboid(
            group,
            -size * 0.12,
            size * 0.34,
            -size * 0.12,
            size * 0.12,
            size * 0.52,
            size * 0.12,
            Color.FromRgb(242, 92, 92),
            Color.FromRgb(180, 58, 58));

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
        Color lightColor,
        Color darkColor)
    {
        var frontMaterial = CreateDiffuseMaterial(lightColor);
        var sideMaterial = CreateDiffuseMaterial(ScaleColor(lightColor, 0.82));
        var darkMaterial = CreateDiffuseMaterial(darkColor);

        var p000 = new Point3D(minX, minY, minZ);
        var p001 = new Point3D(minX, minY, maxZ);
        var p010 = new Point3D(minX, maxY, minZ);
        var p011 = new Point3D(minX, maxY, maxZ);
        var p100 = new Point3D(maxX, minY, minZ);
        var p101 = new Point3D(maxX, minY, maxZ);
        var p110 = new Point3D(maxX, maxY, minZ);
        var p111 = new Point3D(maxX, maxY, maxZ);

        group.Children.Add(CreateQuadModel(p001, p101, p111, p011, frontMaterial));
        group.Children.Add(CreateQuadModel(p100, p000, p010, p110, darkMaterial));
        group.Children.Add(CreateQuadModel(p000, p001, p011, p010, sideMaterial));
        group.Children.Add(CreateQuadModel(p101, p100, p110, p111, darkMaterial));
        group.Children.Add(CreateQuadModel(p000, p100, p101, p001, sideMaterial));
        group.Children.Add(CreateQuadModel(p011, p111, p110, p010, darkMaterial));
    }
}
