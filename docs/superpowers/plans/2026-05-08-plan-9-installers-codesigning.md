# Plan 9 of 10 — Installers + Codesigning

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship signed, distributable installers for all three product surfaces — a Windows MSIX with EV-signed user-mode binaries and WHQL-signed kernel drivers, a Linux .deb / .rpm / AppImage stack with DKMS-managed kernel modules and MOK-enrolled SecureBoot support, and an iPad TestFlight + App Store Connect submission with zero-data-collection privacy posture.

**Architecture:** Three independent installer pipelines (Windows / Linux / iPad) coordinated by a single CI release workflow keyed off `v*` git tags. Code signing is the load-bearing primitive on every platform: Windows uses a paid EV Authenticode cert for binaries plus WHQL for drivers; Linux uses a self-managed MOK key for kernel modules; iPad uses Apple-managed certificates issued through Apple Developer Program enrollment. The CI release workflow builds, signs, and uploads all three artifacts; the iPad goes to TestFlight directly.

**Tech Stack:**
- Windows: MSIX, MakeAppx, SignTool (EV Authenticode), HLK Studio, Microsoft Hardware Dashboard, `pnputil`
- Linux: `debhelper`, `dpkg-buildpackage`, `rpmbuild`, `dkms`, `mokutil`, `appimage-builder`, systemd
- iPad: Xcode, `fastlane`, App Store Connect, Apple Developer Portal, Info.plist
- CI: GitHub Actions, runners (`windows-latest`, `ubuntu-latest`, `macos-latest`)

**Plan scope:** This is **Plan 9 of 10**. It depends on Plans 2–8 being complete — without a working `iextendd`, `iextend-tray`, IddCx driver, evdi DKMS, vhf-stylus driver, Wintab32 shim, and iPad app, there is nothing to install. This plan does **not** ship its own runtime code; every binary it bundles came out of an earlier plan.

**The honest reality of codesigning:** This work is mostly administrative, not technical. The longest pole is *waiting* — Microsoft WHQL takes 3–7 business days per submission; App Store review takes 24–48 hours. Plan release schedules accordingly. Pre-WHQL, the kernel drivers load only on machines with `bcdedit /set testsigning on`, which is fine for closed beta but not for shipping. Treat WHQL turnaround as a release-blocking dependency.

---

## File Structure

```
iExtend/
├── host/
│   ├── installer/
│   │   ├── windows/
│   │   │   ├── iextend.appxmanifest               # MSIX package manifest
│   │   │   ├── Package.appxmanifest               # MSIX package metadata
│   │   │   ├── build.ps1                          # PowerShell build + sign script
│   │   │   ├── sign.ps1                           # SignTool wrapper (EV cert from cert store)
│   │   │   ├── assets/                            # Store tile + screenshots
│   │   │   ├── whql/
│   │   │   │   ├── iextend-iddcx.inf              # IddCx driver INF
│   │   │   │   ├── iextend-vhf-stylus.inf         # vhf-stylus driver INF
│   │   │   │   ├── hlk-config.xml                 # HLK Studio test selection
│   │   │   │   ├── submit.ps1                     # Hardware Dashboard upload
│   │   │   │   └── README.md                      # WHQL submission runbook
│   │   │   └── README.md                          # Windows install runbook
│   │   ├── linux/
│   │   │   ├── debian/
│   │   │   │   ├── control                        # Package metadata + deps
│   │   │   │   ├── rules                          # debhelper rules
│   │   │   │   ├── changelog                      # debian changelog
│   │   │   │   ├── compat                         # debhelper compat level
│   │   │   │   ├── iextend.service                # systemd user service
│   │   │   │   ├── iextend-tray.desktop           # autostart desktop entry
│   │   │   │   ├── postinst                       # post-install hook
│   │   │   │   ├── prerm                          # pre-remove hook
│   │   │   │   └── copyright                      # license metadata
│   │   │   ├── rpm/
│   │   │   │   └── iextend.spec                   # RPM spec file
│   │   │   ├── dkms/
│   │   │   │   ├── postinst-mok.sh                # MOK enrollment UX (extends Plan 4)
│   │   │   │   ├── secureboot-detect.sh           # SecureBoot detection
│   │   │   │   └── README.md                      # DKMS-MOK runbook
│   │   │   ├── AppImage.yml                       # appimage-builder config
│   │   │   └── README.md                          # Linux install runbook
│   │   └── README.md                              # installer overview
├── ipad/
│   └── installer/
│       └── AppStoreConnect/
│           ├── Fastfile                           # fastlane lanes
│           ├── Appfile                            # team + bundle ID
│           ├── Deliverfile                        # App Store metadata
│           ├── PrivacyPolicy.md                   # zero-data-collection policy
│           ├── metadata/
│           │   ├── description.txt
│           │   ├── keywords.txt
│           │   ├── release_notes.txt
│           │   └── support_url.txt
│           ├── screenshots/                       # generated by snapshot
│           └── README.md                          # iPad release runbook
└── .github/
    └── workflows/
        └── release.yml                            # tag-triggered release pipeline
```

**Why this structure:** Each platform is a self-contained subtree under `host/installer/<platform>/` or `ipad/installer/`. The runbooks live next to the build scripts so a fresh contributor can rebuild each artifact without grepping the wider repo. The release workflow at the top level orchestrates all three.

---

### Task 1: Acquire EV Authenticode certificate (administrative)

**Files:**
- Create: `host/installer/windows/README.md` (procurement runbook)

This task is **administrative, not technical**. It's a precondition for every later Windows task. Document it so a successor can repeat it.

