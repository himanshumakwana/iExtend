# Windows IddCx Virtual Display Driver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a signed Windows Indirect Display Driver (`iexdd.sys`) that registers a virtual GPU adapter and one virtual monitor with WDDM, plus a user-mode Rust crate (`ix-display-windows`) that owns the IddCx user-mode partner role and produces a `Receiver<GpuFrame>` of `ID3D11Texture2D` shared handles for downstream encoder consumers. After this plan, Windows treats iExtend as a real second monitor in `ms-settings:display`; capture is zero-copy from IddCx swapchain to user-mode encoder input.

**Architecture:** A single in-process IddCx pair — kernel-mode minifilter (`iexdd.sys`) registers the virtual adapter via `IddCxDeviceInitialize` and the user-mode partner attaches by opening `\\.\IExtendDisplay` and registering as the IDD client. Frames flow swap-chain → kernel buffer queue → inverted-call IOCTL → user-mode `ix-display-windows` → `tokio::sync::mpsc::Sender<GpuFrame>` (zero-copy via `IDXGIResource1::CreateSharedHandle`). The driver owns no D3D context itself — it just hands out shared handles whose lifetime is reference-counted across the kernel/user boundary. Mode list is fixed (1080p120, 1440p120, 4K60); EDID is synthesized in-driver.

**Tech Stack:**
- WDK 10.0.26100 (Windows 11 24H2 SDK) or newer; IddCx 1.10
- MSVC v143 toolset, `vcpkg` not used (driver has no third-party deps)
- Driver C language: `/std:c17`, `/W4`, `/WX`, KMDF 1.33
- Rust user-mode: 1.77+, `bindgen 0.69`, `tokio 1.36+`, `windows 0.56` crate for COM + DXGI types
- Codesigning: EV cert (DigiCert / Sectigo, ~$300/yr) + Microsoft Hardware Dashboard for WHQL attestation
- Test signing for dev: self-signed cert via `MakeCert` + `bcdedit /set testsigning on`

**Plan scope:** This is **Plan 3 of 10** for the iExtend project. Plan 1 (`iExtend.html` visual deliverable) is complete. Plan 2 (Rust host workspace bootstrap) **must be complete** before this plan starts — it provides the cargo workspace, the `xtask` runner, and the placeholder `host/crates/ix-display-windows/` crate stub that this plan fills in.

**Cost estimate (be honest):**
This is the longest pole in the v1 schedule. For a Rust engineer who has not written a Windows driver before:

- Driver development + inverted-call IOCTL: 4–6 weeks of focused work
- WHQL submission turnaround: 1–2 weeks (gated, not active dev)
- Buffer for cert procurement, EV-cert hardware token shipment, Microsoft Partner Center onboarding: ~2 weeks of calendar time, mostly waiting

Plan for **8–10 calendar weeks end-to-end**. The first 2 weeks are mostly setup that can run in parallel with Plan 4 (Linux evdi).

**What this plan does NOT do:**

- No encoder selection or frame encoding — that's Plan 5 (`ix-codec`, `ix-rtc`).
- No WebRTC or networking — Plan 5.
- No installer / MSIX / driver-package codesigning of release artifacts — Plan 9. This plan ships test-signed artifacts and a documented WHQL submission process; the actual signed `.cat` lands in Plan 9's release pipeline.
- No multi-monitor support in v1; the driver advertises exactly one monitor.

---

## File Structure

```
iExtend/host/
├── drivers/
│   └── windows/
│       └── iexdd/
│           ├── iexdd.vcxproj                 # MSBuild project; KMDF + IddCx
│           ├── iexdd.inf                     # INF for `pnputil /add-driver` install
│           ├── iexdd.rc                      # version resource
│           ├── Public.h                      # IOCTL codes + struct layouts shared with user-mode
│           ├── Driver.h / Driver.c           # DriverEntry, WDF device init
│           ├── Adapter.h / Adapter.c         # IddCx adapter + monitor attach
│           ├── SwapChain.h / SwapChain.c     # processing thread + IOCTL pump
│           ├── Edid.h / Edid.c               # synthesized EDID for the virtual monitor
│           └── Trace.h                       # WPP tracing macros
├── crates/
│   └── ix-display-windows/
│       ├── Cargo.toml
│       ├── build.rs                          # bindgen against Public.h
│       ├── wrapper.h                         # bindgen entry point
│       └── src/
│           ├── lib.rs                        # public API: VirtualDisplay::new(), .frames()
│           ├── ioctl.rs                      # inverted-call wrapper, DeviceIoControl helper
│           ├── frame.rs                      # GpuFrame { texture: ID3D11Texture2D, dirty: Vec<RECT>, present_time }
│           └── error.rs                      # WindowsError + thiserror
└── xtask/src/
    └── windows_driver.rs                     # `cargo xtask build-windows-driver`, `cargo xtask install-windows-driver`
```

**Important boundaries:**

- `Public.h` is the single source of truth for the kernel/user IPC layout. Both the driver project and `bindgen` in `ix-display-windows/build.rs` read it. Any field added must come with a versioned struct (no silent ABI changes).
- `Driver.c` does no work beyond `DriverEntry` and `EvtDriverDeviceAdd` — all per-device logic lives in `Adapter.c`. This keeps `DriverEntry` testable-by-inspection.
- `SwapChain.c` runs the per-monitor processing thread; it never calls anything outside its own header (no global state). The thread shuts down on `EvtIddCxMonitorUnassignSwapChain`.
- `ix-display-windows` re-exports nothing from the `windows` crate — its public API uses opaque `GpuFrame` handles. Downstream `ix-codec` does not learn about IddCx existing.

