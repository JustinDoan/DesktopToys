using System;
using System.Collections.Generic;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using ScreenOverlayPhysics.Core;
using ScreenOverlayPhysics.Input;
using ScreenOverlayPhysics.Models;
using ScreenOverlayPhysics.Physics;
using ScreenOverlayPhysics.Rendering;
using ScreenOverlayPhysics.Scene;
using Forms = System.Windows.Forms;
using System.Text;

namespace ScreenOverlayPhysics;

public partial class MainWindow : Window
{
    private readonly AppConfig _config = new();
    private readonly FrameClock _frameClock = new();
    private readonly DisplayBoundsService _displayBoundsService = new();
    private readonly HitTester _hitTester = new();
    private readonly MouseTracker _mouseTracker = new(10);

    private readonly DragController _dragController;
    private readonly SceneRenderer _sceneRenderer;
    private readonly PhysicsWorld _physicsWorld;
    private readonly SceneController _sceneController;
    private readonly DebugOverlayRenderer _debugOverlayRenderer;

    private OverlayWindowService? _overlayWindowService;
    private RectF _screenBounds;
    private Vector2 _cursorLocal;
    private ObjectState? _selectedObject;

    private OverlayInputMode _pendingMode = OverlayInputMode.Interactive;
    private double _modeCandidateSinceSeconds;
    private bool _isInitialized;
    private bool _wasLeftMouseDown;
    private bool _forceInteractiveForDebug;
    private bool _debugHitPrimaryCursor;
    private bool _debugLeftDown;
    private string _lastDragAttempt = "none";
    private readonly StringBuilder _debugBuffer = new(256);

    private readonly Forms.NotifyIcon _trayIcon;

    public MainWindow()
    {
        InitializeComponent();
        Focusable = true;

        _screenBounds = _displayBoundsService.CurrentPrimaryBounds;
        _physicsWorld = new PhysicsWorld(new Vector2(0f, _config.GravityY));
        _sceneRenderer = new SceneRenderer(RootViewport);
        _sceneController = new SceneController(_config, _physicsWorld, _sceneRenderer);
        _dragController = new DragController(_mouseTracker, HitTestObject);
        _debugOverlayRenderer = new DebugOverlayRenderer(DebugPanel, DebugText);

        _displayBoundsService.BoundsChanged += OnDisplayBoundsChanged;
        _trayIcon = BuildTrayIcon();
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        if (_isInitialized)
        {
            return;
        }

        _overlayWindowService = new OverlayWindowService(this);
        _overlayWindowService.ApplyWindowBounds(_screenBounds);
        _overlayWindowService.ApplyOverlayStyles();
        _overlayWindowService.SetInputMode(_config.StartInPassThrough ? OverlayInputMode.PassThrough : OverlayInputMode.Interactive);

        CompositionTarget.Rendering += OnRendering;

        _sceneController.Initialize(_screenBounds);
        _selectedObject = _sceneController.Objects.Count > 0 ? _sceneController.Objects[^1] : null;
        _isInitialized = true;
    }

    private void OnClosed(object? sender, EventArgs e)
    {
        CompositionTarget.Rendering -= OnRendering;
        _displayBoundsService.Dispose();
        _trayIcon.Visible = false;
        _trayIcon.Dispose();
    }

    private void OnRendering(object? sender, EventArgs e)
    {
        if (_overlayWindowService is null)
        {
            return;
        }

        InputSurface.Width = _screenBounds.Width;
        InputSurface.Height = _screenBounds.Height;

        _frameClock.Tick();
        var dt = _frameClock.DeltaTimeSeconds;
        var now = _frameClock.ElapsedSeconds;

        _sceneController.SetGravity(_config.GravityY);
        UpdateCursorPosition(now);
        UpdateClickThroughMode(now);
        HandleGlobalMouseButtons(now);

        if (_dragController.IsDragging)
        {
            _dragController.UpdateDrag(_cursorLocal, now);
        }

        _sceneController.Step(dt, _screenBounds);
        _sceneController.Render(_screenBounds);
        _debugOverlayRenderer.Render(_frameClock, _selectedObject, _overlayWindowService.CurrentMode, BuildDebugDiagnostics());
    }