- [ ] **Step 1: Choose a certificate vendor**

Recommended: DigiCert, Sectigo, or GlobalSign. Cost: ~$300/year for an EV Authenticode cert. Avoid OV certs — Windows SmartScreen has a long warm-up period and EV bypasses it.

- [ ] **Step 2: Order the cert**

Vendor will ship a USB hardware token (FIPS 140-2 Level 2). Cert is non-exportable from the token by design. Plan for 5–10 business days for vetting.

- [ ] **Step 3: Install the token drivers and cert on the build machine**

Token vendor ships SafeNet Authentication Client or equivalent. Verify with:

```powershell
certutil -store -user My
```

Expected: cert appears with `Issuer = DigiCert EV Code Signing CA` (or your vendor's equivalent).

- [ ] **Step 4: Document the runbook**

Create `host/installer/windows/README.md` with the cert vendor, expiry date, token serial number (last 4 digits only), renewal calendar entry instructions, and the fact that the build machine must have the token plugged in for `signtool`.

- [ ] **Step 5: Commit**

```bash
git add host/installer/windows/README.md
git commit -m "docs(installer): document EV Authenticode cert procurement"
```

---

### Task 2: Author MSIX manifest and build script

**Files:**
- Create: `host/installer/windows/Package.appxmanifest`
- Create: `host/installer/windows/build.ps1`
- Create: `host/installer/windows/sign.ps1`
- Create: `host/installer/windows/assets/Square150x150Logo.png` (placeholder)
- Create: `host/installer/windows/assets/Square44x44Logo.png` (placeholder)
- Create: `host/installer/windows/assets/StoreLogo.png` (placeholder)

- [ ] **Step 1: Write `Package.appxmanifest`**

```xml
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
         xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
         xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
         xmlns:desktop="http://schemas.microsoft.com/appx/manifest/desktop/windows10"
         IgnorableNamespaces="uap rescap desktop">

  <Identity Name="iExtend.Host"
            Publisher="CN=iExtend, O=iExtend, C=US"
            Version="0.1.0.0"
            ProcessorArchitecture="x64" />

  <Properties>
    <DisplayName>iExtend</DisplayName>
    <PublisherDisplayName>iExtend</PublisherDisplayName>
    <Logo>assets\StoreLogo.png</Logo>
    <Description>Use your iPad as a wireless second screen for Windows.</Description>
  </Properties>

  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop"
                        MinVersion="10.0.19041.0"
                        MaxVersionTested="10.0.22631.0" />
  </Dependencies>

  <Resources>
    <Resource Language="en-us" />
  </Resources>

  <Applications>
    <Application Id="iExtendTray" Executable="iextend-tray.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="iExtend"
                          Description="iPad as second screen"
                          BackgroundColor="#0A84FF"
                          Square150x150Logo="assets\Square150x150Logo.png"
                          Square44x44Logo="assets\Square44x44Logo.png">
        <uap:DefaultTile />
      </uap:VisualElements>
      <Extensions>
        <desktop:Extension Category="windows.startupTask">
          <desktop:StartupTask TaskId="iExtendTrayAutostart"
                               Enabled="true"
                               DisplayName="iExtend tray" />
        </desktop:Extension>
      </Extensions>
    </Application>
  </Applications>

  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
</Package>
```

- [ ] **Step 2: Write `build.ps1`**

```powershell
[CmdletBinding()]
param(
  [string]$Configuration = "Release",
  [string]$Version = "0.1.0.0",
  [switch]$NoSign
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent | Split-Path -Parent

Write-Host "==> Building host crates ($Configuration)..."
Push-Location "$root\host"
cargo build --release --workspace
Pop-Location

$staging = "$PSScriptRoot\stage"
if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
New-Item -ItemType Directory -Path $staging | Out-Null

Write-Host "==> Staging binaries..."
Copy-Item "$root\host\target\release\iextendd.exe" $staging
Copy-Item "$root\host\target\release\iextend-tray.exe" $staging
Copy-Item "$root\host\target\release\Wintab32.dll" $staging
Copy-Item "$PSScriptRoot\Package.appxmanifest" $staging
Copy-Item "$PSScriptRoot\assets" "$staging\assets" -Recurse

Write-Host "==> Bumping manifest version to $Version..."
(Get-Content "$staging\Package.appxmanifest") -replace 'Version="[\d.]+"', "Version=`"$Version`"" |
  Set-Content "$staging\Package.appxmanifest"

$msix = "$PSScriptRoot\dist\iExtend-$Version.msix"
New-Item -ItemType Directory -Path "$PSScriptRoot\dist" -Force | Out-Null
Write-Host "==> Packing $msix..."
& MakeAppx.exe pack /d $staging /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed: $LASTEXITCODE" }

if (-not $NoSign) {
  Write-Host "==> Signing $msix..."
  & "$PSScriptRoot\sign.ps1" -Path $msix
}

Write-Host "==> Done. Output: $msix"
```

- [ ] **Step 3: Write `sign.ps1`**

```powershell
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string]$Path
)

$ErrorActionPreference = "Stop"

# EV cert lives on the SafeNet hardware token; thumbprint is configured per build machine.
$thumbprint = $env:IEXTEND_EV_THUMBPRINT
if (-not $thumbprint) {
  throw "Set IEXTEND_EV_THUMBPRINT environment variable to the EV cert thumbprint (no spaces)."
}

& signtool.exe sign `
  /sha1 $thumbprint `
  /tr http://timestamp.digicert.com `
  /td sha256 `
  /fd sha256 `
  /v $Path

