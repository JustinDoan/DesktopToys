using System.Collections.Generic;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Input;

public sealed class HitTester
{
    public ObjectState? HitTestTopmost(IReadOnlyList<ObjectState> objects, Vector2 point)
    {
        ObjectState? best = null;
        var bestZ = int.MinValue;

        for (var i = 0; i < objects.Count; i++)
        {
            var candidate = objects[i];
            if (!ContainsPoint(candidate.Body, point))
            {
                continue;
            }

            if (candidate.ZIndex >= bestZ)
            {
                bestZ = candidate.ZIndex;
                best = candidate;
            }
        }

        return best;
    }

    public bool IsPointOverAnyObject(IReadOnlyList<ObjectState> objects, Vector2 point)
    {
        for (var i = 0; i < objects.Count; i++)
        {
            if (ContainsPoint(objects[i].Body, point))
            {
                return true;
            }
        }

        return false;
    }

    private static bool ContainsPoint(Physics.PhysicsBody body, Vector2 point)
    {
        if (body.Shape == Physics.CollisionShape.Circle)
        {
            var radius = System.MathF.Min(body.Width, body.Height) * 0.5f * body.CollisionScale;
            var centerX = body.Position.X + (body.Width * 0.5f);
            var centerY = body.Position.Y + (body.Height * 0.5f);
            var deltaX = point.X - centerX;
            var deltaY = point.Y - centerY;
            return (deltaX * deltaX) + (deltaY * deltaY) <= radius * radius;
        }

        var rect = new RectF(body.Position.X, body.Position.Y, body.Width, body.Height);
        return rect.Contains(point);
    }
}
