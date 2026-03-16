using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Physics;

public enum CollisionShape
{
    Box,
    Circle
}

public sealed class PhysicsBody
{
    public Vector2 Position;
    public Vector2 Velocity;
    public Vector2 Acceleration;

    public float Width;
    public float Height;
    public float Mass;
    public float Restitution;
    public float LinearDamping;
    public float GravityScale = 1f;
    public CollisionShape Shape = CollisionShape.Box;
    public float CollisionScale = 1f;
    public bool IsDragging;
    public bool IsSleeping;
    public float SleepTimerSeconds;
}