if ($LASTEXITCODE -ne 0) { throw "signtool failed: $LASTEXITCODE" }

# Verify
& signtool.exe verify /pa /v $Path
if ($LASTEXITCODE -ne 0) { throw "signtool verify failed: $LASTEXITCODE" }
```

- [ ] **Step 4: Add placeholder PNG assets**

Create three 1x1 transparent PNGs at the listed paths. Real assets come from design later.

```powershell
$png = [byte[]] @(0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A,0x00,0x00,0x00,0x0D,0x49,0x48,0x44,0x52,
  0x00,0x00,0x00,0x01,0x00,0x00,0x00,0x01,0x08,0x06,0x00,0x00,0x00,0x1F,0x15,0xC4,0x89,0x00,0x00,
  0x00,0x0D,0x49,0x44,0x41,0x54,0x78,0x9C,0x62,0x00,0x01,0x00,0x00,0x05,0x00,0x01,0x0D,0x0A,0x2D,
  0xB4,0x00,0x00,0x00,0x00,0x49,0x45,0x4E,0x44,0xAE,0x42,0x60,0x82)
[IO.File]::WriteAllBytes("$PSScriptRoot\assets\Square150x150Logo.png", $png)
[IO.File]::WriteAllBytes("$PSScriptRoot\assets\Square44x44Logo.png", $png)
[IO.File]::WriteAllBytes("$PSScriptRoot\assets\StoreLogo.png", $png)
```

- [ ] **Step 5: Smoke test the build (no-sign)**

On a Windows machine with Rust + Windows SDK:

```powershell
cd host\installer\windows
.\build.ps1 -NoSign
Get-Item dist\iExtend-0.1.0.0.msix | Format-List Name,Length
```

Expected: file exists, size ≥ 5 MB (ish, depends on Plans 2/8 binary sizes).

- [ ] **Step 6: Commit**

```bash
git add host/installer/windows/
git commit -m "feat(installer): add MSIX manifest, build, and signing scripts"
```

---

### Task 3: WHQL submission for IddCx and vhf-stylus drivers

**Files:**
- Create: `host/installer/windows/whql/iextend-iddcx.inf`
- Create: `host/installer/windows/whql/iextend-vhf-stylus.inf`
- Create: `host/installer/windows/whql/hlk-config.xml`
- Create: `host/installer/windows/whql/submit.ps1`
- Create: `host/installer/windows/whql/README.md`

WHQL is **the longest pole in this plan** — 3–7 business days per submission. Schedule accordingly.

- [ ] **Step 1: Author IddCx INF (`iextend-iddcx.inf`)**

The INF references the .sys + .cat from Plan 3:

```ini
[Version]
Signature="$Windows NT$"
Class=Display
ClassGuid={4d36e968-e325-11ce-bfc1-08002be10318}
Provider=%ProviderName%
DriverVer=05/08/2026,0.1.0.0
CatalogFile=iextend-iddcx.cat
PnpLockdown=1

[Manufacturer]
%ProviderName%=iExtend, NTamd64

[iExtend.NTamd64]
%DeviceDesc%=iExtendIddcx, ROOT\iExtendIddcx

[iExtendIddcx.NT]
CopyFiles=iExtendIddcx.NT.Copy

[iExtendIddcx.NT.Copy]
iextend-iddcx.dll

[iExtendIddcx.NT.Services]
AddService=iextend-iddcx,0x00000002,iExtendIddcxService

[iExtendIddcxService]
DisplayName=%ServiceName%
ServiceType=1
StartType=3
ErrorControl=1
ServiceBinary=%12%\iextend-iddcx.dll

[Strings]
ProviderName="iExtend"
DeviceDesc="iExtend Indirect Display"
ServiceName="iExtend Indirect Display Service"
```

- [ ] **Step 2: Author vhf-stylus INF (`iextend-vhf-stylus.inf`)**

Same template, different class (HIDClass), bound to a virtual HID device. Mirror the pattern from `iextend-iddcx.inf` with `Class=HIDClass`, `ClassGuid={745A17A0-74D3-11D0-B6FE-00A0C90F57DA}`, and the vhf-stylus.dll path.

- [ ] **Step 3: HLK Studio test selection (`hlk-config.xml`)**

```xml
<HLKConfig version="1.0">
  <Project name="iExtend" buildNumber="0.1.0.0" />
  <TargetOS>Windows 11 22H2</TargetOS>
  <TestSelection>
    <Category name="Device.DevFund" required="true" />
    <Category name="Device.Display.Indirect" required="true" />
    <Category name="Device.Input.Digitizer" required="true" />
    <Category name="Device.Connectivity.Network.LAN" required="true" />
  </TestSelection>
  <Submission>
    <DashboardURL>https://partner.microsoft.com/dashboard/hardware</DashboardURL>
    <CompanyName>iExtend</CompanyName>
  </Submission>
</HLKConfig>
```

- [ ] **Step 4: Submission script (`submit.ps1`)**

```powershell
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string]$HlkPackagePath
)

$ErrorActionPreference = "Stop"

# Verify HLK package was produced by HLK Studio with the test machine pool
if (-not (Test-Path $HlkPackagePath)) { throw "HLKx package not found at $HlkPackagePath" }

Write-Host "==> Validating HLK package..."
& "$env:HLK_STUDIO\HLKStudio.exe" /ValidateSubmission $HlkPackagePath
if ($LASTEXITCODE -ne 0) { throw "HLK package validation failed" }

