using System;

namespace ScreenOverlayPhysics.Models;

public struct Vector2
{
    public float X;
    public float Y;

    public Vector2(float x, float y)
    {
        X = x;
        Y = y;
    }

    public float LengthSquared => (X * X) + (Y * Y);

    public static Vector2 Zero => new(0f, 0f);

    public static Vector2 operator +(Vector2 left, Vector2 right) => new(left.X + right.X, left.Y + right.Y);
    public static Vector2 operator -(Vector2 left, Vector2 right) => new(left.X - right.X, left.Y - right.Y);
    public static Vector2 operator *(Vector2 left, float scalar) => new(left.X * scalar, left.Y * scalar);
    public static Vector2 operator /(Vector2 left, float scalar) => new(left.X / scalar, left.Y / scalar);
}
