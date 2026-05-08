// IoQueue.h — WDF queue declarations for HELLO, PULL_FRAME, RELEASE_FRAME.
//
// Two queues:
//  DefaultQueue — parallel-dispatch; handles HELLO, RELEASE, QUERY_STATS.
//  PullQueue    — manual-dispatch;  holds outstanding PULL_FRAME IRPs until
//                 the processing thread in SwapChain.c completes them.

#pragma once
#include "Driver.h"

EXTERN_C_START

// Create the parallel-dispatch default queue and register IOCTL handlers.
NTSTATUS IexddCreateDefaultQueue(
    _In_  WDFDEVICE  Device,
    _Out_ WDFQUEUE  *pQueue
);

// Create the manual-dispatch pull queue (no callbacks — thread drives it).
NTSTATUS IexddCreatePullQueue(
    _In_  WDFDEVICE  Device,
    _Out_ WDFQUEUE  *pQueue
);

// IOCTL handlers dispatched by the default queue.
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL IexddEvtIoDeviceControl;

EXTERN_C_END