Write-Host "==> Upload to Hardware Dashboard manually:"
Write-Host "    1. Sign in: https://partner.microsoft.com/dashboard/hardware"
Write-Host "    2. New submission > Driver > Upload $HlkPackagePath"
Write-Host "    3. Wait 3–7 business days for Microsoft signing"
Write-Host "    4. Download signed .cab and extract .cat"
Write-Host "    5. Drop signed .cat into host\installer\windows\whql\signed\"
```

(Hardware Dashboard does not have a CLI for submission as of 2026; manual upload is unavoidable.)

- [ ] **Step 5: Write the WHQL runbook (`README.md`)**

Document: HLK Studio version pinned to Windows 11 22H2 HLK, test pool configuration (a domain-joined controller + at least one client running each target Windows version), expected duration (1 day to set up, 1 day for first HLK run, 3–7 days for Microsoft signing turnaround), and the post-signing step (drop the signed `.cat` into `host/installer/windows/whql/signed/` and re-run `build.ps1` to repackage MSIX with the signed driver).

- [ ] **Step 6: Commit**

```bash
git add host/installer/windows/whql/
git commit -m "feat(installer): WHQL INF, HLK config, and submission runbook"
```

---

### Task 4: Debian package (.deb)

**Files:**
- Create: `host/installer/linux/debian/control`
- Create: `host/installer/linux/debian/rules`
- Create: `host/installer/linux/debian/changelog`
- Create: `host/installer/linux/debian/compat`
- Create: `host/installer/linux/debian/iextend.service`
- Create: `host/installer/linux/debian/iextend-tray.desktop`
- Create: `host/installer/linux/debian/postinst`
- Create: `host/installer/linux/debian/prerm`
- Create: `host/installer/linux/debian/copyright`

- [ ] **Step 1: `debian/control`**

```
Source: iextend
Section: utils
Priority: optional
Maintainer: iExtend <maintainers@iextend.example>
Build-Depends: debhelper-compat (= 13), cargo (>= 1.75), pkg-config,
               libpipewire-0.3-dev, libdrm-dev
Standards-Version: 4.6.0
Homepage: https://iextend.example

Package: iextend
Architecture: amd64
Depends: ${shlibs:Depends}, ${misc:Depends},
         iextend-evdi-dkms (>= 0.1),
         systemd-user-services
Recommends: pipewire (>= 0.3.40)
Description: iPad as a wireless second screen for Linux
 iExtend turns an iPad into a wireless second monitor over Wi-Fi or
 USB-C, with Apple Pencil pressure/tilt forwarded as a Wacom-class
 stylus.
```

- [ ] **Step 2: `debian/rules`**

```makefile
#!/usr/bin/make -f
%:
	dh $@ --buildsystem=cargo

override_dh_auto_build:
	cd host && cargo build --release --workspace

override_dh_auto_install:
	install -Dm755 host/target/release/iextendd \
	  debian/iextend/usr/bin/iextendd
	install -Dm755 host/target/release/iextend-tray \
	  debian/iextend/usr/bin/iextend-tray
	install -Dm644 host/installer/linux/debian/iextend.service \
	  debian/iextend/usr/lib/systemd/user/iextend.service
	install -Dm644 host/installer/linux/debian/iextend-tray.desktop \
	  debian/iextend/etc/xdg/autostart/iextend-tray.desktop
```

- [ ] **Step 3: `debian/changelog`**

```
iextend (0.1.0-1) unstable; urgency=medium

  * Initial release.

 -- iExtend <maintainers@iextend.example>  Fri, 08 May 2026 10:00:00 +0000
```

- [ ] **Step 4: `debian/compat`**

```
13
```

- [ ] **Step 5: `iextend.service` (systemd user unit)**

```ini
[Unit]
Description=iExtend daemon
After=network-online.target

[Service]
Type=simple
ExecStart=/usr/bin/iextendd
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

- [ ] **Step 6: `iextend-tray.desktop` (XDG autostart)**

```ini
[Desktop Entry]
Type=Application
Name=iExtend tray
Exec=/usr/bin/iextend-tray
Icon=iextend
Terminal=false
Categories=Utility;Network;
StartupNotify=false
X-GNOME-Autostart-enabled=true
```

- [ ] **Step 7: `postinst`**

```bash
#!/bin/sh
set -e

if [ "$1" = "configure" ]; then
  systemctl --global enable iextend.service || true
  echo "iExtend installed. Log out and back in to start the tray app."
fi

#DEBHELPER#
exit 0
```

- [ ] **Step 8: `prerm`**

```bash
#!/bin/sh
set -e

if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
  systemctl --global disable iextend.service || true
fi

#DEBHELPER#
exit 0
```

- [ ] **Step 9: `copyright`**

Standard Debian-format file declaring Apache-2.0 for the iExtend code and noting that `iextend-evdi-dkms` (separate package) is GPL-2.0.

- [ ] **Step 10: Smoke build the .deb**

On Ubuntu 24.04:

```bash
cd host
dpkg-buildpackage -b -uc -us
ls -la ../iextend_0.1.0-1_amd64.deb
```

Expected: .deb file produced, lintian warnings only (no errors).

- [ ] **Step 11: Commit**

```bash
git add host/installer/linux/debian/
git commit -m "feat(installer): Debian package definition"
```

---

### Task 5: RPM spec

**Files:**
- Create: `host/installer/linux/rpm/iextend.spec`

- [ ] **Step 1: Write `iextend.spec`**

