namespace ScreenOverlayPhysics.Models;

public readonly struct RectF
{
    public float X { get; }
    public float Y { get; }
    public float Width { get; }
    public float Height { get; }

    public RectF(float x, float y, float width, float height)
    {
        X = x;
        Y = y;
        Width = width;
        Height = height;
    }

    public float Left => X;
    public float Top => Y;
    public float Right => X + Width;
    public float Bottom => Y + Height;

    public bool Contains(Vector2 point) =>
        point.X >= Left &&
        point.X <= Right &&
        point.Y >= Top &&
        point.Y <= Bottom;
}
