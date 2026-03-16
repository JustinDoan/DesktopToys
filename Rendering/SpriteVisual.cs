using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Effects;
using System.Windows.Shapes;

namespace ScreenOverlayPhysics.Rendering;

public sealed class SpriteVisual
{
    public Border Root { get; }
    public RotateTransform RotationTransform { get; }
    public ScaleTransform ScaleTransform { get; }

    public SpriteVisual(double width, double height)
    {
        var gradient = new LinearGradientBrush
        {
            StartPoint = new System.Windows.Point(0, 0),
            EndPoint = new System.Windows.Point(1, 1)
        };
        gradient.GradientStops.Add(new GradientStop(Color.FromRgb(162, 224, 255), 0.0));
        gradient.GradientStops.Add(new GradientStop(Color.FromRgb(46, 116, 214), 0.72));
        gradient.GradientStops.Add(new GradientStop(Color.FromRgb(27, 58, 118), 1.0));

        var highlight = new Rectangle
        {
            Width = width * 0.88,
            Height = height * 0.22,
            RadiusX = 4,
            RadiusY = 4,
            Fill = new SolidColorBrush(Color.FromArgb(66, 255, 255, 255)),
            HorizontalAlignment = System.Windows.HorizontalAlignment.Center,
            VerticalAlignment = System.Windows.VerticalAlignment.Top,
            Margin = new System.Windows.Thickness(0, 5, 0, 0),
            IsHitTestVisible = false
        };

        var container = new Grid();
        container.Children.Add(highlight);

        Root = new Border
        {
            Width = width,
            Height = height,
            Background = gradient,
            CornerRadius = new System.Windows.CornerRadius(8),
            BorderBrush = new SolidColorBrush(Color.FromArgb(120, 255, 255, 255)),
            BorderThickness = new System.Windows.Thickness(1),
            Effect = new DropShadowEffect
            {
                BlurRadius = 16,
                ShadowDepth = 6,
                Opacity = 0.38,
                Color = Color.FromArgb(255, 11, 24, 49)
            },
            Child = container,
            IsHitTestVisible = false,
            RenderTransformOrigin = new System.Windows.Point(0.5, 0.5)
        };

        RotationTransform = new RotateTransform(0);
        ScaleTransform = new ScaleTransform(1, 1);
        var transformGroup = new TransformGroup();
        transformGroup.Children.Add(ScaleTransform);
        transformGroup.Children.Add(RotationTransform);
        Root.RenderTransform = transformGroup;
    }
}
