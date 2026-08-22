$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$NativeRoot = Join-Path $RepoRoot "native"
$ControlRoot = Join-Path $RepoRoot "control-ui"
$SidecarDir = Join-Path $ControlRoot "src-tauri\binaries"
$TargetTriple = "x86_64-pc-windows-msvc"
$RendererExe = Join-Path $NativeRoot "target\release\app.exe"
$RendererManifest = Join-Path $NativeRoot "app\windows.manifest"
$SidecarExe = Join-Path $SidecarDir "screen-overlay-renderer-$TargetTriple.exe"
$CaseConfig = Join-Path $RepoRoot "Assets\case\rewards.json"
$ResourceDir = Join-Path $ControlRoot "src-tauri\resources"
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

Write-Host "Embedding renderer Windows manifest..."
$WindowsKitsBin = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
$ManifestTool = Get-ChildItem -Path $WindowsKitsBin -Filter "mt.exe" -Recurse |
  Where-Object { $_.Directory.Name -eq "x64" } |
  Sort-Object FullName -Descending |
  Select-Object -First 1 -ExpandProperty FullName
if (!$ManifestTool) {
  throw "Windows Manifest Tool (mt.exe) was not found under $WindowsKitsBin"
}
& $ManifestTool -manifest $RendererManifest "-outputresource:$RendererExe;#1"
if ($LASTEXITCODE -ne 0) {
  throw "Embedding the renderer manifest failed with exit code $LASTEXITCODE"
}

Write-Host "Preparing Tauri sidecar..."
New-Item -ItemType Directory -Force -Path $SidecarDir | Out-Null
Copy-Item -LiteralPath $RendererExe -Destination $SidecarExe -Force

# The installer drops this beside the renderer, where it is picked up on launch.
# Editing the installed copy retunes the case without a rebuild.
Write-Host "Staging case rewards config..."
New-Item -ItemType Directory -Force -Path $ResourceDir | Out-Null
Copy-Item -LiteralPath $CaseConfig -Destination (Join-Path $ResourceDir "rewards.json") -Force

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
