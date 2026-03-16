using System.Windows;
using System.Windows.Controls;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics;

public sealed class SettingsWindow : Window
{
    private readonly AppConfig _config;
    private readonly Slider _gravity;
    private readonly Slider _restitution;
    private readonly Slider _damping;
    private readonly Slider _throwSensitivity;

    public SettingsWindow(AppConfig config)
    {
        _config = config;

        Title = "Overlay Settings";
        Width = 340;
        Height = 260;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ResizeMode = ResizeMode.NoResize;

        var root = new Grid
        {
            Margin = new Thickness(12)
        };

        for (var i = 0; i < 5; i++)
        {
            root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        }
        root.RowDefinitions.Add(new RowDefinition());
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });

        _gravity = AddSlider(root, 0, "Gravity", 300, 3000, _config.GravityY);
        _restitution = AddSlider(root, 1, "Restitution", 0.2, 0.95, _config.Restitution);
        _damping = AddSlider(root, 2, "Damping", 0.90, 0.999, _config.LinearDamping);
        _throwSensitivity = AddSlider(root, 3, "Throw Sensitivity", 0.4, 2.5, _config.ThrowSensitivity);

        var buttons = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Right
        };

        var cancel = new Button { Content = "Cancel", Width = 78, Margin = new Thickness(0, 0, 8, 0) };
        cancel.Click += (_, _) => DialogResult = false;
        var save = new Button { Content = "Save", Width = 78 };
        save.Click += (_, _) => SaveAndClose();

        buttons.Children.Add(cancel);
        buttons.Children.Add(save);
        Grid.SetRow(buttons, 6);
        root.Children.Add(buttons);

        Content = root;
    }

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

    private void SaveAndClose()
    {
        _config.GravityY = (float)_gravity.Value;
        _config.Restitution = (float)_restitution.Value;
        _config.LinearDamping = (float)_damping.Value;
        _config.ThrowSensitivity = (float)_throwSensitivity.Value;
        DialogResult = true;
    }
}
