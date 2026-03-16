using System;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using ScreenOverlayPhysics.Models;
using Window = System.Windows.Window;

namespace ScreenOverlayPhysics.Core;

public enum OverlayInputMode
{
    PassThrough,
    Interactive
}

public sealed class OverlayWindowService
{
    private readonly Window _window;
    private readonly HwndSource _source;
    private OverlayInputMode _currentMode;

    public OverlayWindowService(Window window)
    {
        _window = window;
        _source = (HwndSource)PresentationSource.FromVisual(window)!;
        _currentMode = OverlayInputMode.Interactive;
    }

    public OverlayInputMode CurrentMode => _currentMode;

    public void ApplyWindowBounds(in RectF bounds)
    {
        _window.Left = bounds.X;
        _window.Top = bounds.Y;
        _window.Width = bounds.Width;
        _window.Height = bounds.Height;
    }

    public void ApplyOverlayStyles()
    {
        var handle = _source.Handle;
        var style = Win32Interop.GetWindowLongPtr(handle, Win32Interop.GwlExStyle).ToInt64();
        style |= Win32Interop.WsExLayered;
        style |= Win32Interop.WsExToolWindow;
        Win32Interop.SetWindowLongPtr(handle, Win32Interop.GwlExStyle, new IntPtr(style));

        EnsureTopmost();
    }

    public void EnsureTopmost()
    {
        Win32Interop.SetWindowPos(
            _source.Handle,
            Win32Interop.HwndTopmost,
            0,
            0,
            0,
            0,
            Win32Interop.SwpNomove | Win32Interop.SwpNosize | Win32Interop.SwpNoactivate | Win32Interop.SwpShowwindow);
    }

    public void SetInputMode(OverlayInputMode mode)
    {
        if (mode == _currentMode)
        {
            return;
        }

        var handle = _source.Handle;
        var style = Win32Interop.GetWindowLongPtr(handle, Win32Interop.GwlExStyle).ToInt64();

        if (mode == OverlayInputMode.PassThrough)
        {
            style |= Win32Interop.WsExTransparent;
        }
        else
        {
            style &= ~Win32Interop.WsExTransparent;
        }

        Win32Interop.SetWindowLongPtr(handle, Win32Interop.GwlExStyle, new IntPtr(style));
        EnsureTopmost();
        _currentMode = mode;
    }
}