---

## Task 1: Set up the driver build environment

**Files:** none (host machine setup).

This task is documentation + tool installation. Subsequent tasks assume these are done.

- [ ] **Step 1: Install Visual Studio 2022 with C++ + Windows Driver workload**

Download and run the Visual Studio installer. Select:

- **Desktop development with C++** workload
- Individual components:
  - MSVC v143 — VS 2022 C++ x64/x86 build tools
  - Windows 11 SDK 10.0.26100 or newer
  - C++/CLI support for v143 build tools
  - Windows Driver Kit (search for "WDK")

Verify:

```cmd
where /R "C:\Program Files (x86)\Windows Kits\10\bin" devcon.exe
where /R "C:\Program Files (x86)\Windows Kits\10\Include" Iddcx.h
```

Both paths must exist. `Iddcx.h` is the IddCx header; if it's missing the WDK install is incomplete.

- [ ] **Step 2: Install standalone WDK matching the SDK**

Even with the VS workload, install the standalone WDK from `https://learn.microsoft.com/en-us/windows-hardware/drivers/download-the-wdk`. The standalone installer adds the driver project templates and the test-signing toolchain. Match WDK version to SDK version exactly (e.g., both 10.0.26100).

- [ ] **Step 3: Install Rust toolchain on Windows**

```cmd
rustup default stable-x86_64-pc-windows-msvc
rustup component add rust-src llvm-tools-preview
cargo install cargo-xtask --locked
```

Verify with `cargo --version` (must be 1.77+).

- [ ] **Step 4: Procure an EV codesigning cert**

The signed driver requires an Extended Validation cert from a Microsoft-trusted CA. Recommended providers (sorted by typical onboarding speed):

1. **DigiCert EV Code Signing** — ~$320/yr; physical USB token shipped within 5 business days after identity verification.
2. **Sectigo EV Code Signing** — ~$280/yr; identity verification can take 7–10 business days.
3. **GlobalSign EV Code Signing** — ~$500/yr.

Order **before** starting Task 2 — verification + token shipment is the long pole. Until the EV cert is in hand, Tasks 2–7 use a self-signed test cert (Step 5).

- [ ] **Step 5: Generate a self-signed test cert for development**

In an admin developer command prompt:

```cmd
makecert -r -pe -ss PrivateCertStore -n "CN=iExtend Dev" iexdd_test.cer
certmgr -add -c iexdd_test.cer -s -r localMachine root
certmgr -add -c iexdd_test.cer -s -r localMachine trustedpublisher
bcdedit /set testsigning on
shutdown /r /t 5
```

After reboot, `bcdedit | findstr testsigning` must show `testsigning Yes`. Test-signed drivers will load; production-signed drivers still work normally.

⚠️ **Test signing in a VM is strongly recommended.** A stuck driver from a botched build can wedge a real machine into recovery mode. Use Hyper-V or a spare laptop, not your daily driver, for the development loop.

- [ ] **Step 6: Document the dev environment in the repo**

No commit yet (no repo files were created); proceed to Task 2.

---

## Task 2: Scaffold the driver project (failing build)

**Files:**
- Create: `host/drivers/windows/iexdd/iexdd.vcxproj`
- Create: `host/drivers/windows/iexdd/iexdd.inf`
- Create: `host/drivers/windows/iexdd/Driver.h`
- Create: `host/drivers/windows/iexdd/Driver.c`
- Create: `host/drivers/windows/iexdd/Public.h`
- Modify: `host/xtask/src/main.rs` (register the new subcommand)

- [ ] **Step 1: Generate the project from the IddCx WDK template, then strip it down**

In Visual Studio: `File → New → Project → Indirect Display Driver (KMDF)`. Save into `host/drivers/windows/iexdd/`. Delete every generated file *except* `iexdd.vcxproj` and `iexdd.inf`. We will replace their contents.

- [ ] **Step 2: Write `Public.h` — the IPC contract**

```c
// Public.h — kernel/user-mode IPC contract for iexdd. Bumping IEXDD_PROTOCOL_VERSION
// is required for any layout change; old user-mode partners will refuse to connect.

#pragma once
#include <devioctl.h>
#include <wdm.h>

#define IEXDD_PROTOCOL_VERSION 1

#define IEXDD_DEVICE_TYPE 0x9D00 // unallocated user range

#define IOCTL_IEXDD_HELLO         CTL_CODE(IEXDD_DEVICE_TYPE, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_IEXDD_PULL_FRAME    CTL_CODE(IEXDD_DEVICE_TYPE, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS)
#define IOCTL_IEXDD_RELEASE_FRAME CTL_CODE(IEXDD_DEVICE_TYPE, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS)

typedef struct _IEXDD_HELLO {
    UINT32 ProtocolVersion;   // must equal IEXDD_PROTOCOL_VERSION
    UINT32 ClientPid;
    GUID   ClientNonce;       // user-mode-generated; echoed back so user-mode can match calls
} IEXDD_HELLO, *PIEXDD_HELLO;

typedef struct _IEXDD_FRAME_HEADER {
    UINT64    PresentTimeQpc;       // QueryPerformanceCounter at vsync
    UINT64    AcquireSeq;           // monotonic per-monitor; gaps = drops
    HANDLE    SharedTextureHandle;  // NT handle from CreateSharedHandle, duplicated into ClientPid
    UINT32    Width;
    UINT32    Height;
    UINT32    DirtyRectCount;
    UINT32    Reserved;
    // followed by DirtyRectCount * RECT in the same buffer
} IEXDD_FRAME_HEADER, *PIEXDD_FRAME_HEADER;

typedef struct _IEXDD_FRAME_RELEASE {
    UINT64 AcquireSeq;
} IEXDD_FRAME_RELEASE, *PIEXDD_FRAME_RELEASE;
```

