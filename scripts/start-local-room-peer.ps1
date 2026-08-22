param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('Host', 'Guest')]
    [string]$Role
)

$root = Split-Path -Parent $PSScriptRoot
$native = Join-Path $root 'native\target\release\app.exe'

if (-not (Test-Path -LiteralPath $native)) {
    throw "Native release build was not found: $native"
}

if ($Role -eq 'Guest') {
    $env:SCREEN_OVERLAY_ENGINE_IPC_ADDR = '127.0.0.1:47741'
    $env:SCREEN_OVERLAY_WINDOW_IPC_ADDR = '127.0.0.1:47742'
    $env:SCREEN_OVERLAY_ROOM_BRIDGE_ADDR = '127.0.0.1:47743'
} else {
    Remove-Item Env:SCREEN_OVERLAY_ENGINE_IPC_ADDR -ErrorAction SilentlyContinue
    Remove-Item Env:SCREEN_OVERLAY_WINDOW_IPC_ADDR -ErrorAction SilentlyContinue
    Remove-Item Env:SCREEN_OVERLAY_ROOM_BRIDGE_ADDR -ErrorAction SilentlyContinue
}

$env:SCREEN_OVERLAY_SHOW_CONTROL_UI = '1'
$env:SCREEN_OVERLAY_ROOM_TRACE_PATH = Join-Path $root ("room-{0}-trace.log" -f $Role.ToLowerInvariant())
Remove-Item -LiteralPath $env:SCREEN_OVERLAY_ROOM_TRACE_PATH -ErrorAction SilentlyContinue
Start-Process -FilePath $native -WorkingDirectory $root -WindowStyle Hidden

Write-Host "$Role peer started with its own control UI and localhost ports."
