<#
.SYNOPSIS
    Builds and (optionally) signs the iExtend MSIX package.

.DESCRIPTION
    1. Runs `cargo build --release` for the host workspace.
    2. Stages binaries + manifest + assets into a temporary `stage\` directory.
    3. Patches the manifest version to match -Version.
    4. Packs the MSIX via MakeAppx.exe.
    5. Signs the MSIX via sign.ps1 (reads EV cert thumbprint from IEXTEND_EV_THUMBPRINT).

    MakeAppx.exe and SignTool.exe must be on PATH (install Windows SDK 10.0.22621+).
    Rust toolchain (stable, x86_64-pc-windows-msvc) must be installed.

.PARAMETER Configuration
    Cargo profile to build. Default: Release.

.PARAMETER Version
    Four-part MSIX version string, e.g. "0.1.0.0". Default: 0.1.0.0.

.PARAMETER NoSign
    Skip code signing. Produces an unsigned .msix for local smoke testing.
    The resulting package will only install on machines with
    `bcdedit /set testsigning on` or when Developer Mode is enabled.

.EXAMPLE
    .\build.ps1 -Version "0.2.1.0"
    .\build.ps1 -NoSign   # local test without EV token
#>
[CmdletBinding()]
param(
    [string]$Configuration = "Release",
    [string]$Version       = "0.1.0.0",
    [switch]$NoSign
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Resolve repo root: this script lives at host/installer/windows/build.ps1
$windowsDir = $PSScriptRoot
$installerDir = Split-Path $windowsDir -Parent
$hostDir    = Split-Path $installerDir -Parent
$repoRoot   = Split-Path $hostDir -Parent

Write-Host "==> iExtend MSIX build — version $Version ($Configuration)" -ForegroundColor Cyan
Write-Host "    Repo root : $repoRoot"
Write-Host "    Host dir  : $hostDir"

# ---------------------------------------------------------------------------
# Step 1: Build Rust workspace
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "==> [1/5] Building Rust workspace ($Configuration)..." -ForegroundColor Yellow
Push-Location $hostDir
try {
    cargo build --profile ([string]::ToLower($Configuration)) --workspace
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
} finally {
    Pop-Location
}

$targetDir = Join-Path $hostDir "target\$($Configuration.ToLower())"

# ---------------------------------------------------------------------------
# Step 2: Stage artifacts
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "==> [2/5] Staging artifacts..." -ForegroundColor Yellow

$stagingDir = Join-Path $windowsDir "stage"
if (Test-Path $stagingDir) { Remove-Item -Recurse -Force $stagingDir }
New-Item -ItemType Directory -Path $stagingDir | Out-Null

$binaries = @(
    "iextendd.exe",
    "iextend-tray.exe",
    "Wintab32.dll"
)

foreach ($bin in $binaries) {
    $src = Join-Path $targetDir $bin
    if (-not (Test-Path $src)) {
        throw "Expected binary not found: $src`nRun `cargo build --release --workspace` first."
    }
    Copy-Item $src $stagingDir
    Write-Host "    Staged: $bin"
}

# Manifest
Copy-Item (Join-Path $windowsDir "iextend.appxmanifest") (Join-Path $stagingDir "AppxManifest.xml")

# Assets (logos, screenshots)
$assetsDir = Join-Path $windowsDir "assets"
if (Test-Path $assetsDir) {
    Copy-Item $assetsDir (Join-Path $stagingDir "assets") -Recurse
    Write-Host "    Staged: assets\"
} else {
    Write-Warning "assets\ not found — placeholder PNGs expected. Run asset generation step."
}

# ---------------------------------------------------------------------------
# Step 3: Patch manifest version
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "==> [3/5] Patching manifest version to $Version..." -ForegroundColor Yellow

$manifestPath = Join-Path $stagingDir "AppxManifest.xml"
$manifestContent = Get-Content $manifestPath -Raw
$manifestContent = $manifestContent -replace 'Version="[\d.]+"', "Version=`"$Version`""
Set-Content -Path $manifestPath -Value $manifestContent -Encoding UTF8
Write-Host "    Manifest version set to $Version"

# ---------------------------------------------------------------------------
# Step 4: Pack MSIX
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "==> [4/5] Packing MSIX..." -ForegroundColor Yellow

$distDir = Join-Path $windowsDir "dist"
New-Item -ItemType Directory -Path $distDir -Force | Out-Null

$msixPath = Join-Path $distDir "iExtend-$Version.msix"

# MakeAppx.exe: prefer Windows SDK copy; fall back to PATH
$makeAppx = "MakeAppx.exe"
$sdkMakeAppx = "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.0.22621.0\x64\MakeAppx.exe"
if (Test-Path $sdkMakeAppx) { $makeAppx = $sdkMakeAppx }

& $makeAppx pack /d $stagingDir /p $msixPath /o
if ($LASTEXITCODE -ne 0) { throw "MakeAppx.exe failed (exit $LASTEXITCODE)" }

Write-Host "    Packed: $msixPath"
$msixSize = (Get-Item $msixPath).Length
Write-Host ("    Size  : {0:N0} bytes" -f $msixSize)

# ---------------------------------------------------------------------------
# Step 5: Sign (unless -NoSign)
# ---------------------------------------------------------------------------
if ($NoSign) {
    Write-Host ""
    Write-Warning "-NoSign was specified. Skipping code signing."
    Write-Warning "The resulting .msix will only install with Developer Mode or test signing enabled."
    Write-Warning "  bcdedit /set testsigning on   (requires reboot; v0.x beta only)"
} else {
    Write-Host ""
    Write-Host "==> [5/5] Signing MSIX..." -ForegroundColor Yellow
    & (Join-Path $windowsDir "sign.ps1") -Path $msixPath
}

Write-Host ""
Write-Host "==> Build complete." -ForegroundColor Green
Write-Host "    Output: $msixPath"
