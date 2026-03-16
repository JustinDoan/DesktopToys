using System.Windows;
using ScreenOverlayPhysics.Core;

namespace ScreenOverlayPhysics;

public partial class App : System.Windows.Application
{
    private readonly ConsoleLifetimeService _consoleLifetime = new();

    protected override void OnStartup(StartupEventArgs e)
    {
        _consoleLifetime.Start(this);
        base.OnStartup(e);
    }
}
