// Public.h — kernel/user-mode IPC contract for iexdd. Bumping IEXDD_PROTOCOL_VERSION
// is required for any layout change; old user-mode partners will refuse to connect.
//
// This header is included by:
//   * The driver's C source (kernel mode)
//   * `host/crates/ix-display-windows/build.rs` via bindgen (user mode, Rust)
//
// All structs are POD with explicit widths; no compiler-inserted padding via #pragma
// pack(8) for 64-bit alignment of HANDLE/UINT64 fields. Do not add fields without
// bumping IEXDD_PROTOCOL_VERSION; user-mode hard-fails the handshake on mismatch.

#pragma once

#ifdef _KERNEL_MODE
    #include <wdm.h>
    #include <devioctl.h>
#else
    #include <windows.h>
    #include <winioctl.h>
#endif

#define IEXDD_PROTOCOL_VERSION 1

// Custom device type from the user-allocated range (0x8000-0xFFFF). The Microsoft
// docs reserve 0-0x7FFF for vendor-coordinated IHV use; we use 0x9D00 ("iD"isplay).
#define IEXDD_DEVICE_TYPE 0x9D00

#define IOCTL_IEXDD_HELLO \
    CTL_CODE(IEXDD_DEVICE_TYPE, 0x800, METHOD_BUFFERED, FILE_ANY_ACCESS)

#define IOCTL_IEXDD_PULL_FRAME \
    CTL_CODE(IEXDD_DEVICE_TYPE, 0x801, METHOD_BUFFERED, FILE_ANY_ACCESS)

#define IOCTL_IEXDD_RELEASE_FRAME \
    CTL_CODE(IEXDD_DEVICE_TYPE, 0x802, METHOD_BUFFERED, FILE_ANY_ACCESS)

#define IOCTL_IEXDD_QUERY_STATS \
    CTL_CODE(IEXDD_DEVICE_TYPE, 0x803, METHOD_BUFFERED, FILE_ANY_ACCESS)

// Maximum dirty rectangles delivered per frame. If the WDDM frame metadata reports
// more than this, the driver sets DirtyRectCount = 0 (forcing a full-frame encode
// downstream) rather than truncating arbitrarily.
#define IEXDD_MAX_DIRTY_RECTS 16

// Maximum frames the user-mode partner may hold without releasing before the
// driver starts dropping the oldest unacked frame. This is the back-pressure
// envelope; chosen to be 4 because at 120 Hz that's ~33 ms of slack — enough to
// absorb a GC pause or scheduler hiccup, short enough to keep latency bounded.
#define IEXDD_MAX_INFLIGHT_FRAMES 4

// ----------------------------------------------------------------------------
// IOCTL_IEXDD_HELLO — protocol-version handshake.
// ----------------------------------------------------------------------------
// Input: IEXDD_HELLO. Output: IEXDD_HELLO with ProtocolVersion echoed.
// Driver opens the user process by ClientPid (PROCESS_DUP_HANDLE) and stores the
// handle in the file-object context for use during shared-handle duplication
// in IOCTL_IEXDD_PULL_FRAME.

typedef struct _IEXDD_HELLO {
    UINT32 ProtocolVersion;   // must equal IEXDD_PROTOCOL_VERSION
    UINT32 ClientPid;         // process ID of the user-mode partner
    GUID   ClientNonce;       // user-mode-generated; echoed back so user-mode can match calls
} IEXDD_HELLO, *PIEXDD_HELLO;

// ----------------------------------------------------------------------------
// IOCTL_IEXDD_PULL_FRAME — inverted-call pull.
// ----------------------------------------------------------------------------
// Input: empty. Output: IEXDD_FRAME_HEADER followed by DirtyRectCount * RECT.
// The IRP is held in a manual WDF queue; the processing thread completes it when
// a swapchain buffer becomes available. User-mode is expected to keep one PULL
// outstanding at all times; the driver never spontaneously delivers a frame.

typedef struct _IEXDD_FRAME_HEADER {
    UINT64 PresentTimeQpc;       // QueryPerformanceCounter at vsync acquire
    UINT64 AcquireSeq;           // monotonic per-monitor; gaps indicate dropped frames
    HANDLE SharedTextureHandle;  // NT handle from CreateSharedHandle, duplicated into ClientPid
    UINT32 Width;                // frame width in pixels
    UINT32 Height;               // frame height in pixels
    UINT32 DirtyRectCount;       // 0 means "full frame" (or count > IEXDD_MAX_DIRTY_RECTS truncated)
    UINT32 ColorSpaceId;         // DXGI_COLOR_SPACE_TYPE; e.g. RGB_FULL_G22_NONE_P709 = 0
    // followed by DirtyRectCount * RECT in the same buffer
} IEXDD_FRAME_HEADER, *PIEXDD_FRAME_HEADER;

// ----------------------------------------------------------------------------
// IOCTL_IEXDD_RELEASE_FRAME — back-pressure ack.
// ----------------------------------------------------------------------------
// Input: IEXDD_FRAME_RELEASE.
// User-mode posts this once it has finished consuming a frame's shared texture.
// The driver looks up the AcquireSeq, calls IddCxSwapChainFinishedProcessingFrame
// for that frame, and decrements its in-flight counter. Releasing an unknown
// AcquireSeq is silently ignored (in case user-mode releases out of order under
// races during shutdown).

typedef struct _IEXDD_FRAME_RELEASE {
    UINT64 AcquireSeq;
} IEXDD_FRAME_RELEASE, *PIEXDD_FRAME_RELEASE;

// ----------------------------------------------------------------------------
// IOCTL_IEXDD_QUERY_STATS — telemetry pull.
// ----------------------------------------------------------------------------
// Input: empty. Output: IEXDD_STATS.
// Counters are 64-bit and never reset (read-and-diff for rate). Used by the
// `iextend-tray` GUI's diagnostics tab and the soak-test dashboard.

typedef struct _IEXDD_STATS {
    UINT64 FramesAcquired;       // total frames received from WDDM
    UINT64 FramesDelivered;      // frames successfully forwarded to user-mode
    UINT64 FramesDropped;        // frames acked back to WDDM because user-mode was too slow
    UINT64 PullRequestsTotal;    // IOCTL_IEXDD_PULL_FRAME requests received
    UINT64 ReleaseRequestsTotal; // IOCTL_IEXDD_RELEASE_FRAME requests received
    UINT32 InFlightCount;        // current frames held by user-mode (0..IEXDD_MAX_INFLIGHT_FRAMES)
    UINT32 ProtocolVersion;      // echo of IEXDD_PROTOCOL_VERSION
} IEXDD_STATS, *PIEXDD_STATS;

// MonitorContainerId — identifies the virtual monitor for ContainerId-based grouping
// (used by Settings / Devices and Printers UX). Generated once per build of the
// driver; arbitrary GUID is fine.
//
// {7B83A6F1-3D04-4B8E-9A77-1E4DB8F2C6A3}
DEFINE_GUID(IEXDD_MONITOR_CONTAINER_GUID,
    0x7b83a6f1, 0x3d04, 0x4b8e, 0x9a, 0x77, 0x1e, 0x4d, 0xb8, 0xf2, 0xc6, 0xa3);
