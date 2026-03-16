using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Physics;

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
    public bool IsDragging;
}
