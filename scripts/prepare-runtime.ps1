$ErrorActionPreference = "Stop"

if (-not $IsWindows -and $PSVersionTable.PSEdition -eq "Core") {
  throw "The desktop package currently supports Windows only."
}

$projectRoot = Split-Path -Parent $PSScriptRoot
$binaryDir = Join-Path $projectRoot "src-tauri\binaries"
$cacheDir = Join-Path $projectRoot ".runtime-cache"
$targetTriple = if ($env:TAURI_ENV_TARGET_TRIPLE) { $env:TAURI_ENV_TARGET_TRIPLE } else { "x86_64-pc-windows-msvc" }
$suffix = "-$targetTriple.exe"

New-Item -ItemType Directory -Force -Path $binaryDir, $cacheDir | Out-Null

function Install-ToolsFromArchive {
  param(
    [Parameter(Mandatory = $true)][string]$Url,
    [Parameter(Mandatory = $true)][string[]]$Tools,
    [Parameter(Mandatory = $true)][string]$ArchiveName
  )

  $missing = @($Tools | Where-Object { -not (Test-Path -LiteralPath (Join-Path $binaryDir "$_$suffix")) })
  if ($missing.Count -eq 0) { return }

  $archivePath = Join-Path $cacheDir $ArchiveName
  $extractPath = Join-Path $cacheDir ([IO.Path]::GetFileNameWithoutExtension($ArchiveName))
  if (-not (Test-Path -LiteralPath $archivePath)) {
    Write-Host "Downloading $ArchiveName ..."
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $archivePath
  }
  if (Test-Path -LiteralPath $extractPath) { Remove-Item -Recurse -Force -LiteralPath $extractPath }
  Expand-Archive -LiteralPath $archivePath -DestinationPath $extractPath -Force

  foreach ($tool in $missing) {
    $source = Get-ChildItem -LiteralPath $extractPath -Recurse -File -Filter "$tool.exe" | Select-Object -First 1
    if (-not $source) { throw "$tool.exe was not found in $ArchiveName" }
    Copy-Item -LiteralPath $source.FullName -Destination (Join-Path $binaryDir "$tool$suffix") -Force
  }
}

$ffmpegUrl = if ($env:BANMA_FFMPEG_URL) { $env:BANMA_FFMPEG_URL } else { "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip" }
$bento4Url = if ($env:BANMA_BENTO4_URL) { $env:BANMA_BENTO4_URL } else { "https://www.bok.net/Bento4/binaries/Bento4-SDK-1-6-0-641.x86_64-microsoft-win32.zip" }

Install-ToolsFromArchive -Url $ffmpegUrl -Tools @("ffmpeg") -ArchiveName "ffmpeg-windows.zip"
Install-ToolsFromArchive -Url $bento4Url -Tools @("mp4dump", "mp4decrypt") -ArchiveName "bento4-windows.zip"

foreach ($tool in @("ffmpeg", "mp4dump", "mp4decrypt")) {
  $path = Join-Path $binaryDir "$tool$suffix"
  if (-not (Test-Path -LiteralPath $path)) { throw "Missing release runtime dependency: $path" }
  if ((Get-Item -LiteralPath $path).Length -lt 1024) { throw "Invalid release runtime dependency: $path" }
}

Write-Host "Release runtime tools are ready for $targetTriple."
