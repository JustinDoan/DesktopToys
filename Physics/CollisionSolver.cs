using System;
using System.Collections.Generic;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Physics;

public static class CollisionSolver
{
    private const float PenetrationSlop = 0.75f;
    private const float PositionCorrectionPercent = 0.8f;
    private const float WakeImpulseThreshold = 75f;
    private const float WakeVelocityThreshold = 95f;

    public static void SolveObjectCollisions(IReadOnlyList<ObjectState> objects, IReadOnlyList<CollisionPair>? pairs = null)
    {
        if (pairs is null)
        {
            for (var i = 0; i < objects.Count; i++)
            {
                for (var j = i + 1; j < objects.Count; j++)
                {
                    ResolveObjectPair(objects[i], objects[j]);
                }
            }

            return;
        }

        for (var i = 0; i < pairs.Count; i++)
        {
            var pair = pairs[i];
            ResolveObjectPair(objects[pair.LeftIndex], objects[pair.RightIndex]);
        }
    }

    public static void SolveScreenBounds(PhysicsBody body, in RectF bounds, float sleepThreshold, float floorSnapThreshold)
    {
        var hitX = false;
        var hitY = false;
        var insetX = (body.Width * 0.5f) - EffectiveHalfWidth(body);
        var insetY = (body.Height * 0.5f) - EffectiveHalfHeight(body);
        var minX = bounds.Left - insetX;
        var maxX = bounds.Right - body.Width + insetX;
        var minY = bounds.Top - insetY;
        var maxY = bounds.Bottom - body.Height + insetY;

        if (body.Position.X < minX)
        {
            body.Position.X = minX;
            hitX = true;
        }
        else if (body.Position.X > maxX)
        {
            body.Position.X = maxX;
            hitX = true;
        }

        if (body.Position.Y < minY)
        {
            body.Position.Y = minY;
            hitY = true;
        }
        else if (body.Position.Y > maxY)
        {
            body.Position.Y = maxY;
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

        var effectiveBottom = body.Position.Y + body.Height - insetY;
        if (effectiveBottom >= bounds.Bottom - floorSnapThreshold && MathF.Abs(body.Velocity.Y) < sleepThreshold)
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

        if (leftBody.IsSleeping && rightBody.IsSleeping)
        {
            return;
        }

        if (leftBody.Shape == CollisionShape.Circle && rightBody.Shape == CollisionShape.Circle)
        {
            ResolveCirclePair(left, right);
            return;
        }

        if (leftBody.Shape == CollisionShape.Circle || rightBody.Shape == CollisionShape.Circle)
        {
            ResolveCircleBoxPair(left, right);
            return;
        }

        var leftCenterX = leftBody.Position.X + (leftBody.Width * 0.5f);
        var leftCenterY = leftBody.Position.Y + (leftBody.Height * 0.5f);
        var rightCenterX = rightBody.Position.X + (rightBody.Width * 0.5f);
        var rightCenterY = rightBody.Position.Y + (rightBody.Height * 0.5f);

        var deltaX = rightCenterX - leftCenterX;
        var deltaY = rightCenterY - leftCenterY;
        var overlapX = EffectiveHalfWidth(leftBody) + EffectiveHalfWidth(rightBody) - MathF.Abs(deltaX);
        var overlapY = EffectiveHalfHeight(leftBody) + EffectiveHalfHeight(rightBody) - MathF.Abs(deltaY);
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
        ApplyPositionCorrection(leftBody, rightBody, correctionX, correctionY, leftInverseMass, rightInverseMass, inverseMassSum);

        ApplyCollisionImpulse(left, right, normalX, normalY, leftInverseMass, rightInverseMass, inverseMassSum);
    }

    private static void ResolveCirclePair(ObjectState left, ObjectState right)
    {
        var leftBody = left.Body;
        var rightBody = right.Body;
        var leftCenterX = leftBody.Position.X + (leftBody.Width * 0.5f);
        var leftCenterY = leftBody.Position.Y + (leftBody.Height * 0.5f);
        var rightCenterX = rightBody.Position.X + (rightBody.Width * 0.5f);
        var rightCenterY = rightBody.Position.Y + (rightBody.Height * 0.5f);
        var deltaX = rightCenterX - leftCenterX;
        var deltaY = rightCenterY - leftCenterY;
        var distanceSquared = (deltaX * deltaX) + (deltaY * deltaY);
        var leftRadius = EffectiveRadius(leftBody);
        var rightRadius = EffectiveRadius(rightBody);
        var radiusSum = leftRadius + rightRadius;
        if (distanceSquared >= radiusSum * radiusSum)
        {
            return;
        }

        var distance = MathF.Sqrt(MathF.Max(distanceSquared, 0.0001f));
        var normalX = distance > 0.0001f ? deltaX / distance : 1f;
        var normalY = distance > 0.0001f ? deltaY / distance : 0f;
        var penetration = radiusSum - distance;

        var leftInverseMass = leftBody.IsDragging ? 0f : InverseMass(leftBody);
        var rightInverseMass = rightBody.IsDragging ? 0f : InverseMass(rightBody);
        var inverseMassSum = leftInverseMass + rightInverseMass;
        if (inverseMassSum <= 0f)
        {
            return;
        }

        ApplyPositionCorrection(
            leftBody,
            rightBody,
            normalX * penetration,
            normalY * penetration,
            leftInverseMass,
            rightInverseMass,
            inverseMassSum);

        ApplyCollisionImpulse(left, right, normalX, normalY, leftInverseMass, rightInverseMass, inverseMassSum);
    }

    private static void ResolveCircleBoxPair(ObjectState left, ObjectState right)
    {
        var leftIsCircle = left.Body.Shape == CollisionShape.Circle;
        var circleBody = leftIsCircle ? left.Body : right.Body;
        var boxBody = leftIsCircle ? right.Body : left.Body;

        if (!TryGetCircleBoxContact(circleBody, boxBody, out var circleToBoxNormalX, out var circleToBoxNormalY, out var penetration))
        {
            return;
        }

        var normalX = leftIsCircle ? circleToBoxNormalX : -circleToBoxNormalX;
        var normalY = leftIsCircle ? circleToBoxNormalY : -circleToBoxNormalY;
        var leftInverseMass = left.Body.IsDragging ? 0f : InverseMass(left.Body);
        var rightInverseMass = right.Body.IsDragging ? 0f : InverseMass(right.Body);
        var inverseMassSum = leftInverseMass + rightInverseMass;
        if (inverseMassSum <= 0f)
        {
            return;
        }

        ApplyPositionCorrection(
            left.Body,
            right.Body,
            normalX * penetration,
            normalY * penetration,
            leftInverseMass,
            rightInverseMass,
            inverseMassSum);

        ApplyCollisionImpulse(left, right, normalX, normalY, leftInverseMass, rightInverseMass, inverseMassSum);
    }

    private static bool TryGetCircleBoxContact(
        PhysicsBody circleBody,
        PhysicsBody boxBody,
        out float normalX,
        out float normalY,
        out float penetration)
    {
        var circleCenterX = circleBody.Position.X + (circleBody.Width * 0.5f);
        var circleCenterY = circleBody.Position.Y + (circleBody.Height * 0.5f);
        var radius = EffectiveRadius(circleBody);
        var boxInsetX = (boxBody.Width * 0.5f) - EffectiveHalfWidth(boxBody);
        var boxInsetY = (boxBody.Height * 0.5f) - EffectiveHalfHeight(boxBody);
        var boxLeft = boxBody.Position.X + boxInsetX;
        var boxTop = boxBody.Position.Y + boxInsetY;
        var boxRight = boxBody.Position.X + boxBody.Width - boxInsetX;
        var boxBottom = boxBody.Position.Y + boxBody.Height - boxInsetY;
        var closestX = Math.Clamp(circleCenterX, boxLeft, boxRight);
        var closestY = Math.Clamp(circleCenterY, boxTop, boxBottom);
        var deltaX = closestX - circleCenterX;
        var deltaY = closestY - circleCenterY;
        var distanceSquared = (deltaX * deltaX) + (deltaY * deltaY);
        if (distanceSquared > radius * radius)
        {
            normalX = 0f;
            normalY = 0f;
            penetration = 0f;
            return false;
        }

        if (distanceSquared > 0.0001f)
        {
            var distance = MathF.Sqrt(distanceSquared);
            normalX = deltaX / distance;
            normalY = deltaY / distance;
            penetration = radius - distance;
            return true;
        }

        var toLeft = circleCenterX - boxLeft;
        var toRight = boxRight - circleCenterX;
        var toTop = circleCenterY - boxTop;
        var toBottom = boxBottom - circleCenterY;
        var minDistance = MathF.Min(MathF.Min(toLeft, toRight), MathF.Min(toTop, toBottom));

        if (minDistance == toLeft)
        {
            normalX = 1f;
            normalY = 0f;
            penetration = radius + toLeft;
        }
        else if (minDistance == toRight)
        {
            normalX = -1f;
            normalY = 0f;
            penetration = radius + toRight;
        }
        else if (minDistance == toTop)
        {
            normalX = 0f;
            normalY = 1f;
            penetration = radius + toTop;
        }
        else
        {
            normalX = 0f;
            normalY = -1f;
            penetration = radius + toBottom;
        }

        return true;
    }

    private static void ApplyCollisionImpulse(
        ObjectState left,
        ObjectState right,
        float normalX,
        float normalY,
        float leftInverseMass,
        float rightInverseMass,
        float inverseMassSum)
    {
        var leftBody = left.Body;
        var rightBody = right.Body;
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

        if (impulseMagnitude > WakeImpulseThreshold || MathF.Abs(velocityAlongNormal) > WakeVelocityThreshold)
        {
            WakeBody(leftBody);
            WakeBody(rightBody);
        }
    }

    private static void ApplyPositionCorrection(
        PhysicsBody leftBody,
        PhysicsBody rightBody,
        float correctionX,
        float correctionY,
        float leftInverseMass,
        float rightInverseMass,
        float inverseMassSum)
    {
        var correctionMagnitude = MathF.Sqrt((correctionX * correctionX) + (correctionY * correctionY));
        var correctedMagnitude = MathF.Max(correctionMagnitude - PenetrationSlop, 0f) * PositionCorrectionPercent;
        if (correctedMagnitude <= 0f || correctionMagnitude <= 0.0001f)
        {
            return;
        }

        var scale = correctedMagnitude / correctionMagnitude;
        var scaledCorrectionX = correctionX * scale;
        var scaledCorrectionY = correctionY * scale;

        leftBody.Position.X -= scaledCorrectionX * (leftInverseMass / inverseMassSum);
        leftBody.Position.Y -= scaledCorrectionY * (leftInverseMass / inverseMassSum);
        rightBody.Position.X += scaledCorrectionX * (rightInverseMass / inverseMassSum);
        rightBody.Position.Y += scaledCorrectionY * (rightInverseMass / inverseMassSum);
    }

    private static void WakeBody(PhysicsBody body)
    {
        body.IsSleeping = false;
        body.SleepTimerSeconds = 0f;
    }

    private static float InverseMass(PhysicsBody body)
    {
        if (body.IsSleeping)
        {
            return 0f;
        }

        return body.Mass <= 0.0001f ? 1f : 1f / body.Mass;
    }

    private static float EffectiveHalfWidth(PhysicsBody body)
    {
        return body.Width * 0.5f * body.CollisionScale;
    }

    private static float EffectiveHalfHeight(PhysicsBody body)
    {
        return body.Height * 0.5f * body.CollisionScale;
    }

    private static float EffectiveRadius(PhysicsBody body)
    {
        return MathF.Min(body.Width, body.Height) * 0.5f * body.CollisionScale;
    }
}
