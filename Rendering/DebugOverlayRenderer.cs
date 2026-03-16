using System.Text;
using System.Windows;
using System.Windows.Controls;
using ScreenOverlayPhysics.Core;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Rendering;

public sealed class DebugOverlayRenderer
{
    private readonly Border _panel;
    private readonly TextBlock _text;
    private readonly StringBuilder _buffer = new(256);

    public DebugOverlayRenderer(Border panel, TextBlock text)
    {
        _panel = panel;
        _text = text;
    }

    public bool IsVisible => _panel.Visibility == Visibility.Visible;

    public void Toggle()
    {
        _panel.Visibility = IsVisible ? Visibility.Collapsed : Visibility.Visible;
    }

    public void Render(FrameClock frameClock, ObjectState? selected, OverlayInputMode mode, string? extraDiagnostics)
    {
        if (!IsVisible)
        {
            return;
        }

        _buffer.Clear();
        _buffer.Append("F1 Debug | F2 Spawn | F3 Reset | F5 ForceInput | Esc Quit\n");
        _buffer.AppendFormat("FPS: {0:0.0}  Frame: {1:0.00} ms\n", frameClock.Fps, frameClock.FrameTimeMs);
        _buffer.AppendFormat("Mode: {0}\n", mode);

        if (selected is null)
        {
            _buffer.Append("Object: none");
        }
        else
        {
            _buffer.AppendFormat("Pos: ({0:0.0}, {1:0.0})\n", selected.Body.Position.X, selected.Body.Position.Y);
            _buffer.AppendFormat("Vel: ({0:0.0}, {1:0.0})\n", selected.Body.Velocity.X, selected.Body.Velocity.Y);
            _buffer.AppendFormat("Rot: ({0:0.0}, {1:0.0}, {2:0.0})\n", selected.RotationX, selected.RotationY, selected.RotationZ);
            _buffer.AppendFormat("Dragging: {0}", selected.Body.IsDragging ? "yes" : "no");
        }

        if (!string.IsNullOrWhiteSpace(extraDiagnostics))
        {
            _buffer.Append('\n');
            _buffer.Append(extraDiagnostics);
        }

        _text.Text = _buffer.ToString();
    }
}
