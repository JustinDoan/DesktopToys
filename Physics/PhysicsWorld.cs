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

            if (obj.VisualKind == ObjectVisualKind.Dice)
            {
                ApplyDiceFaceSettling(obj, body, bounds, dt, sleepThreshold);
            }

            obj.RotationX += obj.AngularVelocityX * dt;
            obj.RotationY += obj.AngularVelocityY * dt;
            obj.RotationZ += obj.AngularVelocityZ * dt;
        }
    }

    private static void ApplyDiceFaceSettling(ObjectState obj, PhysicsBody body, in RectF bounds, float dt, float sleepThreshold)
    {
        var isNearFloor = body.Position.Y + body.Height >= bounds.Bottom - 2f;
        var horizontalSpeed = Math.Abs(body.Velocity.X);
        var verticalSpeed = Math.Abs(body.Velocity.Y);
        var angularSpeed =
            Math.Abs(obj.AngularVelocityX) +
            Math.Abs(obj.AngularVelocityY) +
            Math.Abs(obj.AngularVelocityZ);

        if (!isNearFloor || horizontalSpeed > sleepThreshold * 1.1f || verticalSpeed > sleepThreshold * 0.8f || angularSpeed > 260.0)
        {
            return;
        }

        var blend = Math.Min(1.0, dt * 7.5);
        var targetX = NearestQuarterTurn(obj.RotationX);
        var targetY = NearestQuarterTurn(obj.RotationY);
        var targetZ = NearestQuarterTurn(obj.RotationZ);

        obj.AngularVelocityX *= 0.82;
        obj.AngularVelocityY *= 0.82;
        obj.AngularVelocityZ *= 0.82;

        obj.RotationX += ShortestAngleDelta(obj.RotationX, targetX) * blend;
        obj.RotationY += ShortestAngleDelta(obj.RotationY, targetY) * blend;
        obj.RotationZ += ShortestAngleDelta(obj.RotationZ, targetZ) * blend;

        if (Math.Abs(ShortestAngleDelta(obj.RotationX, targetX)) < 0.75 &&
            Math.Abs(ShortestAngleDelta(obj.RotationY, targetY)) < 0.75 &&
            Math.Abs(ShortestAngleDelta(obj.RotationZ, targetZ)) < 0.75 &&
            angularSpeed < 42.0)
        {
            obj.RotationX = targetX;
            obj.RotationY = targetY;
            obj.RotationZ = targetZ;
            obj.AngularVelocityX = 0.0;
            obj.AngularVelocityY = 0.0;
            obj.AngularVelocityZ = 0.0;
        }
    }

    private static double Clamp(double value, double min, double max)
    {
        return Math.Max(min, Math.Min(max, value));
    }

    private static double NearestQuarterTurn(double angle)
    {
        return Math.Round(angle / 90.0) * 90.0;
    }

    private static double ShortestAngleDelta(double current, double target)
    {
        var delta = (target - current) % 360.0;
        if (delta > 180.0)
        {
            delta -= 360.0;
        }
        else if (delta < -180.0)
        {
            delta += 360.0;
        }

        return delta;
    }
}
