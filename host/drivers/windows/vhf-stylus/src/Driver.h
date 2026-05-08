// Driver.h — internal types and forward declarations for the vhf-stylus driver.

#pragma once
#include <ntddk.h>
#include <wdf.h>
#include <vhf.h>
#include "hid_report.h"
#include "Public.h"

// ── Device context ────────────────────────────────────────────────────────────

// Per-device context stored alongside the WDFDEVICE object.
typedef struct _DEVICE_CONTEXT {
    VHFHANDLE   VhfHandle;          // VHF device handle (set in EvtDeviceAdd)
    WDFQUEUE    ReportQueue;        // WDFQUEUE draining IOCTL_IEXTEND_SUBMIT_REPORT
    KSPIN_LOCK  QueueLock;          // protects ReportQueue access
    volatile LONG PendingReports;   // count of buffered reports not yet consumed by VHF
} DEVICE_CONTEXT, *PDEVICE_CONTEXT;

WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(DEVICE_CONTEXT, DeviceGetContext)

// ── Function prototypes ───────────────────────────────────────────────────────

DRIVER_INITIALIZE DriverEntry;

EVT_WDF_DRIVER_DEVICE_ADD EvtDeviceAdd;
EVT_WDF_IO_QUEUE_IO_DEVICE_CONTROL EvtIoDeviceControl;

// VHF callbacks
EVT_VHF_ASYNC_OPERATION EvtVhfAsyncOperationGetInputReport;
EVT_VHF_ASYNC_OPERATION EvtVhfAsyncOperationGetFeatureReport;
EVT_VHF_ASYNC_OPERATION EvtVhfAsyncOperationSetFeatureReport;

// Helpers
NTSTATUS CreateReportQueue(WDFDEVICE Device, PDEVICE_CONTEXT DevCtx);
NTSTATUS RegisterVhfDevice(WDFDEVICE Device, PDEVICE_CONTEXT DevCtx);
VOID     SubmitHidReport(PDEVICE_CONTEXT DevCtx, PUCHAR ReportBuf, SIZE_T ReportLen);
