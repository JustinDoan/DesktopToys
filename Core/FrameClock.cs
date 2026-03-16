using System;
using System.Diagnostics;

namespace ScreenOverlayPhysics.Core;

public sealed class FrameClock
{
    private readonly Stopwatch _stopwatch = Stopwatch.StartNew();
    private double _lastSeconds;
    private double _fpsTimer;
    private int _fpsFrames;

    public float DeltaTimeSeconds { get; private set; }
    public float FrameTimeMs { get; private set; }
    public float Fps { get; private set; }
    public double ElapsedSeconds => _stopwatch.Elapsed.TotalSeconds;

    public void Tick()
    {
        var now = _stopwatch.Elapsed.TotalSeconds;
        var rawDt = now - _lastSeconds;
        _lastSeconds = now;

        var clamped = Math.Min(rawDt, 1.0d / 30.0d);
        DeltaTimeSeconds = (float)clamped;
        FrameTimeMs = (float)(clamped * 1000.0d);

        _fpsTimer += clamped;
        _fpsFrames++;
        if (_fpsTimer >= 0.5d)
        {
            Fps = (float)(_fpsFrames / _fpsTimer);
            _fpsFrames = 0;
            _fpsTimer = 0d;
        }
    }
}
