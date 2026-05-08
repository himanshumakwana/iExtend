// Driver.h — DriverEntry + per-device WDF context.
//
// Driver.c does no work beyond DriverEntry and EvtDriverDeviceAdd; all per-device
// IddCx logic lives in Adapter.c. The intent is that DriverEntry stays small
// enough to audit by inspection, since it's the kernel-load entry point.

#pragma once

#include <ntddk.h>
#include <wdf.h>
#include <iddcx.h>

#include "Public.h"
#include "Trace.h"

EXTERN_C_START

DRIVER_INITIALIZE                  DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD          IexddEvtDeviceAdd;
EVT_WDF_DEVICE_CONTEXT_CLEANUP     IexddEvtDeviceContextCleanup;
EVT_WDF_DEVICE_D0_ENTRY            IexddEvtDeviceD0Entry;
EVT_WDF_DEVICE_D0_EXIT             IexddEvtDeviceD0Exit;
EVT_WDF_FILE_CLEANUP               IexddEvtFileCleanup;

// Forward declaration — implemented in Adapter.c.
NTSTATUS IexddCreateAdapter(_In_ WDFDEVICE Device);

// Per-device state. The IddCx adapter handle is the parent of all monitors; we
// only attach one monitor in v1 but the layout supports MaxMonitorsSupported > 1
// without restructuring.
typedef struct _IEXDD_DEVICE_CONTEXT {
    IDDCX_ADAPTER Adapter;       // set in IexddCreateAdapter; valid for life of WDFDEVICE
    WDFQUEUE      DefaultQueue;  // I/O queue for HELLO / RELEASE / QUERY_STATS (parallel-dispatch)
    WDFQUEUE      PullQueue;     // manual-dispatch queue holding pending PULL_FRAME requests
    KSPIN_LOCK    StatsLock;
    IEXDD_STATS   Stats;
} IEXDD_DEVICE_CONTEXT, *PIEXDD_DEVICE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(IEXDD_DEVICE_CONTEXT, IexddDeviceGetContext)

// Per-file-object state. The user-mode partner opens the device once at startup;
// during HELLO we duplicate ClientPid into ClientProcess. The handle lives until
// EvtFileCleanup, which closes it and tears down any in-flight frames.
typedef struct _IEXDD_FILE_CONTEXT {
    HANDLE ClientProcess;        // PROCESS_DUP_HANDLE access; closed in EvtFileCleanup
    GUID   ClientNonce;
    BOOLEAN HelloComplete;       // refuse PULL/RELEASE before HELLO succeeds
} IEXDD_FILE_CONTEXT, *PIEXDD_FILE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(IEXDD_FILE_CONTEXT, IexddFileGetContext)

EXTERN_C_END
