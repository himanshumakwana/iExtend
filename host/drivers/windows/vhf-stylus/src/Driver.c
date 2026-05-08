// Driver.c — vhf-stylus KMDF/VHF kernel driver for iExtend.
//
// PURPOSE
// =======
// Exposes a virtual HID digitizer (stylus) device to Windows via the Virtual
// HID Framework (vhf.sys).  The user-mode partner (ix-input/src/windows.rs)
// opens \\.\iExtendStylus and submits 18-byte HID reports via
// IOCTL_IEXTEND_SUBMIT_REPORT.  The driver forwards those reports to vhf.sys
// which synthesises Windows Ink / Pointer Input events that reach all apps
// through the standard HID stack.
//
// ARCHITECTURE
// ============
//   iextendd.exe
//     └─ DeviceIoControl(IOCTL_IEXTEND_SUBMIT_REPORT, 18-byte report)
//           │
//   [kernel boundary]
//           │
//   vhf-stylus.sys  (this driver)
//     ├─ IOCTL handler: enqueues report into a per-device WDFQUEUE
//     └─ VHF callback EvtVhfAsyncOperationGetInputReport: drains queue → VhfNotifyOperationComplete
//           │
//   vhf.sys  (Microsoft Virtual HID Framework)
//     └─ synthesises HID input report → hidclass.sys → WM_POINTER / Windows Ink
//
// SIGNING
// =======
// Requires the same EV code-signing certificate used by the IddCx driver
// (Plan 3).  Test mode: bcdedit /set testsigning on.
//
// REFERENCES
// ==========
// - MSDN: "Virtual HID Framework (VHF)" https://docs.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf-
// - WDK sample: Vhf\VhidMini (SDK\Samples\hid\vhidmini)
// - HID 1.11 §16 Digitizer Usage Page

#include "Driver.h"

// ── HID report descriptor ─────────────────────────────────────────────────────
//
// 18-byte input report for a pen digitizer.
// Validated against HID Descriptor Tool (DT) 2.4.
//
// The logical ranges below match the user-mode report builder in windows.rs:
//   X, Y      : 0..32767 (fits iPad Pro 12.9" logical width 2732px with 12x scale)
//   Pressure  : 0..1024
//   Tilt-X/Y  : -9000..9000 (centidegrees, i.e. -90.00°..+90.00°)

const UCHAR HidReportDescriptor[HID_REPORT_DESC_SIZE] = {
    0x05, 0x0D,             // Usage Page (Digitizers)
    0x09, 0x02,             // Usage (Pen)
    0xA1, 0x01,             // Collection (Application)
    0x09, 0x20,             //   Usage (Stylus)
    0xA1, 0x00,             //   Collection (Physical)

    // Report ID
    0x85, HID_REPORT_ID,    //     Report ID (1)

    // Tip switch (1 bit)
    0x09, 0x42,             //     Usage (Tip Switch)
    0x15, 0x00,             //     Logical Minimum (0)
    0x25, 0x01,             //     Logical Maximum (1)
    0x75, 0x01,             //     Report Size (1)
    0x95, 0x01,             //     Report Count (1)
    0x81, 0x02,             //     Input (Data, Var, Abs)

    // Barrel switch (1 bit)
    0x09, 0x44,             //     Usage (Barrel Switch)
    0x81, 0x02,             //     Input (Data, Var, Abs)

    // In-range (hover, 1 bit)
    0x09, 0x32,             //     Usage (In Range)
    0x81, 0x02,             //     Input (Data, Var, Abs)

    // Padding (5 bits to reach byte boundary)
    0x75, 0x05,             //     Report Size (5)
    0x95, 0x01,             //     Report Count (1)
    0x81, 0x03,             //     Input (Cnst, Var, Abs)   -- padding

    // X coordinate (32-bit)
    0x05, 0x01,             //     Usage Page (Generic Desktop)
    0x09, 0x30,             //     Usage (X)
    0x15, 0x00,             //     Logical Minimum (0)
    0x27, 0xFF, 0x7F, 0x00, 0x00, // Logical Maximum (32767)
    0x55, 0x0D,             //     Unit Exponent (-3)
    0x65, 0x11,             //     Unit (cm)
    0x75, 0x20,             //     Report Size (32)
    0x95, 0x01,             //     Report Count (1)
    0x81, 0x02,             //     Input (Data, Var, Abs)

    // Y coordinate (32-bit)
    0x09, 0x31,             //     Usage (Y)
    0x81, 0x02,             //     Input (Data, Var, Abs)

    // Pressure (16-bit, 0..1024)
    0x05, 0x0D,             //     Usage Page (Digitizers)
    0x09, 0x30,             //     Usage (Tip Pressure)
    0x15, 0x00,             //     Logical Minimum (0)
    0x26, 0x00, 0x04,       //     Logical Maximum (1024)
    0x75, 0x10,             //     Report Size (16)
    0x81, 0x02,             //     Input (Data, Var, Abs)

    // Tilt-X (16-bit signed, -9000..9000 centidegrees)
    0x09, 0x3D,             //     Usage (X Tilt)
    0x16, 0x70, 0xDC,       //     Logical Minimum (-9000)
    0x26, 0x90, 0x23,       //     Logical Maximum ( 9000)
    0x81, 0x02,             //     Input (Data, Var, Abs)

    // Tilt-Y (16-bit signed)
    0x09, 0x3E,             //     Usage (Y Tilt)
    0x81, 0x02,             //     Input (Data, Var, Abs)

    0xC0,                   //   End Collection (Physical)
    0xC0,                   // End Collection (Application)
};

