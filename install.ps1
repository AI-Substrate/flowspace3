# install.ps1 — Windows installer twin (PRD req 46).
#
# **UNVALIDATED**: there is no Windows machine in the loop yet (plan 004
# records this deferred-validation stance). Treat as source-available
# reference until validated per docs/plans/002-docker-daemon-base/assets/poc/
# cross-platform-strategy.md.

$ErrorActionPreference = "Stop"

$Repo = "AI-Substrate/flowspace3"
$AssetPrefix = if ($env:FS3_ASSET_PREFIX) { $env:FS3_ASSET_PREFIX } else { "flowspace3-" }  # ASSET NAME FREEZE POINT
$Triple = "x86_64-pc-windows-gnu"   # single windows target in the matrix
$Asset = "$AssetPrefix$Triple.exe"

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
