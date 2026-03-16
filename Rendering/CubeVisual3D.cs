using System.Windows.Media;
using System.Windows.Media.Media3D;

namespace ScreenOverlayPhysics.Rendering;

public sealed class CubeVisual3D : SceneObjectVisual3DBase
{
    public CubeVisual3D(double size, Color baseColor)
        : base(BuildCubeModel(size, baseColor))
    {
    }

    private static Model3DGroup BuildCubeModel(double size, Color baseColor)
    {
        var hs = size * 0.5;
        var group = new Model3DGroup();

        AddFace(group, new Point3D(-hs, -hs, hs), new Point3D(hs, -hs, hs), new Point3D(hs, hs, hs), new Point3D(-hs, hs, hs), ScaleColor(baseColor, 1.10));
        AddFace(group, new Point3D(-hs, -hs, -hs), new Point3D(-hs, hs, -hs), new Point3D(hs, hs, -hs), new Point3D(hs, -hs, -hs), ScaleColor(baseColor, 0.62));
        AddFace(group, new Point3D(-hs, -hs, -hs), new Point3D(-hs, -hs, hs), new Point3D(-hs, hs, hs), new Point3D(-hs, hs, -hs), ScaleColor(baseColor, 0.78));
        AddFace(group, new Point3D(hs, -hs, -hs), new Point3D(hs, hs, -hs), new Point3D(hs, hs, hs), new Point3D(hs, -hs, hs), ScaleColor(baseColor, 0.56));
        AddFace(group, new Point3D(-hs, -hs, -hs), new Point3D(hs, -hs, -hs), new Point3D(hs, -hs, hs), new Point3D(-hs, -hs, hs), ScaleColor(baseColor, 0.96));
        AddFace(group, new Point3D(-hs, hs, -hs), new Point3D(-hs, hs, hs), new Point3D(hs, hs, hs), new Point3D(hs, hs, -hs), ScaleColor(baseColor, 0.70));

        return group;
    }

    private static void AddFace(Model3DGroup group, Point3D p0, Point3D p1, Point3D p2, Point3D p3, Color color)
    {
        var material = new DiffuseMaterial(new SolidColorBrush(color));
        group.Children.Add(CreateQuadModel(p0, p1, p2, p3, material));
    }
}
