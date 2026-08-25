$ErrorActionPreference = "Stop"

$projectRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$targets = @(
  (Join-Path $projectRoot "dist"),
  (Join-Path $projectRoot "test-results"),
  (Join-Path $projectRoot "playwright-report"),
  (Join-Path $projectRoot "src-tauri\target")
)

foreach ($target in $targets) {
  $resolved = [IO.Path]::GetFullPath($target)
  if (-not $resolved.StartsWith($projectRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to clean a path outside the project: $resolved"
  }
  if (Test-Path -LiteralPath $resolved) {
    Write-Host "Removing $resolved"
    Remove-Item -LiteralPath $resolved -Recurse -Force
  }
}

Write-Host "Generated build and test outputs have been removed."