    private void UpdateCursorPosition(double nowSeconds)
    {
        if (Win32Interop.GetCursorPos(out var cursor))
        {
            var cursorDip = PointFromScreen(new Point(cursor.X, cursor.Y));
            _cursorLocal = new Vector2((float)cursorDip.X, (float)cursorDip.Y);

            _mouseTracker.AddSample(_cursorLocal, nowSeconds);
        }
    }

    private void UpdateClickThroughMode(double nowSeconds)
    {
        if (_overlayWindowService is null)
        {
            return;
        }

        _debugHitPrimaryCursor = IsPointOverAnyObject(_cursorLocal);
        var shouldBeInteractive = _forceInteractiveForDebug || _dragController.IsDragging || _debugHitPrimaryCursor;
        var desiredMode = shouldBeInteractive ? OverlayInputMode.Interactive : OverlayInputMode.PassThrough;

        if (desiredMode != _pendingMode)
        {
            _pendingMode = desiredMode;
            _modeCandidateSinceSeconds = nowSeconds;
        }

        var debounceSeconds = _config.InteractionDebounceMs / 1000d;
        if (desiredMode == OverlayInputMode.Interactive)
        {
            debounceSeconds = Math.Min(debounceSeconds, 0.02d);
        }

        if (nowSeconds - _modeCandidateSinceSeconds >= debounceSeconds)
        {
            _overlayWindowService.SetInputMode(_pendingMode);
        }
    }

    private void OnMouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (_overlayWindowService is null)
        {
            return;
        }

        _overlayWindowService.SetInputMode(OverlayInputMode.Interactive);
        Focus();
        CaptureMouse();

