using System;
using System.Collections.Generic;
using ScreenOverlayPhysics.Models;

namespace ScreenOverlayPhysics.Input;

public sealed class DragController
{
    private readonly MouseTracker _mouseTracker;
    private readonly Func<IReadOnlyList<ObjectState>, Vector2, ObjectState?> _hitTestObject;

    private ObjectState? _dragged;
    private Vector2 _cursorOffset;

    public DragController(
        MouseTracker mouseTracker,
        Func<IReadOnlyList<ObjectState>, Vector2, ObjectState?> hitTestObject)
    {
        _mouseTracker = mouseTracker;
        _hitTestObject = hitTestObject;
    }

    public bool IsDragging => _dragged is not null;
    public ObjectState? DraggedObject => _dragged;

    public bool BeginDrag(IReadOnlyList<ObjectState> objects, Vector2 cursor, double nowSeconds)
    {
        if (_dragged is not null)
        {
            return true;
        }

        var hit = _hitTestObject(objects, cursor);
        if (hit is null)
        {
            return false;
        }

        _dragged = hit;
        hit.IsDragging = true;
        hit.Body.IsDragging = true;
        hit.Body.IsSleeping = false;
        hit.Body.SleepTimerSeconds = 0f;
        hit.Body.Velocity = Vector2.Zero;
        _cursorOffset = cursor - hit.Body.Position;

        _mouseTracker.Clear();
        _mouseTracker.AddSample(cursor, nowSeconds);
        return true;
    }

    public void UpdateDrag(Vector2 cursor, double nowSeconds)
    {
        if (_dragged is null)
        {
            return;
        }

        _mouseTracker.AddSample(cursor, nowSeconds);
        _dragged.Body.Position = cursor - _cursorOffset;
    }

    public Vector2 EndDrag(double nowSeconds, float throwSensitivity, float maxThrowSpeed)
    {
        if (_dragged is null)
        {
            return Vector2.Zero;
        }

        _mouseTracker.AddSample(_dragged.Body.Position + _cursorOffset, nowSeconds);
        var throwVelocity = _mouseTracker.EstimateVelocity(0.085d, throwSensitivity, maxThrowSpeed);

        _dragged.Body.IsDragging = false;
        _dragged.Body.IsSleeping = false;
        _dragged.Body.SleepTimerSeconds = 0f;
        _dragged.IsDragging = false;
        _dragged.Body.Velocity = throwVelocity;
        _dragged = null;
        _mouseTracker.Clear();

        return throwVelocity;
    }
}
