using System;
using ScreenOverlayPhysics.Models;
using System.Windows;

namespace ScreenOverlayPhysics.Core;

public sealed class DisplayBoundsService
{
    public event Action<RectF>? BoundsChanged;

    public RectF CurrentPrimaryBounds { get; private set; }

    public DisplayBoundsService()
    {
        CurrentPrimaryBounds = ReadPrimaryBounds();
        Microsoft.Win32.SystemEvents.DisplaySettingsChanged += OnDisplaySettingsChanged;
    }

    public void Dispose()
    {
        Microsoft.Win32.SystemEvents.DisplaySettingsChanged -= OnDisplaySettingsChanged;
    }

    private void OnDisplaySettingsChanged(object? sender, EventArgs e)
    {
        CurrentPrimaryBounds = ReadPrimaryBounds();
        BoundsChanged?.Invoke(CurrentPrimaryBounds);
    }

    private static RectF ReadPrimaryBounds()
    {
        return new RectF(
            0f,
            0f,
            (float)SystemParameters.PrimaryScreenWidth,
            (float)SystemParameters.PrimaryScreenHeight);
    }
}
