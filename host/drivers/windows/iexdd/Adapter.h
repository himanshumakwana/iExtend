// Adapter.h — IddCx adapter + virtual monitor declarations.
//
// The adapter is the WDDM "GPU" object. We create exactly one monitor child in
// EvtAdapterInitFinished and never remove it during the life of the device. Mode
// changes are handled by EvtAdapterCommitModes.

#pragma once

#include "Driver.h"
#include "Edid.h"

EXTERN_C_START

// Called by Driver.c during EvtDeviceAdd to create the IddCx adapter object.
NTSTATUS IexddCreateAdapter(_In_ WDFDEVICE Device);

// IddCx callbacks registered in IexddCreateAdapter.
EVT_IDD_CX_ADAPTER_INIT_FINISHED    IexddEvtAdapterInitFinished;
EVT_IDD_CX_ADAPTER_COMMIT_MODES     IexddEvtAdapterCommitModes;
EVT_IDD_CX_PARSE_MONITOR_DESCRIPTION IexddEvtParseMonitorDescription;
EVT_IDD_CX_MONITOR_GET_DEFAULT_DESCRIPTION_MODES IexddEvtMonitorGetDefaultModes;
EVT_IDD_CX_MONITOR_QUERY_TARGET_MODES IexddEvtMonitorQueryTargetModes;
EVT_IDD_CX_MONITOR_ASSIGN_SWAPCHAIN  IexddEvtMonitorAssignSwapChain;
EVT_IDD_CX_MONITOR_UNASSIGN_SWAPCHAIN IexddEvtMonitorUnassignSwapChain;

// Per-monitor context — stored on the IDDCX_MONITOR WDF object.
typedef struct _IEXDD_MONITOR_CONTEXT {
    IDDCX_MONITOR       MonitorObject;
    IDDCX_SWAPCHAIN     SwapChain;
    LUID                RenderAdapterLuid;   // GPU that owns the swapchain resources
    PETHREAD            ProcessingThread;    // kernel thread running IexddProcessingThread
    KEVENT              TerminateEvent;      // set to ask the thread to exit
    WDFDEVICE           OwnerDevice;         // back-pointer for PullQueue access
} IEXDD_MONITOR_CONTEXT, *PIEXDD_MONITOR_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(IEXDD_MONITOR_CONTEXT, IexddMonitorGetContext)

// Three fixed display modes advertised on the virtual monitor.
// 1920x1080 @120 Hz, 2560x1440 @120 Hz, 3840x2160 @60 Hz.
#define IEXDD_MODE_COUNT  3

extern const IDDCX_TARGET_MODE g_SupportedModes[IEXDD_MODE_COUNT];

EXTERN_C_END
