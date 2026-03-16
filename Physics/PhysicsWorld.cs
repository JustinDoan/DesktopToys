using System;
using System.Collections.Generic;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Physics;

public sealed class PhysicsWorld
{
    private readonly List<ObjectState> _objects = [];
    private Vector2 _gravity;

    public PhysicsWorld(Vector2 gravity)
    {
        _gravity = gravity;
    }

    public IReadOnlyList<ObjectState> Objects => _objects;

    public void SetGravity(Vector2 gravity) => _gravity = gravity;

    public void Add(ObjectState state) => _objects.Add(state);

    public void Clear() => _objects.Clear();

    public void Step(float dt, in RectF bounds, float sleepThreshold, float floorSnapThreshold)
    {
        for (var i = 0; i < _objects.Count; i++)
        {
            var body = _objects[i].Body;

            if (body.IsDragging)
            {
                continue;
            }

            body.Velocity += (_gravity + body.Acceleration) * dt;
            body.Velocity *= body.LinearDamping;
            body.Position += body.Velocity * dt;
        }

        CollisionSolver.SolveObjectCollisions(_objects);

        for (var i = 0; i < _objects.Count; i++)
        {
            var body = _objects[i].Body;

            if (body.IsDragging)
            {
                continue;
            }

            CollisionSolver.SolveScreenBounds(body, bounds, sleepThreshold, floorSnapThreshold);

            var obj = _objects[i];
            obj.AngularVelocityY += body.Velocity.X * 0.00045;
            obj.AngularVelocityX += body.Velocity.Y * 0.00018;
            obj.AngularVelocityZ += body.Velocity.X * 0.00012;

            obj.AngularVelocityX = Clamp(obj.AngularVelocityX * 0.992, -340.0, 340.0);
            obj.AngularVelocityY = Clamp(obj.AngularVelocityY * 0.992, -420.0, 420.0);
            obj.AngularVelocityZ = Clamp(obj.AngularVelocityZ * 0.992, -280.0, 280.0);

            obj.RotationX += obj.AngularVelocityX * dt;
            obj.RotationY += obj.AngularVelocityY * dt;
            obj.RotationZ += obj.AngularVelocityZ * dt;
        }
    }

    private static double Clamp(double value, double min, double max)
    {
        return Math.Max(min, Math.Min(max, value));
    }
}
