param([switch]$Check)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot

if (-not (Get-Command Invoke-Formatter -ErrorAction SilentlyContinue)) {
  throw "PSScriptAnalyzer is required. Run: Install-Module PSScriptAnalyzer -Scope CurrentUser"
}

$settings = @{
  IncludeRules = @(
    "PSPlaceOpenBrace",
    "PSPlaceCloseBrace",
    "PSUseConsistentIndentation",
    "PSUseConsistentWhitespace"
  )
  Rules = @{
    PSPlaceOpenBrace = @{ OnSameLine = $true; NewLineAfter = $true }
    PSPlaceCloseBrace = @{ NewLineAfter = $true }
    PSUseConsistentIndentation = @{ Enable = $true; Kind = "space"; IndentationSize = 2 }
    PSUseConsistentWhitespace = @{ Enable = $true }
  }
}

$failed = $false
Get-ChildItem -LiteralPath (Join-Path $projectRoot "scripts") -Filter "*.ps1" -File | ForEach-Object {
  $source = [IO.File]::ReadAllText($_.FullName, [Text.Encoding]::UTF8)
  $formatted = Invoke-Formatter -ScriptDefinition $source -Settings $settings
  if ($Check) {
    if ($source -cne $formatted) {
      Write-Error "PowerShell formatting check failed: $($_.FullName)"
      $failed = $true
    }
  } else {
    [IO.File]::WriteAllText($_.FullName, $formatted, [Text.UTF8Encoding]::new($false))
  }
}

if ($failed) { exit 1 }