```spec
Name:           iextend
Version:        0.1.0
Release:        1%{?dist}
Summary:        iPad as a wireless second screen
License:        Apache-2.0
URL:            https://iextend.example
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo >= 1.75
BuildRequires:  pkgconfig(libpipewire-0.3)
BuildRequires:  pkgconfig(libdrm)
Requires:       iextend-evdi-dkms >= 0.1
Requires:       systemd

%description
iExtend turns an iPad into a wireless second monitor over Wi-Fi or
USB-C, with Apple Pencil pressure/tilt forwarded as a Wacom-class
stylus.

%prep
%setup -q

%build
cd host && cargo build --release --workspace

%install
install -Dm755 host/target/release/iextendd \
  %{buildroot}%{_bindir}/iextendd
install -Dm755 host/target/release/iextend-tray \
  %{buildroot}%{_bindir}/iextend-tray
install -Dm644 host/installer/linux/debian/iextend.service \
  %{buildroot}%{_userunitdir}/iextend.service
install -Dm644 host/installer/linux/debian/iextend-tray.desktop \
  %{buildroot}%{_sysconfdir}/xdg/autostart/iextend-tray.desktop

%post
%systemd_user_post iextend.service

%preun
%systemd_user_preun iextend.service

%files
%{_bindir}/iextendd
%{_bindir}/iextend-tray
%{_userunitdir}/iextend.service
%{_sysconfdir}/xdg/autostart/iextend-tray.desktop

%changelog
* Fri May 08 2026 iExtend <maintainers@iextend.example> - 0.1.0-1
- Initial release.
```

- [ ] **Step 2: Smoke build on Fedora**

```bash
rpmbuild -bb host/installer/linux/rpm/iextend.spec
ls ~/rpmbuild/RPMS/x86_64/iextend-0.1.0-1.*.rpm
```

- [ ] **Step 3: Commit**

```bash
git add host/installer/linux/rpm/
git commit -m "feat(installer): RPM spec mirroring Debian package"
```

---

### Task 6: DKMS postinst MOK enrollment UX

**Files:**
- Create: `host/installer/linux/dkms/postinst-mok.sh`
- Create: `host/installer/linux/dkms/secureboot-detect.sh`
- Create: `host/installer/linux/dkms/README.md`

This finishes the MOK enrollment UX whose key generation was started in Plan 4.

- [ ] **Step 1: `secureboot-detect.sh`**

```bash
#!/bin/sh
# Returns 0 if SecureBoot is enabled, 1 otherwise, 2 if status is unknown.
set -e

if [ ! -d /sys/firmware/efi ]; then
  exit 1  # legacy BIOS, no SecureBoot
fi

if command -v mokutil >/dev/null 2>&1; then
  state=$(mokutil --sb-state 2>/dev/null || echo "unknown")
  case "$state" in
    *"enabled"*)  exit 0 ;;
    *"disabled"*) exit 1 ;;
    *)            exit 2 ;;
  esac
fi

# Fall back to efivar
if [ -f /sys/firmware/efi/efivars/SecureBoot-* ]; then
  byte=$(od -An -tu1 -j 4 -N 1 /sys/firmware/efi/efivars/SecureBoot-* 2>/dev/null | tr -d ' ')
  [ "$byte" = "1" ] && exit 0 || exit 1
fi

exit 2
```

- [ ] **Step 2: `postinst-mok.sh`**

```bash
#!/bin/sh
set -e

KEY_DIR="/var/lib/dkms/iextend-evdi/mok"
KEY_DER="$KEY_DIR/MOK.der"
KEY_PEM="$KEY_DIR/MOK.pem"

# Plan 4 already generated and stored the MOK key here. Verify.
if [ ! -f "$KEY_DER" ]; then
  echo "iExtend: MOK key missing at $KEY_DER. evdi-dkms install incomplete."
  exit 1
fi

if "$(dirname "$0")/secureboot-detect.sh"; then
  cat <<EOF

  ════════════════════════════════════════════════════════════
  iExtend: SecureBoot is enabled on this system
  ════════════════════════════════════════════════════════════

  The evdi kernel module is signed with iExtend's Machine Owner
  Key. To allow the kernel to load it under SecureBoot, you need
  to enroll this key — a one-time step.

  We will now run:

    mokutil --import $KEY_DER

  You'll be asked for a TEMPORARY enrollment password. Pick
  anything you'll remember for the next reboot. After this
  completes, REBOOT your machine. On reboot you'll see a blue
  MOK Manager screen — choose "Enroll MOK", confirm, enter the
  password, and reboot again.

  After that, iExtend's evdi module will load automatically.

  Press Enter to continue, or Ctrl+C to skip and enroll later
  (you can re-run this script as: dpkg-reconfigure iextend-evdi-dkms).

EOF
  read _
  mokutil --import "$KEY_DER"
  echo
  echo "Done. REBOOT now to complete MOK enrollment."
else
  echo "iExtend: SecureBoot disabled — MOK enrollment not needed."
fi
```

- [ ] **Step 3: Wire into `debian/postinst` of the `iextend-evdi-dkms` package (cross-reference Plan 4)**

This requires editing the existing Plan 4 postinst. Add at the end:

```bash
if [ -x /usr/share/iextend/dkms/postinst-mok.sh ]; then
  /usr/share/iextend/dkms/postinst-mok.sh
fi
```

(Plan 4 will need a follow-up commit to source `postinst-mok.sh` and ship it under `/usr/share/iextend/dkms/`.)

- [ ] **Step 4: Runbook**

