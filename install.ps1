# install.ps1 — Windows installer twin (PRD req 46).
#
# **WINDOWS BINARIES ARE NOT PUBLISHED YET.** ort-sys ships no prebuilt ONNX
# runtime for x86_64-pc-windows-gnu (observed 2026-08-26), so that target is
# not in the release matrix and no asset exists to download. This script
# therefore refuses loudly instead of 404-ing, and stays ready for the day the
# target ships: flip $WindowsPublished to $true, nothing else changes.
#
# Also UNVALIDATED: no Windows machine is in the loop yet; validation
# procedure lives in
# docs/plans/002-docker-daemon-base/assets/poc/cross-platform-strategy.md.

$ErrorActionPreference = "Stop"

$WindowsPublished = $false          # <-- flip when the windows target ships

$Repo = "AI-Substrate/flowspace3"
$AssetPrefix = if ($env:FS3_ASSET_PREFIX) { $env:FS3_ASSET_PREFIX } else { "flowspace3-" }  # ASSET NAME FREEZE POINT
$Triple = "x86_64-pc-windows-gnu"
$Asset = "$AssetPrefix$Triple.exe"

if (-not $WindowsPublished) {
    Write-Error @"
flowspace3 does not publish Windows binaries yet.

Why: the local embedder (ort/ONNX Runtime) has no prebuilt library for
$Triple, so the release matrix ships macOS (Apple Silicon) and Linux
(x86_64, aarch64) only.

Options today:
  * run flowspace3 under WSL2 and use the Linux installer:
      curl -fsSL https://raw.githubusercontent.com/$Repo/main/install.sh | sh
  * build from source on Windows with a toolchain that can supply ONNX Runtime.

Track it: https://github.com/$Repo/releases
"@
    exit 1
}

$Base = if ($env:FS3_INSTALL_ASSET_BASE) { $env:FS3_INSTALL_ASSET_BASE } else { "https://github.com/$Repo/releases/latest/download" }

$DestDir = if ($env:LOCALAPPDATA) { "$env:LOCALAPPDATA\Programs\flowspace3" } else { "$HOME\.local\bin" }
New-Item -ItemType Directory -Force -Path $DestDir | Out-Null

$Tmp = New-Item -ItemType Directory -Force -Path ([System.IO.Path]::GetTempPath() + [System.Guid]::NewGuid())
try {
    Write-Host "fetching $Asset ..."
    Invoke-WebRequest -Uri "$Base/$Asset" -OutFile "$Tmp/flowspace3.exe"
    Move-Item -Force "$Tmp/flowspace3.exe" "$DestDir/flowspace3.exe"
    Write-Host "installed: $DestDir\flowspace3.exe"
    Write-Host "note: add $DestDir to your PATH to use it from any shell."
} finally {
    Remove-Item -Recurse -Force $Tmp
}
