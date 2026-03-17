namespace ScreenOverlayPhysics.Models;

public readonly record struct AppColor(byte A, byte R, byte G, byte B)
{
    public static AppColor FromRgb(byte r, byte g, byte b)
    {
        return new AppColor(255, r, g, b);
    }

    public static AppColor FromArgb(byte a, byte r, byte g, byte b)
    {
        return new AppColor(a, r, g, b);
    }
}