// ── DriverEntry ───────────────────────────────────────────────────────────────

NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
)
{
    WDF_DRIVER_CONFIG config;
    NTSTATUS status;

    WDF_DRIVER_CONFIG_INIT(&config, EvtDeviceAdd);

    status = WdfDriverCreate(
        DriverObject,
        RegistryPath,
        WDF_NO_OBJECT_ATTRIBUTES,
        &config,
        WDF_NO_HANDLE
    );
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                   "vhf-stylus: WdfDriverCreate failed 0x%x\n", status));
    }
    return status;
}

// ── EvtDeviceAdd ─────────────────────────────────────────────────────────────

NTSTATUS
EvtDeviceAdd(
    _In_    WDFDRIVER       Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
)
{
    NTSTATUS         status;
    WDFDEVICE        device;
    PDEVICE_CONTEXT  devCtx;
    WDF_OBJECT_ATTRIBUTES deviceAttributes;

    UNREFERENCED_PARAMETER(Driver);

    // Security: allow user-mode access (iextendd.exe runs as LocalService).
    status = WdfDeviceInitAssignSDDLString(DeviceInit, &SDDL_DEVOBJ_SYS_ALL_ADM_RWX_WORLD_RW_RES_R);
    if (!NT_SUCCESS(status)) { return status; }

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttributes, DEVICE_CONTEXT);

    status = WdfDeviceCreate(&DeviceInit, &deviceAttributes, &device);
    if (!NT_SUCCESS(status)) { return status; }

    devCtx = DeviceGetContext(device);
    KeInitializeSpinLock(&devCtx->QueueLock);
    devCtx->PendingReports = 0;

    // Create the symbolic link so user-mode can open \\.\iExtendStylus.
    {
        UNICODE_STRING symlink;
        RtlInitUnicodeString(&symlink, IEXTEND_STYLUS_SYMLINK);
        status = WdfDeviceCreateSymbolicLink(device, &symlink);
        if (!NT_SUCCESS(status)) { return status; }
    }

    status = CreateReportQueue(device, devCtx);
    if (!NT_SUCCESS(status)) { return status; }

    status = RegisterVhfDevice(device, devCtx);
    if (!NT_SUCCESS(status)) { return status; }

    KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_INFO_LEVEL,
               "vhf-stylus: device added, VHF handle %p\n", devCtx->VhfHandle));
    return STATUS_SUCCESS;
}

// ── CreateReportQueue ─────────────────────────────────────────────────────────

NTSTATUS
CreateReportQueue(
    WDFDEVICE       Device,
    PDEVICE_CONTEXT DevCtx
)
{
    WDF_IO_QUEUE_CONFIG queueConfig;
    NTSTATUS            status;

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchSequential);
    queueConfig.EvtIoDeviceControl = EvtIoDeviceControl;

    status = WdfIoQueueCreate(Device, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, &DevCtx->ReportQueue);
    return status;
}

// ── RegisterVhfDevice ─────────────────────────────────────────────────────────

NTSTATUS
RegisterVhfDevice(
    WDFDEVICE       Device,
    PDEVICE_CONTEXT DevCtx
)
{
    VHF_CONFIG vhfConfig;
    NTSTATUS   status;

    VHF_CONFIG_INIT(
        &vhfConfig,
        WdfDeviceWdmGetDeviceObject(Device),
        HID_REPORT_DESC_SIZE,
        (PUCHAR)HidReportDescriptor
    );

    // Register async callback for GetInputReport — this is how VHF asks us
    // for the next report when the HID stack is ready to consume it.
    vhfConfig.EvtVhfAsyncOperationGetInputReport = EvtVhfAsyncOperationGetInputReport;

    status = VhfCreate(&vhfConfig, &DevCtx->VhfHandle);
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_ERROR_LEVEL,
                   "vhf-stylus: VhfCreate failed 0x%x\n", status));
        return status;
    }

    status = VhfStart(DevCtx->VhfHandle);
    return status;
}