- [ ] **Step 3: Write `Driver.h` and `Driver.c`**

```c
// Driver.h
#pragma once
#include <ntddk.h>
#include <wdf.h>
#include <iddcx.h>
#include "Public.h"

EXTERN_C_START

DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD IexddEvtDeviceAdd;
EVT_WDF_DEVICE_CONTEXT_CLEANUP IexddEvtDeviceContextCleanup;

typedef struct _IEXDD_DEVICE_CONTEXT {
    IDDCX_ADAPTER Adapter;
} IEXDD_DEVICE_CONTEXT, *PIEXDD_DEVICE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(IEXDD_DEVICE_CONTEXT, IexddDeviceGetContext)

EXTERN_C_END
```

```c
// Driver.c
#include "Driver.h"

NTSTATUS DriverEntry(_In_ PDRIVER_OBJECT DriverObject, _In_ PUNICODE_STRING RegistryPath)
{
    WDF_DRIVER_CONFIG config;
    WDF_DRIVER_CONFIG_INIT(&config, IexddEvtDeviceAdd);
    config.DriverPoolTag = 'IExD';

    return WdfDriverCreate(DriverObject, RegistryPath, WDF_NO_OBJECT_ATTRIBUTES, &config, WDF_NO_HANDLE);
}

NTSTATUS IexddEvtDeviceAdd(_In_ WDFDRIVER, _Inout_ PWDFDEVICE_INIT DeviceInit)
{
    NTSTATUS status;
    WDF_OBJECT_ATTRIBUTES attrs;
    WDFDEVICE device;

    // Tell IddCx this is going to be an indirect display device — must be called BEFORE WdfDeviceCreate
    status = IddCxDeviceInitConfig(DeviceInit);
    if (!NT_SUCCESS(status)) return status;

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attrs, IEXDD_DEVICE_CONTEXT);
    attrs.EvtCleanupCallback = IexddEvtDeviceContextCleanup;

    status = WdfDeviceCreate(&DeviceInit, &attrs, &device);
    if (!NT_SUCCESS(status)) return status;

    // Adapter creation is in Adapter.c (Task 3)
    return IexddCreateAdapter(device);
}

VOID IexddEvtDeviceContextCleanup(_In_ WDFOBJECT)
{
    // IddCx adapter teardown is automatic via WDF child-object lifetime
}
```

- [ ] **Step 4: Write the INF**

```ini
; iexdd.inf
[Version]
Signature   = "$WINDOWS NT$"
Class       = Display
ClassGuid   = {4d36e968-e325-11ce-bfc1-08002be10318}
Provider    = %ProviderName%
DriverVer   = ; populated by stampinf at build time
CatalogFile = iexdd.cat
PnpLockdown = 1

[DestinationDirs]
DefaultDestDir = 13

[SourceDisksNames]
1 = %DiskName%

[SourceDisksFiles]
iexdd.sys = 1

[Manufacturer]
%ManufacturerName% = Standard, NTamd64.10.0...22000

[Standard.NTamd64.10.0...22000]
%DeviceName% = Iexdd_Install, Root\iExtendDisplay

[Iexdd_Install.NT]
CopyFiles = Iexdd_CopyFiles

[Iexdd_CopyFiles]
iexdd.sys

[Iexdd_Install.NT.Services]
AddService = iexdd, %SPSVCINST_ASSOCSERVICE%, Iexdd_Service

[Iexdd_Service]
DisplayName    = %ServiceName%
ServiceType    = 1   ; SERVICE_KERNEL_DRIVER
StartType      = 3   ; SERVICE_DEMAND_START
ErrorControl   = 1   ; SERVICE_ERROR_NORMAL
ServiceBinary  = %13%\iexdd.sys

[Strings]
SPSVCINST_ASSOCSERVICE = 0x00000002
ProviderName     = "iExtend Project"
ManufacturerName = "iExtend Project"
DiskName         = "iExtend Driver Installation Disk"
DeviceName       = "iExtend Virtual Display"
ServiceName      = "iExtend IDD Service"
```

- [ ] **Step 5: Build the driver**

```cmd
cd host\drivers\windows\iexdd
msbuild iexdd.vcxproj /p:Configuration=Debug /p:Platform=x64 /p:SignMode=TestSign /p:TestCertificate="iExtend Dev"
```

Expected: produces `x64\Debug\iexdd\iexdd.sys` and `iexdd.cat`. Build will succeed even though there's no adapter logic yet — `IexddCreateAdapter` is a forward declaration and will fail at runtime, but the binary links.

- [ ] **Step 6: Hook the build into `xtask`**

In `host/xtask/src/windows_driver.rs`, add a `BuildWindowsDriver` subcommand that shells out to the MSBuild command above. Hook into `host/xtask/src/main.rs`. Verify with `cargo xtask build-windows-driver --debug`.

- [ ] **Step 7: Commit**

```bash
git add host/drivers/windows/iexdd/{Driver.h,Driver.c,Public.h,iexdd.inf,iexdd.vcxproj,iexdd.rc} host/xtask
git commit -m "feat(windows): scaffold iexdd driver project (DriverEntry only, no adapter logic yet)"
```

---

## Task 3: IddCx adapter init + monitor mode list

