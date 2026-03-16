using System;
using System.Collections.Generic;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Physics;

public static class CollisionSolver
{
    public static void SolveObjectCollisions(IReadOnlyList<ObjectState> objects)
    {
        for (var i = 0; i < objects.Count; i++)
        {
            for (var j = i + 1; j < objects.Count; j++)
            {
                ResolveObjectPair(objects[i], objects[j]);
            }
        }
    }

    public static void SolveScreenBounds(PhysicsBody body, in RectF bounds, float sleepThreshold, float floorSnapThreshold)
    {
        var hitX = false;
        var hitY = false;

        if (body.Position.X < bounds.Left)
        {
            body.Position.X = bounds.Left;
            hitX = true;
        }
        else if (body.Position.X + body.Width > bounds.Right)
        {
            body.Position.X = bounds.Right - body.Width;
            hitX = true;
        }

        if (body.Position.Y < bounds.Top)
        {
            body.Position.Y = bounds.Top;
            hitY = true;
        }
        else if (body.Position.Y + body.Height > bounds.Bottom)
        {
            body.Position.Y = bounds.Bottom - body.Height;
            hitY = true;
        }

        if (hitX)
        {
            body.Velocity.X = -body.Velocity.X * body.Restitution;
        }

        if (hitY)
        {
            body.Velocity.Y = -body.Velocity.Y * body.Restitution;
        }

        if (body.Position.Y + body.Height >= bounds.Bottom - floorSnapThreshold && MathF.Abs(body.Velocity.Y) < sleepThreshold)
        {
            body.Velocity.Y = 0f;
        }
    }

    private static void ResolveObjectPair(ObjectState left, ObjectState right)
    {
        var leftBody = left.Body;
        var rightBody = right.Body;

        if (leftBody.IsDragging && rightBody.IsDragging)
        {
            return;
        }

        var leftCenterX = leftBody.Position.X + (leftBody.Width * 0.5f);
        var leftCenterY = leftBody.Position.Y + (leftBody.Height * 0.5f);
        var rightCenterX = rightBody.Position.X + (rightBody.Width * 0.5f);
        var rightCenterY = rightBody.Position.Y + (rightBody.Height * 0.5f);

        var deltaX = rightCenterX - leftCenterX;
        var deltaY = rightCenterY - leftCenterY;
        var overlapX = (leftBody.Width * 0.5f) + (rightBody.Width * 0.5f) - MathF.Abs(deltaX);
        var overlapY = (leftBody.Height * 0.5f) + (rightBody.Height * 0.5f) - MathF.Abs(deltaY);
        if (overlapX <= 0f || overlapY <= 0f)
        {
            return;
        }

        var leftInverseMass = leftBody.IsDragging ? 0f : InverseMass(leftBody);
        var rightInverseMass = rightBody.IsDragging ? 0f : InverseMass(rightBody);
        var inverseMassSum = leftInverseMass + rightInverseMass;
        if (inverseMassSum <= 0f)
        {
            return;
        }

        float normalX;
        float normalY;
        float penetration;

        if (overlapX < overlapY)
        {
            normalX = deltaX >= 0f ? 1f : -1f;
            normalY = 0f;
            penetration = overlapX;
        }
        else
        {
            normalX = 0f;
            normalY = deltaY >= 0f ? 1f : -1f;
            penetration = overlapY;
        }

        var correctionX = normalX * penetration;
        var correctionY = normalY * penetration;
        leftBody.Position.X -= correctionX * (leftInverseMass / inverseMassSum);
        leftBody.Position.Y -= correctionY * (leftInverseMass / inverseMassSum);
        rightBody.Position.X += correctionX * (rightInverseMass / inverseMassSum);
        rightBody.Position.Y += correctionY * (rightInverseMass / inverseMassSum);

        var relativeVelocityX = rightBody.Velocity.X - leftBody.Velocity.X;
        var relativeVelocityY = rightBody.Velocity.Y - leftBody.Velocity.Y;
        var velocityAlongNormal = (relativeVelocityX * normalX) + (relativeVelocityY * normalY);
        if (velocityAlongNormal > 0f)
        {
            return;
        }

        var restitution = MathF.Min(leftBody.Restitution, rightBody.Restitution);
        var impulseMagnitude = -(1f + restitution) * velocityAlongNormal / inverseMassSum;
        var impulseX = impulseMagnitude * normalX;
        var impulseY = impulseMagnitude * normalY;

        leftBody.Velocity.X -= impulseX * leftInverseMass;
        leftBody.Velocity.Y -= impulseY * leftInverseMass;
        rightBody.Velocity.X += impulseX * rightInverseMass;
        rightBody.Velocity.Y += impulseY * rightInverseMass;

        left.AngularVelocityZ += impulseX * 0.015f;
        right.AngularVelocityZ -= impulseX * 0.015f;
    }

    private static float InverseMass(PhysicsBody body)
    {
        return body.Mass <= 0.0001f ? 1f : 1f / body.Mass;
    }
}