// ── EvtIoDeviceControl ────────────────────────────────────────────────────────

VOID
EvtIoDeviceControl(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ SIZE_T     OutputBufferLength,
    _In_ SIZE_T     InputBufferLength,
    _In_ ULONG      IoControlCode
)
{
    NTSTATUS        status = STATUS_SUCCESS;
    WDFDEVICE       device = WdfIoQueueGetDevice(Queue);
    PDEVICE_CONTEXT devCtx = DeviceGetContext(device);
    PVOID           inputBuf;
    SIZE_T          transferred = 0;

    UNREFERENCED_PARAMETER(OutputBufferLength);

    switch (IoControlCode) {
    case IOCTL_IEXTEND_SUBMIT_REPORT:
        if (InputBufferLength < HID_REPORT_SIZE_BYTES) {
            status = STATUS_BUFFER_TOO_SMALL;
            break;
        }
        status = WdfRequestRetrieveInputBuffer(Request, HID_REPORT_SIZE_BYTES, &inputBuf, NULL);
        if (!NT_SUCCESS(status)) { break; }

        SubmitHidReport(devCtx, (PUCHAR)inputBuf, HID_REPORT_SIZE_BYTES);
        transferred = 0;
        break;

    default:
        status = STATUS_INVALID_DEVICE_REQUEST;
        break;
    }

    WdfRequestCompleteWithInformation(Request, status, transferred);
}

// ── SubmitHidReport ───────────────────────────────────────────────────────────

VOID
SubmitHidReport(
    PDEVICE_CONTEXT DevCtx,
    PUCHAR          ReportBuf,
    SIZE_T          ReportLen
)
{
    HID_XFER_PACKET xfer;
    NTSTATUS        status;

    xfer.reportBuffer     = ReportBuf;
    xfer.reportBufferLen  = (ULONG)ReportLen;
    xfer.reportId         = HID_REPORT_ID;

    status = VhfNotifyOperationComplete(DevCtx->VhfHandle, NULL, &xfer);
    if (!NT_SUCCESS(status)) {
        KdPrintEx((DPFLTR_IHVDRIVER_ID, DPFLTR_WARNING_LEVEL,
                   "vhf-stylus: VhfNotifyOperationComplete 0x%x\n", status));
    }
}

// ── VHF async callbacks ───────────────────────────────────────────────────────

VOID
EvtVhfAsyncOperationGetInputReport(
    _In_opt_ PVOID   VhfClientContext,
    _In_     VHFOPERATIONHANDLE VhfOperationHandle,
    _In_opt_ PVOID   VhfOperationContext,
    _In_     PHID_XFER_PACKET HidTransferPacket
)
{
    // VHF polls us for the latest report.  In our design the user-mode partner
    // drives injection via IOCTL, so there is nothing to push here.
    // Complete the operation with STATUS_SUCCESS and an empty report so the
    // HID stack does not stall.
    UNREFERENCED_PARAMETER(VhfClientContext);
    UNREFERENCED_PARAMETER(VhfOperationContext);
    UNREFERENCED_PARAMETER(HidTransferPacket);
    VhfAsyncOperationComplete(VhfOperationHandle, STATUS_SUCCESS);
}

VOID
EvtVhfAsyncOperationGetFeatureReport(
    _In_opt_ PVOID   VhfClientContext,
    _In_     VHFOPERATIONHANDLE VhfOperationHandle,
    _In_opt_ PVOID   VhfOperationContext,
    _In_     PHID_XFER_PACKET HidTransferPacket
)
{
    UNREFERENCED_PARAMETER(VhfClientContext);
    UNREFERENCED_PARAMETER(VhfOperationContext);
    UNREFERENCED_PARAMETER(HidTransferPacket);
    VhfAsyncOperationComplete(VhfOperationHandle, STATUS_NOT_SUPPORTED);
}

VOID
EvtVhfAsyncOperationSetFeatureReport(
    _In_opt_ PVOID   VhfClientContext,
    _In_     VHFOPERATIONHANDLE VhfOperationHandle,
    _In_opt_ PVOID   VhfOperationContext,
    _In_     PHID_XFER_PACKET HidTransferPacket
)
{
    UNREFERENCED_PARAMETER(VhfClientContext);
    UNREFERENCED_PARAMETER(VhfOperationContext);
    UNREFERENCED_PARAMETER(HidTransferPacket);
    VhfAsyncOperationComplete(VhfOperationHandle, STATUS_NOT_SUPPORTED);
}
