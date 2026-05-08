// SwapChain.h — per-monitor swap-chain processing thread declarations.
//
// The processing thread lives for the lifetime of a single swapchain assignment.
// It acquires frames from IddCx, duplicates shared texture handles into the
// user-mode partner's process, completes pending PULL_FRAME IRPs, and waits
// for RELEASE_FRAME before acking the frame back to WDDM.

#pragma once
#include "Driver.h"
#include "Adapter.h"

EXTERN_C_START

// Start the per-monitor processing thread and associate it with the swap chain.
// Called from EvtIddCxMonitorAssignSwapChain.
NTSTATUS IexddSwapChainStart(
    _In_ IDDCX_MONITOR  Monitor,
    _In_ IDDCX_SWAPCHAIN SwapChain,
    _In_ LUID           RenderAdapterLuid
);

// Signal the processing thread to exit and wait for it to complete.
// Called from EvtIddCxMonitorUnassignSwapChain.
VOID IexddSwapChainStop(
    _In_ IDDCX_MONITOR Monitor
);

// IddCx callbacks — registered in Adapter.c.
EVT_IDD_CX_MONITOR_ASSIGN_SWAPCHAIN   IexddEvtMonitorAssignSwapChain;
EVT_IDD_CX_MONITOR_UNASSIGN_SWAPCHAIN IexddEvtMonitorUnassignSwapChain;

// Thread function; signature matches PsCreateSystemThread.
KSTART_ROUTINE IexddProcessingThread;

EXTERN_C_END
