#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Enable test signing and generate a self-signed development certificate for iexdd.sys.

.DESCRIPTION
    Performs the one-time setup needed to load test-signed kernel drivers:
      1. Enables test signing via bcdedit.
      2. Generates a self-signed EV-style code-signing cert ("iExtend Dev").
      3. Installs the cert into the local machine root and trusted-publisher stores.
      4. Prompts for a reboot (required for bcdedit change to take effect).

    After reboot:
      • `bcdedit | findstr testsigning` should show "testsigning    Yes"
      • Build the driver and sign with:
          signtool sign /v /s PrivateCertStore /n "iExtend Dev" `
                        /t http://timestamp.digicert.com `
                        x64\Debug\iexdd\iexdd.sys

.NOTES
    Run this in an elevated PowerShell session on the development / test VM.
    Do NOT run on a production machine — test signing weakens system integrity.

    To undo:
        bcdedit /deletevalue testsigning
        (reboot)
        # Remove the cert from certmgr.msc > Local Computer > Trusted Root CAs.
#>

[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$CertSubject = "iExtend Dev",
    [string]$CertStoreLocation = "Cert:\LocalMachine\My",
    [switch]$SkipRebootPrompt
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# ---------------------------------------------------------------------------
# Step 1: Enable test signing
# ---------------------------------------------------------------------------
Write-Host "[1/4] Enabling test signing via bcdedit..." -ForegroundColor Cyan

$bcdeditOutput = & bcdedit /set testsigning on 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Error "bcdedit failed: $bcdeditOutput"
    exit 1
}
Write-Host "      Test signing enabled." -ForegroundColor Green

# ---------------------------------------------------------------------------
# Step 2: Generate a self-signed certificate
# ---------------------------------------------------------------------------
Write-Host "[2/4] Generating self-signed code-signing certificate '$CertSubject'..." -ForegroundColor Cyan

# Check if cert already exists.
$existingCert = Get-ChildItem $CertStoreLocation |
    Where-Object { $_.Subject -eq "CN=$CertSubject" } |
    Select-Object -First 1

if ($existingCert) {
    Write-Host "      Certificate already exists (thumbprint: $($existingCert.Thumbprint))." -ForegroundColor Yellow
    $cert = $existingCert
} else {
    $cert = New-SelfSignedCertificate `
        -Subject       "CN=$CertSubject" `
        -Type          CodeSigningCert `
        -KeyUsage      DigitalSignature `
        -KeyAlgorithm  RSA `
        -KeyLength     4096 `
        -HashAlgorithm SHA256 `
        -CertStoreLocation $CertStoreLocation `
        -NotAfter      (Get-Date).AddYears(5)

    Write-Host "      Certificate created (thumbprint: $($cert.Thumbprint))." -ForegroundColor Green
}

# ---------------------------------------------------------------------------
# Step 3: Install cert into Trusted Root CA + Trusted Publisher stores
# ---------------------------------------------------------------------------
Write-Host "[3/4] Installing certificate into Trusted Root CA and Trusted Publisher stores..." -ForegroundColor Cyan

function Install-CertInStore {
    param([System.Security.Cryptography.X509Certificates.X509Certificate2]$Certificate,
          [string]$StoreName)

    $store = New-Object System.Security.Cryptography.X509Certificates.X509Store(
        $StoreName,
        [System.Security.Cryptography.X509Certificates.StoreLocation]::LocalMachine
    )
    $store.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)

    $existing = $store.Certificates | Where-Object { $_.Thumbprint -eq $Certificate.Thumbprint }
    if (-not $existing) {
        $store.Add($Certificate)
        Write-Host "      Installed into $StoreName." -ForegroundColor Green
    } else {
        Write-Host "      Already present in $StoreName (skipped)." -ForegroundColor Yellow
    }
    $store.Close()
}

Install-CertInStore -Certificate $cert -StoreName "Root"
Install-CertInStore -Certificate $cert -StoreName "TrustedPublisher"

# Export the public cert for sharing with other machines / CI.
$exportPath = Join-Path $PSScriptRoot "iexdd_test.cer"
Export-Certificate -Cert $cert -FilePath $exportPath -Type CERT | Out-Null
Write-Host "      Public cert exported to: $exportPath" -ForegroundColor Green

# ---------------------------------------------------------------------------
# Step 4: Reboot prompt
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "[4/4] Setup complete." -ForegroundColor Green
Write-Host ""
Write-Host "  Certificate subject : CN=$CertSubject"
Write-Host "  Certificate thumbprint: $($cert.Thumbprint)"
Write-Host "  Exported cert: $exportPath"
Write-Host ""
Write-Host "  IMPORTANT: Test signing requires a reboot to take effect." -ForegroundColor Yellow

if (-not $SkipRebootPrompt) {
    $answer = Read-Host "Reboot now? [y/N]"
    if ($answer -match '^[Yy]') {
        Write-Host "Rebooting in 5 seconds..." -ForegroundColor Red
        Start-Sleep -Seconds 5
        Restart-Computer -Force
    } else {
        Write-Host "Reboot deferred. Remember to reboot before loading the driver." -ForegroundColor Yellow
    }
}

# ---------------------------------------------------------------------------
# Usage reminder
# ---------------------------------------------------------------------------
Write-Host ""
Write-Host "After reboot, verify with:"
Write-Host "  bcdedit | findstr testsigning"
Write-Host "  # Should show: testsigning    Yes"
Write-Host ""
Write-Host "To sign the driver (from an elevated prompt in the build output dir):"
Write-Host "  signtool sign /v /s PrivateCertStore /n `"$CertSubject`" ``"
Write-Host "               /t http://timestamp.digicert.com ``"
Write-Host "               iexdd.sys iexdd.cat"
Write-Host ""
Write-Host "To install the driver:"
Write-Host "  cargo xtask install-windows-driver"
