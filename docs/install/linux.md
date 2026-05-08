# Installing iExtend on Linux

iExtend on Linux requires two components:

1. **`iextend-evdi-dkms`** — the evdi kernel module (GPL-2.0, maintained by
   DisplayLink), shipped as a DKMS source package that builds against your
   running kernel.
2. **`iextend`** — the daemon + tray app (Apache-2.0).

They are separate packages to keep the GPL license boundary clean: the daemon
loads `libevdi` only at runtime via `dlopen` and does not inherit the GPL.

---

## Supported configurations

| Distro / Config | Status | Notes |
|---|---|---|
| Ubuntu 22.04 LTS | Supported | `apt` repo |
| Ubuntu 24.04 LTS | Supported | `apt` repo |
| Debian 12 (Bookworm) | Supported | `apt` repo |
| Fedora 39+ | Supported | `dnf` repo |
| Arch Linux | Community | AUR (`iextend-evdi-dkms`) |
| RHEL 9 / Rocky 9 | Supported | Needs `kernel-devel` package |
| **Fedora Silverblue / Bazzite** | **Not supported** | Immutable root; can't install kernel modules |
| **Steam Deck (SteamOS)** | **Not supported** | Read-only root |
| **ChromeOS Flex** | **Not supported** | Locked-down kernel |
| Enterprise images with module signing locked to OEM keys | **Not supported** | MOK enrollment blocked |
| Flatpak / Snap iextendd | **Not supported** | Can't access `/dev/evdi*` |

> **v2 note:** A software-rendered fallback that avoids the kernel module is
> planned for v2.  It will trade 10–20 ms of additional latency for broader
> distro support.  Track progress at:
> `docs/superpowers/specs/2026-05-08-iextend-design.md §1`.

---

## Quick install — Ubuntu / Debian

```bash
# Add the iExtend apt repository
curl -fsSL https://apt.iextend.app/gpg | sudo gpg --dearmor \
     -o /usr/share/keyrings/iextend.gpg
echo "deb [signed-by=/usr/share/keyrings/iextend.gpg] \
     https://apt.iextend.app/ubuntu $(lsb_release -cs) main" \
     | sudo tee /etc/apt/sources.list.d/iextend.list

sudo apt update
sudo apt install iextend iextend-evdi-dkms
```

The package will:

1. Build the `evdi` kernel module against your running kernel via DKMS.
2. If SecureBoot is enabled: generate a signing certificate and print a
   `mokutil --import` command.  **Run it before rebooting.**
3. Install the iextend daemon and tray app.

---

## Quick install — Fedora / RHEL

```bash
sudo dnf config-manager --add-repo https://rpm.iextend.app/iextend.repo
sudo dnf install iextend iextend-evdi-dkms

# Ensure kernel development headers are present for your running kernel.
sudo dnf install "kernel-devel-$(uname -r)"
```

---

## SecureBoot walkthrough

If `mokutil --sb-state` prints `SecureBoot enabled`, the postinst has already
generated `/var/lib/iextend/MOK.der` — a machine-unique certificate.  You need
to enroll it once:

```bash
# 1. Import the certificate (sets a one-time password).
sudo mokutil --import /var/lib/iextend/MOK.der

# 2. Reboot.
sudo reboot

# 3. At the blue "Perform MOK management" screen:
#    Enroll MOK → Continue → enter the password you set → Reboot.

# 4. After the second reboot, verify:
lsmod | grep evdi   # should show "evdi"
ls /dev/evdi*       # should show "/dev/evdi0"
```

The certificate is unique to your machine.  It does not grant us (or anyone
else) the ability to load arbitrary modules; it only matches the `evdi` build
produced by the DKMS package for your kernels.

---

## Verifying the installation

```bash
# Kernel module loaded?
lsmod | grep evdi

# Device node present?
ls -l /dev/evdi*

# Daemon running?
systemctl --user status iextend

# Logs (shows "virtual monitor connected" on success):
journalctl --user -u iextend -n 50
```

---

## Troubleshooting

### `lsmod | grep evdi` shows nothing

```bash
# Load manually:
sudo modprobe evdi initial_device_count=1

# If that fails with "Required key not available":
#   → SecureBoot MOK enrollment was not completed.  Repeat the steps above.

# If that fails with "Module evdi not found":
#   → DKMS build failed.  Check:
dkms status iextend-evdi
dkms build  iextend-evdi -v 1.14.7 -k "$(uname -r)"
```

### `/dev/evdi*` missing after `modprobe evdi`

```bash
# Force device node creation:
sudo modprobe evdi initial_device_count=1
# Should create /dev/evdi0.  If still missing, check kernel ring buffer:
dmesg | grep evdi | tail -20
```

### Daemon starts but no frame arrives within 2 s (Wayland)

1. Is the virtual monitor visible to the compositor?
   ```bash
   # On KDE / Sway:
   wlr-randr | grep EVDI
   # On GNOME:
   gnome-randr   # or: xrandr --query | grep EVDI
   ```
2. Was the xdg-desktop-portal grant remembered?  Re-run iextend interactively;
   the portal picker should appear once.  Choose the EVDI-1 output.

### Daemon starts but no frame arrives (X11 path)

```bash
# Check that XShm and XDamage extensions are available:
xdpyinfo -ext MIT-SHM | grep "MIT-SHM"
xdpyinfo -ext DAMAGE   | grep "DAMAGE"

# Check that EVDI-1 appears in RandR:
xrandr | grep EVDI
```

---

## Uninstalling

```bash
# Debian / Ubuntu:
sudo apt remove iextend iextend-evdi-dkms

# Fedora / RHEL:
sudo dnf remove iextend iextend-evdi-dkms

# Optionally revoke the MOK signing certificate:
sudo mokutil --delete /var/lib/iextend/MOK.der
# Reboot and follow the blue MOK Manager prompt.
```

---

## Architecture notes (for contributors)

- **Why DKMS?** evdi is an out-of-tree kernel module.  DKMS rebuilds it
  automatically on kernel upgrades so users don't need to intervene.
- **Why a separate package?** evdi is GPL-2.0.  The iextend daemon is
  Apache-2.0.  Keeping them in separate `.deb`/`.rpm` packages with a
  runtime-only linkage (`dlopen`) means the daemon does not need to be
  relicensed.
- **Why not Flatpak?** Flatpak sandboxes cannot access `/dev/evdi*` by
  default.  We ship native packages only.
- **NVIDIA proprietary driver caveat:** The proprietary driver (pre-555 open
  kernel module) cannot expose DMA-BUF for NVENC ingest.  iExtend detects
  this at startup and switches to a CUDA-interop copy (~0.3 ms overhead).
  See `host/crates/ix-display-linux/src/nvidia_cuda.rs`.