        var cursor = e.GetPosition(InputSurface);
        _cursorLocal = new Vector2((float)cursor.X, (float)cursor.Y);
        var started = !_dragController.IsDragging &&
                      _dragController.BeginDrag(_physicsWorld.Objects, _cursorLocal, _frameClock.ElapsedSeconds);
        if (started)
        {
            _selectedObject = _dragController.DraggedObject;
            e.Handled = true;
        }
    }

    private void OnMouseMove(object sender, System.Windows.Input.MouseEventArgs e)
    {
        var cursor = e.GetPosition(InputSurface);
        _cursorLocal = new Vector2((float)cursor.X, (float)cursor.Y);
    }

    private void OnMouseLeftButtonUp(object sender, MouseButtonEventArgs e)
    {
        if (!_dragController.IsDragging)
        {
            ReleaseMouseCapture();
            return;
        }

        EndDragAndApplySpin();
        ReleaseMouseCapture();
        e.Handled = true;
    }

    private void HandleGlobalMouseButtons(double nowSeconds)
    {
        if (_overlayWindowService is null)
        {
            return;
        }

        var isLeftDown = Win32Interop.IsLeftMouseButtonDown();
        _debugLeftDown = isLeftDown;
        if (isLeftDown && !_wasLeftMouseDown)
        {
            _overlayWindowService.SetInputMode(OverlayInputMode.Interactive);
            var began = _dragController.BeginDrag(_physicsWorld.Objects, _cursorLocal, nowSeconds);
            _lastDragAttempt = began ? "begin:primary" : "miss:primary";

            if (began)
            {
                _selectedObject = _dragController.DraggedObject;
            }
        }
        else if (!isLeftDown && _wasLeftMouseDown && _dragController.IsDragging)
        {
            EndDragAndApplySpin();
            ReleaseMouseCapture();
            _lastDragAttempt = "release";
        }

        _wasLeftMouseDown = isLeftDown;
    }

    private void EndDragAndApplySpin()
    {
        var throwVelocity = _dragController.EndDrag(_frameClock.ElapsedSeconds, _config.ThrowSensitivity, _config.MaxThrowSpeed);
        if (_selectedObject is null)
        {
            return;
        }

        _selectedObject.AngularVelocityY += throwVelocity.X * 0.22;
        _selectedObject.AngularVelocityX += throwVelocity.Y * 0.16;
        _selectedObject.AngularVelocityZ += throwVelocity.X * 0.08;
    }

    private ObjectState? HitTestObject(IReadOnlyList<ObjectState> objects, Vector2 cursor)
    {
        var viewportHit = _sceneRenderer.HitTest(objects, new Point(cursor.X, cursor.Y));
        return viewportHit ?? _hitTester.HitTestTopmost(objects, cursor);
    }

    private bool IsPointOverAnyObject(Vector2 cursor)
    {
        return HitTestObject(_physicsWorld.Objects, cursor) is not null;
    }

    private void OnKeyDown(object sender, System.Windows.Input.KeyEventArgs e)
    {
        switch (e.Key)
        {
            case Key.F1:
                _debugOverlayRenderer.Toggle();
                break;
            case Key.F2:
                _selectedObject = _sceneController.SpawnCube(new Vector2(_screenBounds.Width * 0.5f, 40f));
                break;
            case Key.F3:
                ResetObjects();
                break;
            case Key.F4:
                OpenSettingsPanel();
                break;
            case Key.Escape:
                Close();
                break;
            case Key.F5:
                _forceInteractiveForDebug = !_forceInteractiveForDebug;
                _lastDragAttempt = _forceInteractiveForDebug ? "force-input:on" : "force-input:off";
                break;
        }
    }

    private void ResetObjects()
    {
        _sceneController.Reset(_screenBounds);
        _selectedObject = _sceneController.Objects.Count > 0 ? _sceneController.Objects[^1] : null;
    }

    private void OnDisplayBoundsChanged(RectF bounds)
    {
        Dispatcher.Invoke(() =>
        {
            _screenBounds = bounds;
            _overlayWindowService?.ApplyWindowBounds(bounds);
            _overlayWindowService?.EnsureTopmost();
        });
    }

    private Forms.NotifyIcon BuildTrayIcon()
    {
        var menu = new Forms.ContextMenuStrip();
        menu.Items.Add("Toggle Debug (F1)", null, (_, _) => _debugOverlayRenderer.Toggle());
        menu.Items.Add("Spawn Cube (F2)", null, (_, _) => _selectedObject = _sceneController.SpawnCube(new Vector2(_screenBounds.Width * 0.5f, 40f)));
        menu.Items.Add("Reset (F3)", null, (_, _) => ResetObjects());
        menu.Items.Add("Settings (F4)", null, (_, _) => OpenSettingsPanel());
        menu.Items.Add("-");
        menu.Items.Add("Exit", null, (_, _) => Close());

        var icon = new Forms.NotifyIcon
        {
            Icon = System.Drawing.SystemIcons.Application,
            Text = "ScreenOverlayPhysics",
            Visible = true,
            ContextMenuStrip = menu
        };

        icon.DoubleClick += (_, _) => OpenSettingsPanel();
        return icon;
    }

    private void OpenSettingsPanel()
    {
        var settingsWindow = new SettingsWindow(_config)
        {
            Owner = this
        };

        if (settingsWindow.ShowDialog() == true)
        {
            _sceneController.ApplyRuntimePhysicsConfig();
        }
    }

    private string BuildDebugDiagnostics()
    {
        _debugBuffer.Clear();
        _debugBuffer.AppendFormat("ForceInteractive(F5): {0}\n", _forceInteractiveForDebug ? "ON" : "off");
        _debugBuffer.AppendFormat("Cursor: ({0:0.0},{1:0.0}) hit={2}\n", _cursorLocal.X, _cursorLocal.Y, _debugHitPrimaryCursor ? "yes" : "no");
        _debugBuffer.AppendFormat("LMB: {0} WasDown: {1}\n", _debugLeftDown ? "down" : "up", _wasLeftMouseDown ? "yes" : "no");
        _debugBuffer.AppendFormat("DragAttempt: {0}\n", _lastDragAttempt);
        _debugBuffer.AppendFormat("PendingMode: {0}", _pendingMode);
        return _debugBuffer.ToString();
    }
}