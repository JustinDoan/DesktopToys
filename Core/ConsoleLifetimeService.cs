using System;
using System.Windows;

namespace ScreenOverlayPhysics.Core;

public sealed class ConsoleLifetimeService : IDisposable
{
    private bool _attachedToParentConsole;
    private bool _disposed;

    public void Start(Application application)
    {
        if (_disposed)
        {
            throw new ObjectDisposedException(nameof(ConsoleLifetimeService));
        }

        _attachedToParentConsole = Win32Interop.AttachConsole(Win32Interop.AttachParentProcess);
        if (!_attachedToParentConsole)
        {
            return;
        }

        Console.CancelKeyPress += OnCancelKeyPress;
        application.Exit += OnApplicationExit;
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        Console.CancelKeyPress -= OnCancelKeyPress;
        if (_attachedToParentConsole)
        {
            Win32Interop.FreeConsole();
        }

        _disposed = true;
    }

    private void OnCancelKeyPress(object? sender, ConsoleCancelEventArgs e)
    {
        e.Cancel = true;
        CurrentApplication()?.Dispatcher.BeginInvoke(new Action(() => CurrentApplication()?.Shutdown()));
    }

    private void OnApplicationExit(object? sender, ExitEventArgs e)
    {
        if (sender is Application application)
        {
            application.Exit -= OnApplicationExit;
        }

        Dispose();
    }

    private static Application? CurrentApplication()
    {
        return Application.Current;
    }
}
