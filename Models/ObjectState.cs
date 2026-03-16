using System;
using System.Windows.Media;
using ScreenOverlayPhysics.Physics;

namespace ScreenOverlayPhysics.Models;

public enum ObjectVisualKind
{
    Cube,
    Dice,
    Crystal,
    Satellite,
    ImportedModel
}

public sealed class ObjectState
{
    public Guid Id { get; init; } = Guid.NewGuid();
    public PhysicsBody Body { get; init; } = new();
    public double RotationX { get; set; }
    public double RotationY { get; set; }
    public double RotationZ { get; set; }
    public double AngularVelocityX { get; set; }
    public double AngularVelocityY { get; set; }
    public double AngularVelocityZ { get; set; }
    public bool IsHovered { get; set; }
    public bool IsDragging { get; set; }
    public int ZIndex { get; set; }
    public Color BaseColor { get; set; } = Color.FromRgb(127, 202, 255);
    public ObjectVisualKind VisualKind { get; set; } = ObjectVisualKind.Cube;
    public string? ModelSourcePath { get; set; }
    public float ModelScaleMultiplier { get; set; } = 1f;
}