**Files:**
- Create: `host/drivers/windows/iexdd/Adapter.h`
- Create: `host/drivers/windows/iexdd/Adapter.c`
- Create: `host/drivers/windows/iexdd/Edid.h`
- Create: `host/drivers/windows/iexdd/Edid.c`

The adapter is the parent of the monitor. WDDM treats the adapter as a virtual GPU; the monitor is what shows up in `ms-settings:display`. We advertise exactly one monitor with three modes.

- [ ] **Step 1: Implement `IexddCreateAdapter`**

```c
// Adapter.c (excerpt)
#include "Driver.h"
#include "Adapter.h"

static IDDCX_ADAPTER_CAPS s_AdapterCaps = {
    .Size                  = sizeof(IDDCX_ADAPTER_CAPS),
    .MaxMonitorsSupported  = 1,
    .EndPointDiagnostics   = {
        .Size              = sizeof(IDDCX_ENDPOINT_DIAGNOSTIC_INFO),
        .GenericAdapter    = IDDCX_TRANSMISSION_TYPE_OTHER,
        .TransmissionType  = IDDCX_TRANSMISSION_TYPE_WIRED_OTHER,
    },
};

NTSTATUS IexddCreateAdapter(WDFDEVICE device)
{
    IDDCX_ADAPTER_CONFIG cfg = {0};
    cfg.Size = sizeof(cfg);
    cfg.EvtIddCxAdapterInitFinished = IexddEvtAdapterInitFinished;
    cfg.EvtIddCxAdapterCommitModes  = IexddEvtAdapterCommitModes;
    cfg.AdapterCaps = s_AdapterCaps;

    IDDCX_ADAPTER adapter;
    NTSTATUS status = IddCxAdapterInitAsync(device, &cfg, &adapter);
    if (!NT_SUCCESS(status)) return status;

    PIEXDD_DEVICE_CONTEXT ctx = IexddDeviceGetContext(device);
    ctx->Adapter = adapter;
    return STATUS_SUCCESS;
}

VOID IexddEvtAdapterInitFinished(IDDCX_ADAPTER adapter, const IDARG_OUT_ADAPTER_INIT_FINISHED* args)
{
    if (!NT_SUCCESS(args->Status)) return;

    // Attach exactly one monitor — the rest of the work is in MonitorPlugin.
    UCHAR edid[128];
    IexddBuildEdid(edid, "iExtend Virtual Display");

    IDDCX_MONITOR_INFO info = {
        .Size              = sizeof(info),
        .MonitorType       = IDDCX_MONITOR_TYPE_LCD,
        .ConnectorIndex    = 0,
        .MonitorDescription = {
            .Size = sizeof(IDDCX_MONITOR_DESCRIPTION),
            .Type = IDDCX_MONITOR_DESCRIPTION_TYPE_EDID,
            .DataSize = sizeof(edid),
            .pData    = edid,
        },
    };
    info.MonitorContainerId = IEXDD_MONITOR_CONTAINER_GUID;

    IDARG_IN_MONITORCREATE create = { .ObjectAttributes = WDF_NO_OBJECT_ATTRIBUTES, .MonitorInfo = info };
    IDARG_OUT_MONITORCREATE created;
    if (!NT_SUCCESS(IddCxMonitorCreate(adapter, &create, &created))) return;
    (void)IddCxMonitorArrival(created.MonitorObject); // tells WDDM the monitor is connected
}
```

- [ ] **Step 2: Implement the mode list in `EvtAdapterCommitModes`**

Three modes: 1920×1080@120, 2560×1440@120, 3840×2160@60. Mode list reported in `IDDCX_TARGET_MODE` arrays from `EvtAdapterQueryTargetModes`. (Exact enumeration code: see WDK sample `IddSampleApp` — 35 lines, copy verbatim with our resolutions substituted.)

- [ ] **Step 3: Synthesize EDID in `Edid.c`**

128-byte standard EDID 1.4 block. Manufacturer ID `IXT` (custom; not registered with VESA — fine for an indirect display, but document this in `Edid.h`). Product code 0x0001. Serial 0x12345678. Established timings: VGA 640×480 only (rest are reported as detailed timings). Detailed timing 1: 1920×1080@120. Detailed timing 2: 2560×1440@120. Detailed timing 3 (range limits): 24–75 Hz vertical, 30–135 kHz horizontal. Final byte: checksum (sum of all 128 bytes mod 256 must equal 0).

EDID generation is well-documented; the exact byte layout is in VESA E-EDID 1.4. Hand-encode it in `Edid.c` as a `static const UCHAR g_BaseEdid[128]` with a runtime checksum patch in `IexddBuildEdid`.

- [ ] **Step 4: Test-deploy and verify the monitor appears**

In an elevated cmd:

```cmd
cargo xtask build-windows-driver --debug
pnputil /add-driver host\drivers\windows\iexdd\x64\Debug\iexdd\iexdd.inf /install
devcon install host\drivers\windows\iexdd\x64\Debug\iexdd\iexdd.inf Root\iExtendDisplay
```

Open `ms-settings:display`. **Expected:** a "Display 2" tile appears, named "iExtend Virtual Display." It will show as 1080p120 by default.

If the monitor doesn't show up, the most common causes are: (a) test signing not enabled, (b) EDID checksum wrong, (c) `IddCxMonitorArrival` not called. Check the kernel debug log via `Trace.h` WPP.

- [ ] **Step 5: Commit**

```bash
git add host/drivers/windows/iexdd/{Adapter.h,Adapter.c,Edid.h,Edid.c}
git commit -m "feat(windows): IddCx adapter init, EDID synthesis, virtual monitor advertised to WDDM"
```

---

## Task 4: Swap-chain assignment + no-op producer thread

