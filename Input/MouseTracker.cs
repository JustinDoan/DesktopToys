using System;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Input;

public sealed class MouseTracker
{
    private readonly MouseSample[] _samples;
    private int _nextIndex;
    private int _count;

    public MouseTracker(int capacity = 10)
    {
        _samples = new MouseSample[Math.Max(2, capacity)];
    }

    public void AddSample(Vector2 position, double timestampSeconds)
    {
        _samples[_nextIndex] = new MouseSample(position, timestampSeconds);
        _nextIndex = (_nextIndex + 1) % _samples.Length;
        _count = Math.Min(_count + 1, _samples.Length);
    }

    public void Clear()
    {
        _nextIndex = 0;
        _count = 0;
    }

    public Vector2 EstimateVelocity(double lookbackSeconds, float sensitivity, float maxSpeed)
    {
        if (_count < 2)
        {
            return Vector2.Zero;
        }

        var latestIndex = (_nextIndex - 1 + _samples.Length) % _samples.Length;
        var latest = _samples[latestIndex];
        var targetTime = latest.TimeSeconds - lookbackSeconds;
        var oldest = latest;

        for (var i = 1; i < _count; i++)
        {
            var idx = (latestIndex - i + _samples.Length) % _samples.Length;
            var sample = _samples[idx];
            oldest = sample;
            if (sample.TimeSeconds <= targetTime)
            {
                break;
            }
        }

        var elapsed = latest.TimeSeconds - oldest.TimeSeconds;
        if (elapsed < 0.0001d)
        {
            return Vector2.Zero;
        }

        var velocity = (latest.Position - oldest.Position) / (float)elapsed;
        velocity *= sensitivity;

        var speedSq = velocity.LengthSquared;
        var maxSq = maxSpeed * maxSpeed;
        if (speedSq > maxSq)
        {
            var scale = maxSpeed / MathF.Sqrt(speedSq);
            velocity *= scale;
        }

        return velocity;
    }

    private readonly record struct MouseSample(Vector2 Position, double TimeSeconds);
}
