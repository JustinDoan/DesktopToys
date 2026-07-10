$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$NativeRoot = Join-Path $RepoRoot "native"
$ControlRoot = Join-Path $RepoRoot "control-ui"
$SidecarDir = Join-Path $ControlRoot "src-tauri\binaries"
$TargetTriple = "x86_64-pc-windows-msvc"
$RendererExe = Join-Path $NativeRoot "target\release\app.exe"
$SidecarExe = Join-Path $SidecarDir "screen-overlay-renderer-$TargetTriple.exe"
$BundleDir = Join-Path $ControlRoot "src-tauri\target\release\bundle\nsis"

Write-Host "Building native renderer..."
Push-Location $NativeRoot
cargo build -p app --release
if ($LASTEXITCODE -ne 0) {
  throw "Native renderer release build failed with exit code $LASTEXITCODE"
}
Pop-Location

if (!(Test-Path $RendererExe)) {
  throw "Renderer build did not create $RendererExe"
}

Write-Host "Preparing Tauri sidecar..."
New-Item -ItemType Directory -Force -Path $SidecarDir | Out-Null
Copy-Item -LiteralPath $RendererExe -Destination $SidecarExe -Force

Write-Host "Building Tauri installer..."
Push-Location $ControlRoot
npx tauri build --config src-tauri/tauri.package.conf.json
if ($LASTEXITCODE -ne 0) {
  throw "Tauri installer build failed with exit code $LASTEXITCODE"
}
Pop-Location

Write-Host ""
Write-Host "Package output:"
if (Test-Path $BundleDir) {
  Get-ChildItem -Path $BundleDir -Filter "*.exe" | Select-Object -ExpandProperty FullName
} else {
  throw "No NSIS bundle directory found at $BundleDir"
}