**Files:**
- Create: `host/drivers/windows/iexdd/SwapChain.h`
- Create: `host/drivers/windows/iexdd/SwapChain.c`
- Modify: `host/drivers/windows/iexdd/Adapter.c` (wire `EvtIddCxMonitorAssignSwapChain`)

This task makes Windows actually *render* into the virtual monitor. We just acquire and release frames in a tight loop without consuming them; the goal is to confirm the swap-chain plumbing works.

- [ ] **Step 1: Implement `EvtIddCxMonitorAssignSwapChain`**

```c
// SwapChain.c (excerpt)
NTSTATUS IexddSwapChainStart(IDDCX_MONITOR monitor, IDDCX_SWAPCHAIN swapChain, LUID renderAdapterLuid)
{
    PIEXDD_MONITOR_CONTEXT mctx = IexddMonitorGetContext(monitor);
    mctx->SwapChain         = swapChain;
    mctx->RenderAdapterLuid = renderAdapterLuid;
    KeInitializeEvent(&mctx->TerminateEvent, NotificationEvent, FALSE);

    HANDLE thread;
    NTSTATUS status = PsCreateSystemThread(&thread, THREAD_ALL_ACCESS, NULL, NULL, NULL,
                                           IexddProcessingThread, mctx);
    if (!NT_SUCCESS(status)) return status;

    ObReferenceObjectByHandle(thread, THREAD_ALL_ACCESS, NULL, KernelMode,
                              (PVOID*)&mctx->ProcessingThread, NULL);
    ZwClose(thread);
    return STATUS_SUCCESS;
}

VOID IexddProcessingThread(PVOID ctx)
{
    PIEXDD_MONITOR_CONTEXT mctx = (PIEXDD_MONITOR_CONTEXT)ctx;
    KeSetSystemAffinityThread((KAFFINITY)1); // pin to P-core 0
    KeSetBasePriorityThread(KeGetCurrentThread(), 8); // above-normal

    for (;;) {
        if (KeReadStateEvent(&mctx->TerminateEvent)) break;

        IDARG_IN_RELEASEANDACQUIREBUFFER acq = { .WaitTimeoutInMs = 16 }; // ~60 Hz min
        IDARG_OUT_RELEASEANDACQUIREBUFFER got;
        NTSTATUS s = IddCxSwapChainReleaseAndAcquireBuffer(mctx->SwapChain, &acq, &got);
        if (s == STATUS_PENDING) continue;
        if (!NT_SUCCESS(s)) break;

        // For Task 4 we just hand the frame back to WDDM immediately.
        // Task 5 will queue it for user-mode pickup before releasing.
        IDARG_IN_FINISHEDFRAMENOTIFY done = { .MetaData = got.MetaData };
        IddCxSwapChainFinishedProcessingFrame(mctx->SwapChain, &done);
    }

    PsTerminateSystemThread(STATUS_SUCCESS);
}
```

- [ ] **Step 2: Build, deploy, exercise**

```cmd
cargo xtask build-windows-driver --debug
cargo xtask install-windows-driver
```

Open Display Settings. Right-click the iExtend monitor → "Extend desktop." Move a window onto the iExtend monitor. The window should appear there (Windows is composing it; we just ack the frames). Confirm via `tlist` or Process Explorer that `dwm.exe` is producing frames at ~60–120 Hz.

A `WPP_PRINT` trace from `IexddProcessingThread` should fire on every frame. If it doesn't, the swap-chain isn't being assigned — check that `EvtIddCxMonitorAssignSwapChain` is registered in `Adapter.c`.

- [ ] **Step 3: Commit**

```bash
git add host/drivers/windows/iexdd/{SwapChain.h,SwapChain.c,Adapter.c}
git commit -m "feat(windows): swap-chain processing thread, frames acked immediately (no consumer yet)"
```

---

## Task 5: Inverted-call IOCTL pipe, kernel side

**Files:**
- Modify: `host/drivers/windows/iexdd/Driver.c` (file-object cleanup)
- Modify: `host/drivers/windows/iexdd/SwapChain.c` (queue, IOCTL pump)
- Create: `host/drivers/windows/iexdd/IoQueue.h`
- Create: `host/drivers/windows/iexdd/IoQueue.c`

"Inverted call" means user-mode posts a `IOCTL_IEXDD_PULL_FRAME` request and waits; the driver completes it from the producer thread when a frame is ready. This is the standard kernel→user notification pattern; cleaner than ALPC and zero-copy-friendly because the IRP buffer can carry the shared-handle directly.

- [ ] **Step 1: Create a manual-dispatch WDF queue for pull requests**

```c
// IoQueue.c (excerpt)
NTSTATUS IexddCreatePullQueue(WDFDEVICE device, WDFQUEUE* outQueue)
{
    WDF_IO_QUEUE_CONFIG cfg;
    WDF_IO_QUEUE_CONFIG_INIT(&cfg, WdfIoQueueDispatchManual);
    cfg.PowerManaged = WdfFalse;
    return WdfIoQueueCreate(device, &cfg, WDF_NO_OBJECT_ATTRIBUTES, outQueue);
}
```

The processing thread, when it has a frame ready, calls `WdfIoQueueRetrieveNextRequest` and completes that IRP with the populated `IEXDD_FRAME_HEADER`.

- [ ] **Step 2: Duplicate the shared handle into the user PID**

Inside `IexddProcessingThread`, after `IddCxSwapChainReleaseAndAcquireBuffer` returns a buffer:

