$ErrorActionPreference = "Stop"

$projectRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$package = Get-Content -LiteralPath (Join-Path $projectRoot "package.json") -Raw -Encoding UTF8 | ConvertFrom-Json
$version = $package.version
$targetTriple = if ($env:TAURI_ENV_TARGET_TRIPLE) { $env:TAURI_ENV_TARGET_TRIPLE } else { "x86_64-pc-windows-msvc" }
$cargoTargetDir = if ($env:CARGO_TARGET_DIR) {
  if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
    [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
  } else {
    [IO.Path]::GetFullPath((Join-Path $projectRoot $env:CARGO_TARGET_DIR))
  }
} else {
  Join-Path $projectRoot "src-tauri\target"
}
$releaseDir = Join-Path $cargoTargetDir "release"
$bundleDir = Join-Path $releaseDir "bundle\portable"
$stagingDir = Join-Path $bundleDir "BanmaCollector-$version"
$archivePath = Join-Path $bundleDir "BanmaCollector_${version}_x64-portable.zip"

$appSource = Join-Path $releaseDir "banma-collector.exe"
if (-not (Test-Path -LiteralPath $appSource)) {
  throw "Release executable was not found. Build the application before creating a portable archive."
}

$bundleRoot = [IO.Path]::GetFullPath($bundleDir)
$stagingRoot = [IO.Path]::GetFullPath($stagingDir)
if (-not $stagingRoot.StartsWith($bundleRoot, [StringComparison]::OrdinalIgnoreCase)) {
  throw "Invalid portable staging directory: $stagingRoot"
}

New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null
if (Test-Path -LiteralPath $stagingDir) {
  Remove-Item -LiteralPath $stagingDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null

Copy-Item -LiteralPath $appSource -Destination (Join-Path $stagingDir "BanmaCollector.exe")
foreach ($tool in @("ffmpeg", "mp4dump", "mp4decrypt")) {
  $source = Join-Path $projectRoot "src-tauri\binaries\$tool-$targetTriple.exe"
  if (-not (Test-Path -LiteralPath $source)) { throw "Missing portable runtime tool: $source" }
  Copy-Item -LiteralPath $source -Destination (Join-Path $stagingDir "$tool.exe")
}
Copy-Item -LiteralPath (Join-Path $projectRoot "README.md") -Destination $stagingDir
Copy-Item -LiteralPath (Join-Path $projectRoot "LICENSE") -Destination $stagingDir

if (Test-Path -LiteralPath $archivePath) { Remove-Item -LiteralPath $archivePath -Force }
Compress-Archive -Path (Join-Path $stagingDir "*") -DestinationPath $archivePath -CompressionLevel Optimal
Remove-Item -LiteralPath $stagingDir -Recurse -Force

Write-Host "Portable archive created: $archivePath"
