// Driver.c — DriverEntry + EvtDeviceAdd. Intentionally small.

#include "Driver.h"
#include "IoQueue.h"

NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
)
{
    WDF_DRIVER_CONFIG config;
    NTSTATUS          status;
    WDF_OBJECT_ATTRIBUTES attrs;

    WPP_INIT_TRACING(DriverObject, RegistryPath);
    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER, "DriverEntry: protocol=%u",
        IEXDD_PROTOCOL_VERSION);

    WDF_OBJECT_ATTRIBUTES_INIT(&attrs);
    attrs.EvtCleanupCallback = IexddEvtDriverContextCleanup;

    WDF_DRIVER_CONFIG_INIT(&config, IexddEvtDeviceAdd);
    config.DriverPoolTag = 'IExD';

    status = WdfDriverCreate(DriverObject, RegistryPath, &attrs, &config, WDF_NO_HANDLE);
    if (!NT_SUCCESS(status)) {
        TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER, "WdfDriverCreate=0x%08X", status);
        WPP_CLEANUP(DriverObject);
        return status;
    }

    return STATUS_SUCCESS;
}

VOID
IexddEvtDriverContextCleanup(
    _In_ WDFOBJECT DriverObject
)
{
    WPP_CLEANUP(WdfDriverWdmGetDriverObject((WDFDRIVER)DriverObject));
}

NTSTATUS
IexddEvtDeviceAdd(
    _In_    WDFDRIVER       Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
)
{
    NTSTATUS              status;
    WDF_OBJECT_ATTRIBUTES devAttrs;
    WDF_OBJECT_ATTRIBUTES fileAttrs;
    WDF_FILEOBJECT_CONFIG fileCfg;
    WDF_PNPPOWER_EVENT_CALLBACKS power;
    WDFDEVICE             device;
    PIEXDD_DEVICE_CONTEXT ctx;

    UNREFERENCED_PARAMETER(Driver);

    // Tell IddCx this is going to be an indirect display device. Must be called
    // BEFORE WdfDeviceCreate; configures the framework so subsequent IddCx calls
    // see a properly initialized stack.
    status = IddCxDeviceInitConfig(DeviceInit);
    if (!NT_SUCCESS(status)) {
        TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER, "IddCxDeviceInitConfig=0x%08X", status);
        return status;
    }

    // PnP/power callbacks — D0Entry is where we'll start the swapchain processing
    // thread once a render adapter is bound (Task 4).
    WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&power);
    power.EvtDeviceD0Entry = IexddEvtDeviceD0Entry;
    power.EvtDeviceD0Exit  = IexddEvtDeviceD0Exit;
    WdfDeviceInitSetPnpPowerEventCallbacks(DeviceInit, &power);

    // Per-file context: HELLO state, duplicated client process handle, nonce.
    WDF_FILEOBJECT_CONFIG_INIT(&fileCfg, NULL, NULL, IexddEvtFileCleanup);
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&fileAttrs, IEXDD_FILE_CONTEXT);
    WdfDeviceInitSetFileObjectConfig(DeviceInit, &fileCfg, &fileAttrs);

    // Per-device context.
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&devAttrs, IEXDD_DEVICE_CONTEXT);
    devAttrs.EvtCleanupCallback = IexddEvtDeviceContextCleanup;

    status = WdfDeviceCreate(&DeviceInit, &devAttrs, &device);
    if (!NT_SUCCESS(status)) {
        TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER, "WdfDeviceCreate=0x%08X", status);
        return status;
    }

    ctx = IexddDeviceGetContext(device);
    KeInitializeSpinLock(&ctx->StatsLock);
    ctx->Stats.ProtocolVersion = IEXDD_PROTOCOL_VERSION;

    // Default queue: HELLO / RELEASE / QUERY_STATS — quick handlers, parallel dispatch.
    status = IexddCreateDefaultQueue(device, &ctx->DefaultQueue);
    if (!NT_SUCCESS(status)) return status;

    // Pull queue: manual dispatch — PULL_FRAME requests park here until the
    // processing thread completes them with a frame buffer.
    status = IexddCreatePullQueue(device, &ctx->PullQueue);
    if (!NT_SUCCESS(status)) return status;

    // IddCx adapter creation (Adapter.c). After this returns successfully, IddCx
    // will fire EvtAdapterInitFinished asynchronously to signal monitor attach.
    status = IexddCreateAdapter(device);
    if (!NT_SUCCESS(status)) {
        TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER, "IexddCreateAdapter=0x%08X", status);
        return status;
    }

    return STATUS_SUCCESS;
}

VOID
IexddEvtDeviceContextCleanup(
    _In_ WDFOBJECT Device
)
{
    UNREFERENCED_PARAMETER(Device);
    // IddCx adapter and child monitors are torn down via WDF child-object lifetime.
    // Pull queue requests are completed with STATUS_CANCELLED automatically when
    // the device leaves D0; nothing to do here.
}

NTSTATUS
IexddEvtDeviceD0Entry(
    _In_ WDFDEVICE Device,
    _In_ WDF_POWER_DEVICE_STATE PreviousState
)
{
    UNREFERENCED_PARAMETER(Device);
    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER, "D0Entry from %d", PreviousState);
    return STATUS_SUCCESS;
}

NTSTATUS
IexddEvtDeviceD0Exit(
    _In_ WDFDEVICE Device,
    _In_ WDF_POWER_DEVICE_STATE TargetState
)
{
    UNREFERENCED_PARAMETER(Device);
    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER, "D0Exit to %d", TargetState);
    return STATUS_SUCCESS;
}

VOID
IexddEvtFileCleanup(
    _In_ WDFFILEOBJECT FileObject
)
{
    PIEXDD_FILE_CONTEXT fctx = IexddFileGetContext(FileObject);
    if (fctx->ClientProcess != NULL) {
        ZwClose(fctx->ClientProcess);
        fctx->ClientProcess = NULL;
    }
    fctx->HelloComplete = FALSE;
    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER, "EvtFileCleanup: client gone");
}