```c
// got.Buffer.MetaData.SystemBuffer is the IDXGIResource1 backing the frame.
// IddCx already gave us a HANDLE to a shared NT object; we duplicate it into the
// client's process so user-mode can OpenSharedResource1 on it directly.
HANDLE userHandle = NULL;
NTSTATUS s = ZwDuplicateObject(NtCurrentProcess(),
                               got.MetaData.PreparedBuffer,
                               clientProcessHandle,
                               &userHandle,
                               0, 0, DUPLICATE_SAME_ACCESS);
```

`clientProcessHandle` is opened once at `IOCTL_IEXDD_HELLO` time from the `ClientPid` field, kept in the file-object context, closed in `EvtFileCleanup`.

- [ ] **Step 3: Wire the dirty-rect array**

`got.MetaData.DirtyRectsBuffer` is an `IDDCX_METADATA_*` array. Convert to flat `RECT[]` and append to the IRP buffer after the `IEXDD_FRAME_HEADER`. Cap at 16 dirty rects per frame; if more, send `DirtyRectCount = 0` (force full-frame encode downstream).

- [ ] **Step 4: Acknowledge the frame back to WDDM only after user-mode releases it**

Add a simple lookup: `AcquireSeq → got.MetaData.PresentationFrameNumber`. When `IOCTL_IEXDD_RELEASE_FRAME` arrives with that `AcquireSeq`, call `IddCxSwapChainFinishedProcessingFrame`. This is the back-pressure mechanism: user-mode can hold up to N frames before WDDM stops handing us new ones.

Cap the in-flight set at 4 frames. If user-mode is slower than that, drop the *oldest* frame (acknowledge it ourselves) so WDDM keeps making progress; bump a "drops" counter visible in user-mode telemetry.

- [ ] **Step 5: Commit**

```bash
git add host/drivers/windows/iexdd/{Driver.c,SwapChain.c,IoQueue.h,IoQueue.c}
git commit -m "feat(windows): inverted-call IOCTL pipe, shared-handle hand-off, in-flight cap=4"
```

---

## Task 6: Rust user-mode partner — `ix-display-windows`

**Files:**
- Modify: `host/crates/ix-display-windows/Cargo.toml`
- Create: `host/crates/ix-display-windows/build.rs`
- Create: `host/crates/ix-display-windows/wrapper.h`
- Create: `host/crates/ix-display-windows/src/{lib.rs,ioctl.rs,frame.rs,error.rs}`

- [ ] **Step 1: `Cargo.toml`**

```toml
[package]
name    = "ix-display-windows"
version = "0.0.1"
edition = "2021"

[target.'cfg(windows)'.dependencies]
windows = { version = "0.56", features = [
    "Win32_Foundation", "Win32_Graphics_Direct3D11", "Win32_Graphics_Dxgi",
    "Win32_System_IO", "Win32_System_Threading", "Win32_Security",
] }
tokio     = { version = "1.36", features = ["sync", "rt", "macros"] }
thiserror = "1"
tracing   = "0.1"

[target.'cfg(windows)'.build-dependencies]
bindgen = "0.69"
```

- [ ] **Step 2: `build.rs` — bindgen against `Public.h`**

```rust
fn main() {
    let header = "../../drivers/windows/iexdd/Public.h";
    println!("cargo:rerun-if-changed={header}");
    let bindings = bindgen::Builder::default()
        .header(header)
        .clang_arg("-I").clang_arg(std::env::var("WindowsSdkDir").unwrap_or_default() + "/Include/10.0.26100.0/shared")
        .allowlist_type("IEXDD_.*")
        .allowlist_var("IOCTL_IEXDD_.*")
        .allowlist_var("IEXDD_PROTOCOL_VERSION")
        .derive_default(true)
        .generate()
        .expect("bindgen Public.h");
    bindings.write_to_file(std::env::var("OUT_DIR").unwrap() + "/iexdd_sys.rs").unwrap();
}
```

- [ ] **Step 3: `lib.rs` — public API**

```rust
//! Windows IddCx user-mode partner. One [`VirtualDisplay`] per virtual monitor.
//!
//! Lifetime is tied to the device handle; dropping it tells the driver to stop
//! producing frames. Frames flow through the [`tokio::sync::mpsc::Receiver`]
//! returned by [`VirtualDisplay::frames`].

use std::path::Path;
use tokio::sync::mpsc;

mod ioctl;
mod frame;
mod error;

pub use error::{Error, Result};
pub use frame::GpuFrame;

const PIPE_NAME: &str = r"\\.\IExtendDisplay";

pub struct VirtualDisplay {
    inner: ioctl::Connection,
}

impl VirtualDisplay {
    /// Open the virtual display device and exchange protocol-version handshake.
    /// Fails with [`Error::DriverNotInstalled`] if the driver isn't loaded.
    pub fn open() -> Result<Self> {
        let inner = ioctl::Connection::open(Path::new(PIPE_NAME))?;
        inner.hello()?;
        Ok(Self { inner })
    }

    /// Spawn the pull loop and return the frame receiver. Buffer capacity is 4
    /// (matches the kernel-side in-flight cap).
    pub fn frames(self) -> mpsc::Receiver<GpuFrame> {
        let (tx, rx) = mpsc::channel(4);
        std::thread::Builder::new()
            .name("iextend-display-pull".into())
            .spawn(move || self.inner.run(tx))
            .expect("spawn pull thread");
        rx
    }
}
```

- [ ] **Step 4: `ioctl.rs` — pull loop**

The pull loop is dead-simple synchronous I/O on a dedicated thread. We don't need IOCP overlapped here — one outstanding pull at a time is exactly the back-pressure semantics we want. Each iteration:

