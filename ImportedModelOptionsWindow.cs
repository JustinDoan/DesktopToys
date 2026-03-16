using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;

namespace ScreenOverlayPhysics;

public sealed class ImportedModelOptionsWindow : Window
{
    private readonly Slider _scale;
    private readonly Slider _red;
    private readonly Slider _green;
    private readonly Slider _blue;
    private readonly TextBlock _scaleValueText;
    private readonly Border _preview;
    private readonly Border _scalePreview;

    public ImportedModelOptionsWindow()
    {
        Title = "Import Options";
        Width = 360;
        Height = 470;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ResizeMode = ResizeMode.NoResize;

        var root = new Grid
        {
            Margin = new Thickness(12)
        };

        for (var i = 0; i < 7; i++)
        {
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        }

        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        _scale = AddSlider(root, 0, "Scale", 0.25, 3.0, 1.0);
        _scaleValueText = new TextBlock
        {
            Margin = new Thickness(0, 0, 0, 8),
            Foreground = new SolidColorBrush(Color.FromRgb(180, 180, 180))
        };
        Grid.SetRow(_scaleValueText, 1);
        root.Children.Add(_scaleValueText);

        _red = AddSlider(root, 2, "Tint R", 0, 255, 255);
        _green = AddSlider(root, 3, "Tint G", 0, 255, 255);
        _blue = AddSlider(root, 4, "Tint B", 0, 255, 255);

        var previewPanel = new StackPanel
        {
            Margin = new Thickness(0, 8, 0, 12),
            Orientation = Orientation.Vertical
        };
        Grid.SetRow(previewPanel, 5);
        root.Children.Add(previewPanel);

        var scalePreviewFrame = new Border
        {
            Height = 112,
            Width = 112,
            HorizontalAlignment = HorizontalAlignment.Center,
            BorderThickness = new Thickness(1),
            BorderBrush = new SolidColorBrush(Color.FromRgb(86, 86, 86)),
            Background = new SolidColorBrush(Color.FromRgb(28, 28, 28)),
            CornerRadius = new CornerRadius(8),
            Child = new Grid()
        };
        previewPanel.Children.Add(scalePreviewFrame);

        _scalePreview = new Border
        {
            Width = 44,
            Height = 44,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            CornerRadius = new CornerRadius(6),
            BorderThickness = new Thickness(1),
            BorderBrush = new SolidColorBrush(Color.FromRgb(86, 86, 86))
        };
        ((Grid)scalePreviewFrame.Child).Children.Add(_scalePreview);

        _preview = new Border
        {
            Height = 44,
            Margin = new Thickness(0, 10, 0, 0),
            CornerRadius = new CornerRadius(6),
            BorderThickness = new Thickness(1),
            BorderBrush = new SolidColorBrush(Color.FromRgb(86, 86, 86))
        };
        previewPanel.Children.Add(_preview);

        UpdatePreview();
        _scale.ValueChanged += (_, _) => UpdatePreview();
        _red.ValueChanged += (_, _) => UpdatePreview();
        _green.ValueChanged += (_, _) => UpdatePreview();
        _blue.ValueChanged += (_, _) => UpdatePreview();

        var buttons = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Right
        };

        var cancel = new Button { Content = "Cancel", Width = 84, Margin = new Thickness(0, 0, 8, 0) };
        cancel.Click += (_, _) => DialogResult = false;

        var import = new Button { Content = "Import", Width = 84 };
        import.Click += (_, _) => DialogResult = true;

        buttons.Children.Add(cancel);
        buttons.Children.Add(import);
        Grid.SetRow(buttons, 8);
        root.Children.Add(buttons);

        Content = root;
    }

    public float ScaleMultiplier => (float)_scale.Value;

    public Color Tint => Color.FromRgb((byte)_red.Value, (byte)_green.Value, (byte)_blue.Value);

    private static Slider AddSlider(Grid parent, int row, string label, double min, double max, double value)
    {
        var panel = new StackPanel
        {
            Margin = new Thickness(0, 0, 0, 8)
        };

        panel.Children.Add(new TextBlock
        {
            Text = label,
            Margin = new Thickness(0, 0, 0, 2)
        });

        var slider = new Slider
        {
            Minimum = min,
            Maximum = max,
            Value = value
        };

        panel.Children.Add(slider);
        Grid.SetRow(panel, row);
        parent.Children.Add(panel);
        return slider;
    }

    private void UpdatePreview()
    {
        var tintBrush = new SolidColorBrush(Tint);
        _preview.Background = tintBrush;
        _scalePreview.Background = tintBrush;

        var previewSize = 44.0 * _scale.Value;
        previewSize = double.Clamp(previewSize, 14.0, 92.0);
        _scalePreview.Width = previewSize;
        _scalePreview.Height = previewSize;
        _scaleValueText.Text = $"Scale: {_scale.Value:0.00}x  |  relative footprint preview";
    }
}