Write `host/installer/linux/dkms/README.md` covering the MOK lifecycle, how to verify enrollment (`mokutil --list-enrolled | grep -A1 "iExtend"`), and how to revoke.

- [ ] **Step 5: Commit**

```bash
git add host/installer/linux/dkms/
git commit -m "feat(installer): DKMS postinst MOK enrollment UX"
```

---

### Task 7: AppImage

**Files:**
- Create: `host/installer/linux/AppImage.yml`

- [ ] **Step 1: Write `AppImage.yml`**

```yaml
version: 1
script:
  - cd host && cargo build --release --workspace
AppDir:
  path: ./AppDir
  app_info:
    id: example.iextend
    name: iExtend
    icon: iextend
    version: 0.1.0
    exec: usr/bin/iextend-tray
  apt:
    arch: amd64
    sources:
      - sourceline: deb http://archive.ubuntu.com/ubuntu/ noble main universe
    include:
      - libpipewire-0.3-0
      - libdrm2
  files:
    include:
      - host/target/release/iextendd
      - host/target/release/iextend-tray
    exclude:
      - usr/share/man
      - usr/share/doc
AppImage:
  arch: x86_64
  update-information: gh-releases-zsync|iextend|iextend|latest|*.AppImage.zsync
```

- [ ] **Step 2: Document the limits**

Add a top-of-file comment in `AppImage.yml`:

```
# AppImage CANNOT install the evdi kernel module. This AppImage works only on:
#   (a) machines that already have iextend-evdi-dkms installed, or
#   (b) the v2 user-space-only mode (PipeWire screencast portal directly,
#       no virtual monitor — read-only mirror only).
# For most users, prefer the .deb / .rpm packages.
```

- [ ] **Step 3: Smoke build**

```bash
appimage-builder --recipe host/installer/linux/AppImage.yml --skip-test
ls iExtend-0.1.0-x86_64.AppImage
```

- [ ] **Step 4: Commit**

```bash
git add host/installer/linux/AppImage.yml
git commit -m "feat(installer): AppImage recipe (no-driver, advanced users)"
```

---

### Task 8: Apple Developer Program enrollment + identifiers

