using System.Windows.Media;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Rendering;

public static class AppColorExtensions
{
    public static Color ToWpfColor(this AppColor color)
    {
        return Color.FromArgb(color.A, color.R, color.G, color.B);
    }
}
