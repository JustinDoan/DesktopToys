using System;
using ScreenOverlayPhysics.Models;
using ScreenOverlayPhysics.Physics;

namespace ScreenOverlayPhysics;

internal static class Program
{
    private static int Main()
    {
        Console.WriteLine("ScreenOverlayPhysics");
        Console.WriteLine("Running the shared simulation core on a non-Windows host.");
        Console.WriteLine("The desktop overlay UI still remains on the Windows-specific WPF path.");
        Console.WriteLine();

        var world = new PhysicsWorld(new Vector2(0f, 1800f));
        var bounds = new RectF(0f, 0f, 1280f, 720f);

        world.Add(CreateObject(new Vector2(360f, 40f), AppColor.FromRgb(127, 202, 255), ObjectVisualKind.Cube));
        world.Add(CreateObject(new Vector2(540f, 24f), AppColor.FromRgb(108, 241, 255), ObjectVisualKind.Crystal));
        world.Add(CreateObject(new Vector2(720f, 12f), AppColor.FromRgb(245, 245, 240), ObjectVisualKind.Dice));

        const float dt = 1f / 60f;
        const int steps = 240;

        for (var step = 0; step < steps; step++)
        {
            world.Step(dt, bounds, sleepThreshold: 24f, floorSnapThreshold: 3f);

            if ((step + 1) % 60 != 0)
            {
                continue;
            }

            Console.WriteLine($"After {step + 1} frames:");
            for (var i = 0; i < world.Objects.Count; i++)
            {
                var obj = world.Objects[i];
                Console.WriteLine(
                    $"  {obj.VisualKind,-8} pos=({obj.Body.Position.X,7:0.0}, {obj.Body.Position.Y,7:0.0}) vel=({obj.Body.Velocity.X,7:0.0}, {obj.Body.Velocity.Y,7:0.0})");
            }

            Console.WriteLine();
        }

        Console.WriteLine("Simulation completed.");
        return 0;
    }

    private static ObjectState CreateObject(Vector2 position, AppColor color, ObjectVisualKind visualKind)
    {
        return new ObjectState
        {
            BaseColor = color,
            VisualKind = visualKind,
            ZIndex = 1,
            Body =
            {
                Width = 84f,
                Height = 84f,
                Position = position,
                Mass = 1f,
                Restitution = 0.75f,
                LinearDamping = 0.992f,
                GravityScale = visualKind == ObjectVisualKind.Satellite ? 0f : 1f,
                Shape = visualKind == ObjectVisualKind.Crystal ? CollisionShape.Circle : CollisionShape.Box,
                CollisionScale = visualKind == ObjectVisualKind.Crystal ? 0.82f : 1f
            }
        };
    }
}