**Files:** none (administrative work documented in next task's README).

This task is administrative. Document each step but do not commit code.

- [ ] **Step 1: Enroll**

Visit https://developer.apple.com/programs/. $99/year. Use a corporate identity if possible (D-U-N-S number speeds up review).

- [ ] **Step 2: Create the App ID**

Apple Developer Portal > Identifiers > App IDs > New.
- Description: `iExtend`
- Bundle ID (Explicit): `com.iextend.iExtend` (or your org's reverse-DNS)
- Capabilities: none beyond defaults (no push, no iCloud, no HealthKit; Network.framework needs no entitlement)

- [ ] **Step 3: Create distribution certificate**

Certificates > New > "iOS Distribution". Save the .p12 to a 1Password vault entry called `iExtend / Apple / Distribution Cert`.

- [ ] **Step 4: Create App Store provisioning profile**

Profiles > New > "App Store" > select the App ID and the distribution cert. Download.

- [ ] **Step 5: Document everything**

Stash all of this in a private 1Password vault. Don't commit the .p12 or any credentials.

---

### Task 9: Info.plist + Xcode distribution config

**Files:**
- Modify: `ipad/iExtend.xcodeproj` configurations
- Modify: `ipad/iExtendUI/Info.plist`

- [ ] **Step 1: `Info.plist` keys**

```xml
<key>CFBundleDisplayName</key>
<string>iExtend</string>
<key>CFBundleIdentifier</key>
<string>com.iextend.iExtend</string>
<key>CFBundleVersion</key>
<string>1</string>
<key>CFBundleShortVersionString</key>
<string>0.1.0</string>
<key>UIDeviceFamily</key>
<array><integer>2</integer></array>  <!-- iPad only -->
<key>UIRequiredDeviceCapabilities</key>
<array>
  <string>wifi</string>
  <string>metal</string>
</array>
<key>NSLocalNetworkUsageDescription</key>
<string>iExtend needs to find your computer on your Wi-Fi network and stream
your screen between them.</string>
<key>NSBonjourServices</key>
<array>
  <string>_iextend._tcp</string>
</array>
<key>UIBackgroundModes</key>
<array>
  <string>processing</string>
</array>
<key>UISupportedInterfaceOrientations</key>
<array>
  <string>UIInterfaceOrientationLandscapeLeft</string>
  <string>UIInterfaceOrientationLandscapeRight</string>
  <string>UIInterfaceOrientationPortrait</string>
  <string>UIInterfaceOrientationPortraitUpsideDown</string>
</array>
<key>LSApplicationCategoryType</key>
<string>public.app-category.utilities</string>
```

`NSLocalNetworkUsageDescription` is **mandatory** for iOS 14+ and triggers the local-network permission prompt on first connect attempt. Without it, mDNS discovery silently fails.

- [ ] **Step 2: Xcode project: Distribution scheme**

Configure the `Release` build configuration with:
- Code Signing Identity: `Apple Distribution`
- Provisioning Profile: `iExtend App Store`
- Strip Linked Product: Yes
- Deployment Target: iPadOS 17.0

- [ ] **Step 3: Commit**

```bash
git add ipad/iExtendUI/Info.plist ipad/iExtend.xcodeproj
git commit -m "feat(ipad): Info.plist + Xcode distribution config"
```

---

### Task 10: fastlane for TestFlight + App Store Connect

**Files:**
- Create: `ipad/installer/AppStoreConnect/Fastfile`
- Create: `ipad/installer/AppStoreConnect/Appfile`
- Create: `ipad/installer/AppStoreConnect/Deliverfile`
- Create: `ipad/installer/AppStoreConnect/metadata/description.txt`
- Create: `ipad/installer/AppStoreConnect/metadata/keywords.txt`
- Create: `ipad/installer/AppStoreConnect/metadata/release_notes.txt`
- Create: `ipad/installer/AppStoreConnect/metadata/support_url.txt`
- Create: `ipad/installer/AppStoreConnect/README.md`

- [ ] **Step 1: `Appfile`**

```ruby
app_identifier "com.iextend.iExtend"
apple_id ENV["APPLE_ID"]
team_id ENV["APPLE_TEAM_ID"]
itc_team_id ENV["APPLE_ITC_TEAM_ID"]
```

- [ ] **Step 2: `Fastfile`**

```ruby
default_platform(:ios)

platform :ios do
  desc "Build a Release IPA"
  lane :build do
    setup_ci if ENV["CI"]
    match(type: "appstore", readonly: true)
    increment_build_number(xcodeproj: "../../iExtend.xcodeproj")
    build_app(
      scheme: "iExtend",
      configuration: "Release",
      export_method: "app-store"
    )
  end

  desc "Push to TestFlight"
  lane :beta do
    build
    upload_to_testflight(
      skip_waiting_for_build_processing: true,
      changelog: File.read("metadata/release_notes.txt")
    )
  end

  desc "Push to App Store Connect for review"
  lane :release do
    build
    upload_to_app_store(
      submit_for_review: true,
      automatic_release: false,
      submission_information: {
        add_id_info_uses_idfa: false,
        export_compliance_uses_encryption: true,
        export_compliance_encryption_updated: false,
        export_compliance_is_exempt: true   # standard-encryption-only exemption
      }
    )
  end
end
```

- [ ] **Step 3: `Deliverfile`**

```ruby
metadata_path "./metadata"
screenshots_path "./screenshots"
app_review_information(
  first_name: "iExtend",
  last_name: "Support",
  phone_number: "+1-555-0100",
  email_address: "review@iextend.example",
  notes: "Pair the iPad with the companion Windows or Linux app via the
4-digit PIN or QR code. Both devices must be on the same Wi-Fi
network. No login required."
)
```

- [ ] **Step 4: Metadata files**

`metadata/description.txt`:

```
iExtend turns your iPad into a wireless second screen for your Windows
or Linux laptop.

• Drag windows over to your iPad and keep working — it's a real second
  monitor, not a mirror (toggle to mirror anytime).
• Apple Pencil pressure and tilt forwarded as a real drawing tablet —
  Photoshop, Krita, OneNote, and Clip Studio all just work.
• Wi-Fi or USB-C. No cables required.
• No cloud account, no telemetry, no login.

Requires the free iExtend desktop app for Windows or Linux.
```

`metadata/keywords.txt`:

```
ipad,second screen,extend,sidecar,wacom,drawing tablet,wireless display
```

`metadata/release_notes.txt`:

```
Initial release.
```

`metadata/support_url.txt`:

```
https://iextend.example/support
```

- [ ] **Step 5: Local smoke test**

```bash
cd ipad/installer/AppStoreConnect
bundle exec fastlane beta
```

Expected: ipa builds, uploads to TestFlight, processing email arrives within ~10 minutes.

- [ ] **Step 6: Commit**

```bash
git add ipad/installer/AppStoreConnect/
git commit -m "feat(ipad): fastlane lanes for TestFlight + App Store"
```

---

### Task 11: Privacy policy + App Store privacy nutrition label

**Files:**
- Create: `ipad/installer/AppStoreConnect/PrivacyPolicy.md`

- [ ] **Step 1: Write `PrivacyPolicy.md`**

```markdown
# iExtend Privacy Policy

_Last updated: 2026-05-08_

## Summary

iExtend collects no data. None.

## What we don't collect

- No personal information
- No device identifiers
- No usage analytics
- No telemetry by default
- No advertising IDs
- No location data
- No contacts, photos, calendar, or health data

## Network use

iExtend communicates only between your iPad and your own computer on
your local Wi-Fi network. No data leaves your local network during
normal use. There are no iExtend servers, no cloud account, no login.

## Optional crash reports

If — and only if — you opt in to crash reporting in Settings, the iPad
app may upload a crash backtrace plus your device model, iPadOS version,
and app version to a self-hosted server. We never collect personal
information, even with crash reports enabled. You can disable crash
reports at any time in Settings.

## Third-party services

None.

## Children's privacy

iExtend does not knowingly collect data from anyone, including children.

## Contact

privacy@iextend.example
```

- [ ] **Step 2: App Store Connect privacy section**

In App Store Connect > App Privacy, declare:
- Data Types Collected: **None** (when crash reports are off — the default)
- Data Used to Track You: **None**
- Privacy Policy URL: `https://iextend.example/privacy` (host the .md as static HTML on GitHub Pages)

- [ ] **Step 3: Host the policy**

Either:
(a) Push `PrivacyPolicy.md` to a GitHub Pages branch, OR
(b) Render it via Jekyll/MkDocs/your static site generator at `iextend.example/privacy`.

- [ ] **Step 4: Commit**

```bash
git add ipad/installer/AppStoreConnect/PrivacyPolicy.md
git commit -m "docs(ipad): zero-data-collection privacy policy"
```

---

### Task 12: Release CI workflow

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Write the workflow**

```yaml
name: Release

on:
  push:
    tags: ['v*']

jobs:
  windows:
    runs-on: windows-latest
    env:
      IEXTEND_EV_THUMBPRINT: ${{ secrets.IEXTEND_EV_THUMBPRINT }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rs/toolchain@v1
        with: { toolchain: stable, override: true }
      - name: Build & sign MSIX
        shell: pwsh
        run: |
          cd host\installer\windows
          .\build.ps1 -Version "${{ github.ref_name }}".TrimStart('v') + ".0"
      - uses: softprops/action-gh-release@v2
        with:
          files: host/installer/windows/dist/*.msix

  linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rs/toolchain@v1
        with: { toolchain: stable, override: true }
      - name: System deps
        run: |
          sudo apt-get update
          sudo apt-get install -y debhelper rpm appimage-builder \
            libpipewire-0.3-dev libdrm-dev pkg-config
      - name: Build .deb
        run: cd host && dpkg-buildpackage -b -uc -us
      - name: Build .rpm
        run: rpmbuild -bb host/installer/linux/rpm/iextend.spec
      - name: Build AppImage
        run: appimage-builder --recipe host/installer/linux/AppImage.yml --skip-test
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            ../iextend_*.deb
            ~/rpmbuild/RPMS/x86_64/iextend-*.rpm
            iExtend-*.AppImage

  ipad:
    runs-on: macos-latest
    env:
      MATCH_PASSWORD: ${{ secrets.MATCH_PASSWORD }}
      APPLE_ID: ${{ secrets.APPLE_ID }}
      APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
      APPLE_ITC_TEAM_ID: ${{ secrets.APPLE_ITC_TEAM_ID }}
      FASTLANE_APP_SPECIFIC_PASSWORD: ${{ secrets.FASTLANE_APP_SPECIFIC_PASSWORD }}
    steps:
      - uses: actions/checkout@v4
      - uses: maxim-lobanov/setup-xcode@v1
        with: { xcode-version: latest-stable }
      - name: Push to TestFlight
        run: |
          cd ipad/installer/AppStoreConnect
          bundle install
          bundle exec fastlane beta
```

- [ ] **Step 2: Configure repo secrets**

In GitHub repo Settings > Secrets and variables > Actions:

- `IEXTEND_EV_THUMBPRINT` — EV cert thumbprint (no spaces)
- `MATCH_PASSWORD` — fastlane match keychain password
- `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_ITC_TEAM_ID`
- `FASTLANE_APP_SPECIFIC_PASSWORD` — generated at appleid.apple.com

The Windows runner needs the EV token *plugged in*. GitHub-hosted runners don't support hardware tokens; this will require either a self-hosted runner with the token, or `azuresignedtool` against a cloud HSM (DigiCert KeyLocker). For v1, use a self-hosted Windows runner pinned to one machine.

- [ ] **Step 3: Test the workflow**

Push a test tag:

```bash
git tag v0.0.1-rc1
git push origin v0.0.1-rc1
```

Watch the Actions tab. Three jobs run in parallel: windows, linux, ipad. All three must succeed for a release to be valid.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: tag-triggered release pipeline (Windows + Linux + iPad)"
```

---

## Done criteria

1. `host/installer/windows/dist/iExtend-<version>.msix` builds, is signed by the EV cert, and installs cleanly on a fresh Windows 11 22H2 VM.
2. `host/installer/windows/whql/` package has been submitted to the Microsoft Hardware Dashboard at least once and a signed `.cat` retrieved (allow 3–7 business days).
3. `iextend_<version>_amd64.deb` builds and installs cleanly on Ubuntu 24.04 (lintian: zero errors).
4. `iextend-<version>.x86_64.rpm` builds via `rpmbuild` and installs cleanly on Fedora 40.
5. `iExtend-<version>-x86_64.AppImage` builds and runs on a machine that already has `iextend-evdi-dkms` installed.
6. The DKMS postinst-mok flow runs end-to-end on a SecureBoot-enabled Ubuntu 24.04 VM: prompt appears, mokutil import succeeds, post-reboot enrollment confirmed via `mokutil --list-enrolled`.
7. `bundle exec fastlane beta` from `ipad/installer/AppStoreConnect/` produces a TestFlight build that lands in the App Store Connect "TestFlight" tab and processes successfully.
8. Privacy policy renders at the public URL declared in App Store Connect.
9. Pushing a `v*` tag triggers `release.yml`, which produces all three artifacts and uploads them to a GitHub Release.
10. Pre-WHQL caveat is documented in the Windows runbook: "v0.0.x betas require `bcdedit /set testsigning on` because the kernel drivers are not yet WHQL-signed. Do not ship outside closed beta until WHQL approval lands."

## Out of scope (later plans)

- Microsoft Store *publication* (we ship sideload-only via AppInstaller URL for v1; full Store submission is a v1.1 follow-up).
- Snap and Flatpak packaging — neither supports kernel modules cleanly; defer.
- macOS host (no plan; macOS has Sidecar).
- Auto-update *channels* (stable/beta/canary) — v1 ships from a single tag stream.

## Open questions

1. Self-hosted Windows runner with EV token vs. DigiCert KeyLocker (cloud HSM). KeyLocker is ~$500/yr extra but removes the dedicated machine. Decision deferred to first release cycle.
2. Whether to dual-sign Linux modules with both an iExtend MOK and the user's distro vendor key (Canonical, Red Hat). Cleaner UX but requires per-distro paperwork. Defer to v1.1.
3. Whether to apply for App Store Small Business Program (15% commission instead of 30%). Cosmetic; no impact on technical plan.