1. `DeviceIoControl(handle, IOCTL_IEXDD_PULL_FRAME, ...)` — blocks until the kernel completes a frame.
2. Parse the IRP buffer into `IEXDD_FRAME_HEADER + RECT[]`.
3. Call `D3D11Device::OpenSharedResource1` on the duplicated handle to get an `ID3D11Texture2D`.
4. Wrap into `GpuFrame { texture, dirty, present_time }`.
5. `tx.blocking_send(frame)` — if the consumer dropped, the thread exits cleanly.
6. After the consumer drops the `GpuFrame`, the `Drop` impl posts `IOCTL_IEXDD_RELEASE_FRAME` (so the kernel can ack the frame back to WDDM).

If `tx.blocking_send` blocks for >16 ms, log a `tracing::warn!` — that's a sign the encoder thread is starved.

- [ ] **Step 5: `frame.rs`**

```rust
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use std::time::Duration;

pub struct GpuFrame {
    pub texture:      ID3D11Texture2D,
    pub dirty_rects:  Vec<windows::Win32::Foundation::RECT>,
    pub present_time: Duration, // from boot, monotonic
    pub width:        u32,
    pub height:       u32,
    pub(crate) seq:   u64,
    pub(crate) conn:  std::sync::Weak<crate::ioctl::ReleaseChannel>,
}

impl Drop for GpuFrame {
    fn drop(&mut self) {
        if let Some(c) = self.conn.upgrade() {
            let _ = c.release(self.seq); // best-effort; if disconnected, kernel will GC on file-handle close
        }
    }
}
```

- [ ] **Step 6: Integration test against the real driver**

```rust
// host/crates/ix-display-windows/tests/smoke.rs
#[tokio::test(flavor = "multi_thread")]
async fn captures_real_frames_for_5_seconds() {
    let vd = ix_display_windows::VirtualDisplay::open().expect("driver installed?");
    let mut rx = vd.frames();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut got = 0u64;
    while std::time::Instant::now() < deadline {
        if let Some(frame) = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await.ok().flatten() {
            assert!(frame.width >= 1280 && frame.height >= 720);
            got += 1;
        }
    }
    assert!(got >= 60, "expected at least 60 frames in 5s, got {got}");
}
```

Run on a machine with the test-signed driver installed and the virtual monitor extended. Bar: 60+ frames in 5 s (= ≥12 fps; not the latency target, just "the pipe works").

- [ ] **Step 7: Commit**

```bash
git add host/crates/ix-display-windows
git commit -m "feat(windows): ix-display-windows user-mode partner; smoke test ≥60 frames in 5s"
```

---

## Task 7: Test-signing setup, scripted

**Files:**
- Create: `host/xtask/src/windows_signing.rs`
- Modify: `host/xtask/src/main.rs`

The dev loop right now is "build → manually re-sign → manually re-install via `pnputil` → manually trigger devcon update." Bake all of that into `cargo xtask`.

- [ ] **Step 1: `cargo xtask sign-driver-test`**

Wraps `signtool sign /v /s PrivateCertStore /n "iExtend Dev" /t http://timestamp.digicert.com iexdd.sys iexdd.cat`. Expects the test cert from Task 1 Step 5 to be present. Refuses to run on machines where `bcdedit | findstr testsigning` doesn't show `Yes`.

- [ ] **Step 2: `cargo xtask install-windows-driver`**

Composes `pnputil /add-driver iexdd.inf /install` + `devcon update iexdd.inf Root\iExtendDisplay`. Includes a `--reload` flag that does `devcon remove` + `pnputil /delete-driver` first — useful when iterating on the INF.

- [ ] **Step 3: `cargo xtask trace`**

Starts a WPP trace session against the running driver and writes to `iexdd.etl`. Uses `tracelog -start iExtend -guid iexdd.ctl -f iexdd.etl`. The matching `tracefmt` invocation produces a human-readable log. Document the GUID in `host/drivers/windows/iexdd/Trace.h`.

- [ ] **Step 4: Commit**

```bash
git add host/xtask/src/{windows_signing.rs,main.rs}
git commit -m "build(windows): xtask subcommands for sign/install/trace, dev loop scripted end-to-end"
```

---

## Task 8: WHQL submission flow — documented, not automated

**Files:**
- Create: `docs/windows-whql.md`

WHQL is interactive — Microsoft Partner Center upload, attestation file generation, signed catalog download. Automating it isn't a v1 win. Documenting it carefully *is*, because the next person to do a release shouldn't have to rediscover the steps.

- [ ] **Step 1: Write `docs/windows-whql.md`**

Cover, in order:

1. **Microsoft Partner Center onboarding** — link to `https://partner.microsoft.com/`. Requires an EV cert (proven by signing a small file Microsoft hosts). Onboarding is one-time per company; budget ~3 business days for the first time.
2. **HLK setup** — install the Hardware Lab Kit (HLK) Studio + Controller on a fresh Windows 11 machine; install the matching HLK Client on the test target (a separate machine — virtual is fine for signed-driver testing). The version of HLK *must* match the target Windows version exactly.
3. **Run the IDD certification tests.** Run the `Indirect Display Driver Certification` set against `iexdd.sys`. Typical run time ~3 hours. Allow re-runs for transient failures.
4. **Generate `.hlkx` package.** HLK Studio produces a single archive with the test results + the driver binaries.
5. **Upload to Partner Center → Hardware → Drivers.** Select target audiences (Windows 11 24H2 client). Wait for "Signed" status — typically 24–72 hours.
6. **Download the signed catalog** and replace `iexdd.cat` in the release artifact. Re-package for distribution.

