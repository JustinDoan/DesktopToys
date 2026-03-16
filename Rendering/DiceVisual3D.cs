using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Media3D;

namespace ScreenOverlayPhysics.Rendering;

public sealed class DiceVisual3D : SceneObjectVisual3DBase
{
    public DiceVisual3D(double size)
        : base(BuildDiceModel(size))
    {
    }

    private static Model3DGroup BuildDiceModel(double size)
    {
        var hs = size * 0.5;
        var group = new Model3DGroup();

        AddFace(group,
            new Point3D(-hs, -hs, hs),
            new Point3D(hs, -hs, hs),
            new Point3D(hs, hs, hs),
            new Point3D(-hs, hs, hs),
            CreateFaceMaterial(1, 1.02));

        AddFace(group,
            new Point3D(-hs, -hs, -hs),
            new Point3D(-hs, hs, -hs),
            new Point3D(hs, hs, -hs),
            new Point3D(hs, -hs, -hs),
            CreateFaceMaterial(6, 0.84));

        AddFace(group,
            new Point3D(-hs, -hs, -hs),
            new Point3D(-hs, -hs, hs),
            new Point3D(-hs, hs, hs),
            new Point3D(-hs, hs, -hs),
            CreateFaceMaterial(4, 0.92));

        AddFace(group,
            new Point3D(hs, -hs, -hs),
            new Point3D(hs, hs, -hs),
            new Point3D(hs, hs, hs),
            new Point3D(hs, -hs, hs),
            CreateFaceMaterial(3, 0.80));

        AddFace(group,
            new Point3D(-hs, -hs, -hs),
            new Point3D(hs, -hs, -hs),
            new Point3D(hs, -hs, hs),
            new Point3D(-hs, -hs, hs),
            CreateFaceMaterial(5, 0.97));

        AddFace(group,
            new Point3D(-hs, hs, -hs),
            new Point3D(-hs, hs, hs),
            new Point3D(hs, hs, hs),
            new Point3D(hs, hs, -hs),
            CreateFaceMaterial(2, 0.76));

        return group;
    }

    private static Material CreateFaceMaterial(int pipCount, double shadeFactor)
    {
        var brush = new DrawingBrush
        {
            Drawing = BuildFaceDrawing(pipCount, shadeFactor),
            Stretch = Stretch.Fill,
            ViewportUnits = BrushMappingMode.RelativeToBoundingBox
        };

        brush.Freeze();
        var material = new DiffuseMaterial(brush);
        material.Freeze();
        return material;
    }

    private static Drawing BuildFaceDrawing(int pipCount, double shadeFactor)
    {
        var drawingGroup = new DrawingGroup();
        var faceColor = ScaleColor(Color.FromRgb(245, 245, 240), shadeFactor);
        var borderPen = new Pen(new SolidColorBrush(Color.FromRgb(210, 210, 205)), 4);
        borderPen.Freeze();

        drawingGroup.Children.Add(new GeometryDrawing(
            new SolidColorBrush(faceColor),
            borderPen,
            new RectangleGeometry(new Rect(0, 0, 100, 100), 12, 12)));

        foreach (var center in GetPipCenters(pipCount))
        {
            drawingGroup.Children.Add(new GeometryDrawing(
                Brushes.Black,
                null,
                new EllipseGeometry(center, 9, 9)));
        }

        drawingGroup.Freeze();
        return drawingGroup;
    }

    private static Point[] GetPipCenters(int pipCount)
    {
        var topLeft = new Point(27, 27);
        var topRight = new Point(73, 27);
        var middleLeft = new Point(27, 50);
        var middle = new Point(50, 50);
        var middleRight = new Point(73, 50);
        var bottomLeft = new Point(27, 73);
        var bottomRight = new Point(73, 73);

        return pipCount switch
        {
            1 => [middle],
            2 => [topLeft, bottomRight],
            3 => [topLeft, middle, bottomRight],
            4 => [topLeft, topRight, bottomLeft, bottomRight],
            5 => [topLeft, topRight, middle, bottomLeft, bottomRight],
            6 => [topLeft, topRight, middleLeft, middleRight, bottomLeft, bottomRight],
            _ => []
        };
    }

    private static void AddFace(Model3DGroup group, Point3D p0, Point3D p1, Point3D p2, Point3D p3, Material material)
    {
        group.Children.Add(CreateTexturedQuadModel(p0, p1, p2, p3, material));
    }
}
