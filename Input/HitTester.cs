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
            var rect = new RectF(candidate.Body.Position.X, candidate.Body.Position.Y, candidate.Body.Width, candidate.Body.Height);
            if (!rect.Contains(point))
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
            var body = objects[i].Body;
            if (point.X >= body.Position.X &&
                point.X <= body.Position.X + body.Width &&
                point.Y >= body.Position.Y &&
                point.Y <= body.Position.Y + body.Height)
            {
                return true;
            }
        }

        return false;
    }
}