7. **What can go wrong** — append a debugging matrix:
   - "Driver does not load" → bcdedit testsigning + cert presence check
   - "WHQL test fails on multi-monitor" → expected; we advertise one monitor only — file an exception waiver via Partner Center.
   - "Signing service rejects the catalog" — almost always a missing or wrong target audience selection.

- [ ] **Step 2: Run a real submission as a smoke test**

The first submission of an unsigned `iexdd.sys` won't load on production Windows machines. Submit it anyway with target audience "Windows 11 — Internal" so the team can install on their non-test-signed dev boxes. ~$0 cost; just calendar time. **Do not skip this step** — discovering a Partner Center config issue 4 weeks before ship is much worse than discovering it now.

- [ ] **Step 3: Commit**

```bash
git add docs/windows-whql.md
git commit -m "docs(windows): WHQL submission walkthrough + first-pass smoke checklist"
```

---

## Task 9: End-to-end smoke test — capture real frames, verify pixels

**Files:**
- Create: `host/crates/ix-display-windows/examples/dump_frames.rs`

Confirms the captured `ID3D11Texture2D` actually contains the on-screen pixels — not zero-filled, not stale, not from the wrong monitor.

- [ ] **Step 1: Implement `dump_frames`**

```rust
//! cargo run -p ix-display-windows --example dump_frames -- 5
//!
//! Captures `<n>` frames spread across 1 second from the virtual monitor and
//! writes them as `frame_NN.dds` files. DDS is what NVIDIA Texture Tools / Photoshop
//! / Paint.NET can open directly — no encoder dependency.

use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let n: usize = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(5);
    let vd = ix_display_windows::VirtualDisplay::open()?;
    let mut rx = vd.frames();
    let stride = Duration::from_millis(1000 / n as u64);
    let mut next = Instant::now();
    let mut idx = 0usize;
    while idx < n {
        let frame = rx.recv().await.ok_or_else(|| anyhow::anyhow!("driver disconnected"))?;
        if Instant::now() >= next {
            ix_display_windows::dds::save(&frame, format!("frame_{:02}.dds", idx))?;
            idx += 1;
            next += stride;
        }
    }
    Ok(())
}
```

Add a small `dds.rs` helper that reads the texture back to system memory via a staging texture and writes a DXT1-compressed DDS. ~120 lines; copy from a public-domain DDS-write reference.

- [ ] **Step 2: Run the end-to-end exercise**

1. Boot a clean Windows 11 24H2 VM with test signing on.
2. `cargo xtask install-windows-driver`.
3. Open Display Settings, extend desktop to the iExtend monitor.
4. Drag a Notepad window onto the iExtend monitor, type "iExtend smoke test."
5. `cargo run -p ix-display-windows --example dump_frames -- 5`.
6. Open `frame_00.dds` through `frame_04.dds` in Paint.NET. Confirm Notepad with the typed text is visible. The mouse cursor *will not* be in the frame — IddCx renders the cursor separately; we'll wire that up in Plan 8 (cursor reprojection).

- [ ] **Step 3: Commit**

```bash
git add host/crates/ix-display-windows/examples/dump_frames.rs host/crates/ix-display-windows/src/dds.rs
git commit -m "test(windows): dump_frames example; end-to-end captures verified pixel-correct"
```

- [ ] **Step 4: Tag**

```bash
git tag -a plan-3-complete -m "Plan 3 of 10 complete: Windows IddCx driver shipping real frames to Rust user-mode"
```

---

## Done criteria

All must hold for Plan 3 to be considered shipped:

1. `iexdd.sys` builds clean (`cargo xtask build-windows-driver`).
2. After `cargo xtask install-windows-driver`, `ms-settings:display` shows "iExtend Virtual Display."
3. `cargo test -p ix-display-windows` passes (the 5-second smoke test).
4. `cargo run -p ix-display-windows --example dump_frames -- 5` produces 5 valid DDS files containing on-screen pixels.
5. `docs/windows-whql.md` exists with the submission walkthrough.
6. A real submission has been pushed to Microsoft Partner Center with target audience "Windows 11 — Internal" (catalog status: Signed or In Progress).
7. Tag `plan-3-complete` is on the head commit.

## Out of scope (handled by later plans)

- Encoder integration (Plan 5)
- Cursor capture / reprojection (Plan 8)
- Codesigning of release artifacts via the production EV cert + automated WHQL submission (Plan 9)
- HDR pipeline (Plan 5 turns it on; the IddCx driver already supports 10-bit modes)
- Audio routing (Plan 11+ — explicitly deferred past v1)

## Recommendations for the implementer

- **Bring up the kernel debugger early.** `windbg -k net:port=50000,key=...` against a target VM is non-negotiable for IddCx development; trying to debug from `printk`-equivalent traces alone will burn weeks. Microsoft's "Debugging Tools for Windows" is in the WDK install.
- **Don't optimize before Task 6 lands.** The processing thread looks like a hot path, and it is, but the latency budget is dominated by the encoder (Plan 5), not capture. Prove it works before tuning.
- **Test on at least three GPU vendors.** NVIDIA, Intel iGPU, AMD. NVIDIA is usually the smoothest path; Intel iGPU has historically had IddCx-specific quirks around shared-handle creation; AMD is generally fine but worth confirming. CI hardware (Plan 10) covers all three.
- **The Partner Center smoke-submission in Task 8 Step 2 is the most underrated step in this plan.** Every team that ships a kernel driver discovers Partner Center problems exactly when they can least afford to. Push a useless submission early; you'll be glad you did.
