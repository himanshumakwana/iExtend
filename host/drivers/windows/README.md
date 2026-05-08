# Windows Driver: iexdd.sys

Indirect Display Driver (IDD) for iExtend. Registers a virtual WDDM adapter
and one virtual monitor, then delivers zero-copy frames to the Rust user-mode
partner (`ix-display-windows`) via an inverted-call IOCTL pipe.

---

## Quick-start (development loop)

### Prerequisites

- Windows 11 22H2+ (build 22621 or later) — required for IddCx 1.10
- Visual Studio 2022 with:
  - MSVC v143 toolset (x64)
  - Windows SDK 10.0.26100
  - Windows Driver Kit (WDK) — standalone installer matching the SDK
- Rust 1.77+ (`rustup default stable-x86_64-pc-windows-msvc`)
- Admin rights for install / signing steps

### 1. Enable test signing (one-time, dev VM only)

```powershell
# Run in an elevated PowerShell on the test VM:
.\host\drivers\windows\iexdd\test-signing.ps1
# Script enables bcdedit /set testsigning on and installs a self-signed cert.
# Reboot when prompted.
```

> **Warning:** Do not enable test signing on a production machine. Use a Hyper-V
> VM or a spare laptop. Test-signed drivers bypass kernel code-integrity; an
> unstable driver can wedge the machine.

### 2. Build the driver

```cmd
cargo xtask build-windows-driver --config debug --test-sign
```

### 3. Install the driver

```cmd
# Elevated prompt:
cargo xtask install-windows-driver --config debug
```

Open **Settings → System → Display**. A second display named
"iExtend Virtual Display" should appear. Right-click → "Extend" to attach it.

### 4. Run a WPP trace (optional)

```cmd
# Elevated prompt:
cargo xtask trace
# Press Ctrl+C to stop. Produces iexdd.etl.
tracefmt iexdd.etl -o iexdd.txt
```

### 5. Remove the driver

```cmd
devcon remove Root\iExtendDisplay
pnputil /delete-driver iexdd.inf /uninstall
```

---

## WHQL Submission Flow

Complete WHQL attestation is required before the driver can load on production
Windows machines without test signing. The process takes 8–15 calendar days
the first time (mostly waiting). Do this early — every team that ships a kernel
driver discovers Partner Center problems at the worst time.

### Step 1: Procure an EV codesigning certificate

You need an Extended Validation code-signing cert from a Microsoft-trusted CA.
The EV hardware token is required for WHQL submission; a software OV cert is
**not** accepted.

| Provider   | Price/yr | Typical onboarding |
|------------|----------|-------------------|
| DigiCert   | ~$320    | 5 business days after ID verification |
| Sectigo    | ~$280    | 7–10 business days |
| GlobalSign | ~$500    | 5–7 business days |

Order the cert **before** writing driver code — verification + token shipment
is the long pole. During development, use a self-signed test cert (see §1 above).

### Step 2: Set up Microsoft Partner Center (one-time)

1. Go to <https://partner.microsoft.com/> and sign in with an Azure AD account.
2. Enrol in the **Hardware** program (requires an EV cert — sign their test
   binary to prove possession of the cert).
3. Budget 3 business days for Microsoft to approve the account.

### Step 3: Set up the Hardware Lab Kit (HLK)

The HLK is Microsoft's test framework. You need two machines:

- **HLK Controller + Studio** — installs on a Windows Server or Win11 machine.
  Never install on the device under test.
- **HLK Client** — installs on the test target (the machine running iexdd.sys).
  A Hyper-V VM with test signing on is fine.

**Critical:** HLK version must exactly match the OS build on the test target.
Download from: <https://learn.microsoft.com/en-us/windows-hardware/test/hlk/windows-hardware-lab-kit>

### Step 4: Run the IDD certification tests

In HLK Studio:
1. Create a new project.
2. Select the test target machine.
3. Under **Features**, select the `Indirect Display` feature for `iexdd`.
4. Run the `Indirect Display Driver Certification` playlist.
   - Typical runtime: 2–4 hours.
   - Allow up to 3 re-runs for transient failures (network dropouts, etc.).
5. Confirm all tests pass. Expected waiver: "multi-monitor" tests — we
   advertise one monitor intentionally. File an exception via Partner Center.

### Step 5: Generate the .hlkx submission package

In HLK Studio → **Package** → **Create Package**. This produces a single
`.hlkx` archive containing:
- Test results
- Driver binaries (iexdd.sys, iexdd.inf, iexdd.cat — unsigned at this point)

### Step 6: Upload to Partner Center

1. Partner Center → **Hardware** → **Drivers** → **Submit new driver**.
2. Upload the `.hlkx` file.
3. Under **Target audiences**, select: `Windows 11 — 22H2 Client (x64)`.
4. Submit. Status changes: *Pending* → *In progress* → *Signed*.
   Typical turnaround: **24–72 hours**.

### Step 7: Download the signed catalog

Once status is **Signed**:
1. Download the signed package from Partner Center.
2. Extract `iexdd.cat` — this is now Microsoft-signed.
3. Place it in `host/drivers/windows/iexdd/` (overwriting the build-generated one).
4. Re-run `cargo xtask install-windows-driver` on a machine **without** test signing.
   The driver should load without errors.

### Troubleshooting matrix

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| Driver doesn't load, Event ID 7045 | Test signing not enabled | Run `test-signing.ps1`, reboot |
| Driver loads but monitor doesn't appear | EDID checksum wrong or `IddCxMonitorArrival` not called | Check WPP trace: `cargo xtask trace` |
| WHQL tests fail on multi-monitor | Expected — we advertise 1 monitor | File exception waiver in Partner Center |
| Signing service rejects the catalog | Wrong target audience | Re-select `Windows 11 22H2 Client` |
| EV cert not accepted | Software OV cert used | Must be EV hardware token |
| `cargo xtask install-windows-driver` fails: "Access denied" | Not elevated | Run from Administrator prompt |
| `devcon` not found | WDK tools not in PATH | Install standalone WDK; add WDK bin to PATH |

---

## Dev loop timing (estimated)

| Phase | Duration |
|-------|----------|
| Environment setup (VS + WDK + Rust) | 1 day |
| EV cert procurement | 5–10 business days (calendar gating) |
| Partner Center onboarding | 3 business days (calendar gating) |
| Driver code + IOCTL pipe (Tasks 2–5) | 3–5 weeks |
| User-mode Rust partner (Task 6) | 1–2 weeks |
| HLK run + WHQL submission | 2–4 days active, 1–3 days waiting |
| Signed catalog back from Microsoft | 1–3 days |
| **Total (first time)** | **8–10 calendar weeks** |

---

## File layout

```
host/drivers/windows/iexdd/
├── Driver.c / Driver.h         DriverEntry, EvtDeviceAdd — intentionally minimal
├── Adapter.c / Adapter.h       IddCx adapter + monitor, mode list, EDID
├── SwapChain.c / SwapChain.h   Frame-producer thread, inverted-call IOCTL pump
├── IoQueue.c / IoQueue.h       WDF queue creation, IOCTL dispatch handlers
├── Edid.c / Edid.h             128-byte synthesised EDID 1.4 block
├── Public.h                    Kernel/user-mode IPC contract (struct layouts, IOCTL codes)
├── Trace.h                     WPP tracing macros and GUID
├── iexdd.vcxproj               MSBuild project (KMDF + IddCx, v143 toolset)
├── iexdd.inf                   INF for pnputil installation
└── test-signing.ps1            Dev test-signing setup helper
```
