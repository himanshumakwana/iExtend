// Adapter.c — IddCx adapter init, virtual monitor attachment, mode list.
//
// Responsible for:
//   1. Creating the IddCx adapter object via IddCxAdapterInitAsync.
//   2. In EvtAdapterInitFinished: synthesising an EDID and calling
//      IddCxMonitorCreate + IddCxMonitorArrival.
//   3. Advertising three target modes (1080p120, 1440p120, 4K60).
//   4. Registering the SwapChain callbacks that SwapChain.c implements.
//
// No global mutable state. All per-adapter data flows through the WDF object
// tree: device → IEXDD_DEVICE_CONTEXT → IEXDD_MONITOR_CONTEXT.

#include "Driver.h"
#include "Adapter.h"
#include "SwapChain.h"
#include "Edid.h"

// ---------------------------------------------------------------------------
// Mode table
// ---------------------------------------------------------------------------
// Pixel clock is computed as: width * height * refresh * 1.05 (5% overhead for
// blanking). Values here are in units of 10 kHz as required by IDDCX_TARGET_MODE.

const IDDCX_TARGET_MODE g_SupportedModes[IEXDD_MODE_COUNT] = {
    // 1920x1080 @120 Hz
    {
        .Size = sizeof(IDDCX_TARGET_MODE),
        .TargetVideoSignalInfo = {
            .AdditionalSignalInfo = {
                .SegmentCount = 1,
                .VsyncFreqDivider = 1,
            },
            .TotalSize = { .cx = 2080, .cy = 1111 }, // includes blanking
            .ActiveSize = { .cx = 1920, .cy = 1080 },
            .VSyncFreq = { .Numerator = 120, .Denominator = 1 },
            .HSyncFreq = { .Numerator = 135600, .Denominator = 1 },
            .PixelRate = 270000000, // 270 MHz
            .ScanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE,
        },
    },
    // 2560x1440 @120 Hz
    {
        .Size = sizeof(IDDCX_TARGET_MODE),
        .TargetVideoSignalInfo = {
            .AdditionalSignalInfo = {
                .SegmentCount = 1,
                .VsyncFreqDivider = 1,
            },
            .TotalSize = { .cx = 2720, .cy = 1481 },
            .ActiveSize = { .cx = 2560, .cy = 1440 },
            .VSyncFreq = { .Numerator = 120, .Denominator = 1 },
            .HSyncFreq = { .Numerator = 177720, .Denominator = 1 },
            .PixelRate = 483502080, // ~483 MHz
            .ScanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE,
        },
    },
    // 3840x2160 @60 Hz
    {
        .Size = sizeof(IDDCX_TARGET_MODE),
        .TargetVideoSignalInfo = {
            .AdditionalSignalInfo = {
                .SegmentCount = 1,
                .VsyncFreqDivider = 1,
            },
            .TotalSize = { .cx = 4000, .cy = 2222 },
            .ActiveSize = { .cx = 3840, .cy = 2160 },
            .VSyncFreq = { .Numerator = 60, .Denominator = 1 },
            .HSyncFreq = { .Numerator = 133320, .Denominator = 1 },
            .PixelRate = 533280000, // ~533 MHz
            .ScanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE,
        },
    },
};

// ---------------------------------------------------------------------------
// Adapter capabilities
// ---------------------------------------------------------------------------

static const IDDCX_ADAPTER_CAPS s_AdapterCaps = {
    .Size                 = sizeof(IDDCX_ADAPTER_CAPS),
    .MaxMonitorsSupported = 1,
    .EndPointDiagnostics  = {
        .Size             = sizeof(IDDCX_ENDPOINT_DIAGNOSTIC_INFO),
        .GammaSupport     = IDDCX_FEATURE_IMPLEMENTATION_NONE,
        .TransmissionType = IDDCX_TRANSMISSION_TYPE_WIRED_OTHER,
        .pEndPointFriendlyName = L"iExtend Virtual Adapter",
        .pEndPointManufacturerName = L"iExtend Project",
        .pEndPointModelName  = L"iExtend Virtual Display",
    },
};

// ---------------------------------------------------------------------------
// IexddCreateAdapter — called from EvtDeviceAdd in Driver.c
// ---------------------------------------------------------------------------

NTSTATUS
IexddCreateAdapter(
    _In_ WDFDEVICE Device
)
{
    NTSTATUS              status;
    IDDCX_ADAPTER_CONFIG  cfg;
    IDARG_IN_ADAPTER_INIT adapterIn;
    IDARG_OUT_ADAPTER_INIT adapterOut;
    PIEXDD_DEVICE_CONTEXT ctx;

    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER, "IexddCreateAdapter");

    RtlZeroMemory(&cfg, sizeof(cfg));
    cfg.Size                             = sizeof(cfg);
    cfg.AdapterCaps                      = s_AdapterCaps;
    cfg.EvtIddCxAdapterInitFinished      = IexddEvtAdapterInitFinished;
    cfg.EvtIddCxAdapterCommitModes       = IexddEvtAdapterCommitModes;
    cfg.EvtIddCxParseMonitorDescription  = IexddEvtParseMonitorDescription;
    cfg.EvtIddCxMonitorGetDefaultDescriptionModes = IexddEvtMonitorGetDefaultModes;
    cfg.EvtIddCxMonitorQueryTargetModes  = IexddEvtMonitorQueryTargetModes;
    cfg.EvtIddCxMonitorAssignSwapChain   = IexddEvtMonitorAssignSwapChain;
    cfg.EvtIddCxMonitorUnassignSwapChain = IexddEvtMonitorUnassignSwapChain;

    RtlZeroMemory(&adapterIn, sizeof(adapterIn));
    adapterIn.WdfDevice         = Device;
    adapterIn.pAdapterConfig    = &cfg;
    adapterIn.ObjectAttributes  = WDF_NO_OBJECT_ATTRIBUTES;

    status = IddCxAdapterInitAsync(&adapterIn, &adapterOut);
    if (!NT_SUCCESS(status)) {
        TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER,
                    "IddCxAdapterInitAsync=0x%08X", status);
        return status;
    }

    ctx          = IexddDeviceGetContext(Device);
    ctx->Adapter = adapterOut.AdapterObject;

    return STATUS_SUCCESS;
}

// ---------------------------------------------------------------------------
// EvtIddCxAdapterInitFinished — fires asynchronously once WDDM acks the adapter.
// We use this to announce the single virtual monitor.
// ---------------------------------------------------------------------------

VOID
IexddEvtAdapterInitFinished(
    _In_ IDDCX_ADAPTER                       Adapter,
    _In_ const IDARG_OUT_ADAPTER_INIT_FINISHED *pFinished
)
{
    NTSTATUS                status;
    UCHAR                   edid[IEXDD_EDID_SIZE];
    IDDCX_MONITOR_INFO      monitorInfo;
    IDARG_IN_MONITORCREATE  monIn;
    IDARG_OUT_MONITORCREATE monOut;
    WDF_OBJECT_ATTRIBUTES   monAttrs;

    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER,
                "AdapterInitFinished status=0x%08X", pFinished->AdapterInitStatus);

    if (!NT_SUCCESS(pFinished->AdapterInitStatus)) {
        TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER,
                    "Adapter init failed — monitor will not be created");
        return;
    }

    // Synthesize an EDID 1.4 block for the virtual monitor.
    IexddBuildEdid(edid, "iExtend Virtual Display");

    RtlZeroMemory(&monitorInfo, sizeof(monitorInfo));
    monitorInfo.Size                              = sizeof(monitorInfo);
    monitorInfo.MonitorType                       = IDDCX_MONITOR_TYPE_LCD;
    monitorInfo.ConnectorIndex                    = 0;
    monitorInfo.MonitorContainerId                = IEXDD_MONITOR_CONTAINER_GUID;
    monitorInfo.MonitorDescription.Size           = sizeof(IDDCX_MONITOR_DESCRIPTION);
    monitorInfo.MonitorDescription.Type           = IDDCX_MONITOR_DESCRIPTION_TYPE_EDID;
    monitorInfo.MonitorDescription.DataSize       = IEXDD_EDID_SIZE;
    monitorInfo.MonitorDescription.pData          = edid;

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&monAttrs, IEXDD_MONITOR_CONTEXT);

    RtlZeroMemory(&monIn, sizeof(monIn));
    monIn.ObjectAttributes = &monAttrs;
    monIn.MonitorInfo      = monitorInfo;

    status = IddCxMonitorCreate(Adapter, &monIn, &monOut);
    if (!NT_SUCCESS(status)) {
        TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER,
                    "IddCxMonitorCreate=0x%08X", status);
        return;
    }

    // Initialise the per-monitor context back-pointer.
    PIEXDD_MONITOR_CONTEXT mctx = IexddMonitorGetContext(monOut.MonitorObject);
    RtlZeroMemory(mctx, sizeof(*mctx));
    mctx->MonitorObject = monOut.MonitorObject;
    // OwnerDevice is determined via IddCxMonitorCreate's parent adapter;
    // retrieve device via WdfObjectGetParent chain if needed in SwapChain.c.

    // Tell WDDM the monitor is now connected (hot-plug arrival).
    status = IddCxMonitorArrival(monOut.MonitorObject);
    if (!NT_SUCCESS(status)) {
        TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER,
                    "IddCxMonitorArrival=0x%08X", status);
    } else {
        TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER,
                    "Virtual monitor announced to WDDM");
    }
}

// ---------------------------------------------------------------------------
// EvtIddCxAdapterCommitModes — WDDM sends this when a mode change is committed.
// We accept all mode commits (our mode list has already been negotiated by
// EvtMonitorQueryTargetModes). No per-mode D3D resource allocation needed —
// IddCx handles that in the swapchain layer.
// ---------------------------------------------------------------------------

NTSTATUS
IexddEvtAdapterCommitModes(
    _In_ IDDCX_ADAPTER                        Adapter,
    _In_ const IDARG_IN_COMMITMODES          *pCommitModes
)
{
    UNREFERENCED_PARAMETER(Adapter);
    UNREFERENCED_PARAMETER(pCommitModes);
    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER, "EvtAdapterCommitModes");
    return STATUS_SUCCESS;
}

// ---------------------------------------------------------------------------
// EvtIddCxParseMonitorDescription — decode raw EDID bytes (or other formats)
// into a list of preferred modes that WDDM should consider.
// We ship a single-format EDID and return the same three modes always.
// ---------------------------------------------------------------------------

NTSTATUS
IexddEvtParseMonitorDescription(
    _In_  const IDARG_IN_PARSEMONITORDESCRIPTION  *pIn,
    _Out_       IDARG_OUT_PARSEMONITORDESCRIPTION *pOut
)
{
    UNREFERENCED_PARAMETER(pIn);

    // Report that we support IEXDD_MODE_COUNT preferred modes.
    pOut->MonitorModeBufferOutputCount = IEXDD_MODE_COUNT;

    if (pIn->MonitorModeBufferInputCount == 0) {
        // First call: just report the count.
        return STATUS_BUFFER_TOO_SMALL;
    }

    if (pIn->MonitorModeBufferInputCount < IEXDD_MODE_COUNT) {
        return STATUS_BUFFER_TOO_SMALL;
    }

    // Fill in the preferred mode list from our global table.
    for (UINT32 i = 0; i < IEXDD_MODE_COUNT; i++) {
        pIn->pMonitorModes[i]        = g_SupportedModes[i];
        pIn->pMonitorModes[i].Origin = IDDCX_MONITOR_MODE_ORIGIN_MONITORDESCRIPTOR;
    }

    // The preferred (default) mode is 1080p120 at index 0.
    pOut->PreferredMonitorModeIdx = 0;

    return STATUS_SUCCESS;
}

// ---------------------------------------------------------------------------
// EvtIddCxMonitorGetDefaultDescriptionModes — return EDID-preferred modes when
// no EDID is available. We always have an EDID so this callback is a stub.
// ---------------------------------------------------------------------------

NTSTATUS
IexddEvtMonitorGetDefaultModes(
    _In_  IDDCX_MONITOR                                           Monitor,
    _In_  const IDARG_IN_GETDEFAULTDESCRIPTIONMODES              *pIn,
    _Out_       IDARG_OUT_GETDEFAULTDESCRIPTIONMODES             *pOut
)
{
    UNREFERENCED_PARAMETER(Monitor);
    UNREFERENCED_PARAMETER(pIn);

    pOut->DefaultMonitorModeBufferOutputCount = IEXDD_MODE_COUNT;

    if (pIn->DefaultMonitorModeBufferInputCount == 0) {
        return STATUS_BUFFER_TOO_SMALL;
    }

    if (pIn->DefaultMonitorModeBufferInputCount < IEXDD_MODE_COUNT) {
        return STATUS_BUFFER_TOO_SMALL;
    }

    for (UINT32 i = 0; i < IEXDD_MODE_COUNT; i++) {
        pIn->pDefaultMonitorModes[i]        = g_SupportedModes[i];
        pIn->pDefaultMonitorModes[i].Origin = IDDCX_MONITOR_MODE_ORIGIN_DRIVER;
    }

    pOut->PreferredDefaultMonitorModeIdx = 0;

    return STATUS_SUCCESS;
}

// ---------------------------------------------------------------------------
// EvtIddCxMonitorQueryTargetModes — report the target modes we can drive.
// WDDM calls this after EvtParseMonitorDescription to get the negotiated list.
// ---------------------------------------------------------------------------

NTSTATUS
IexddEvtMonitorQueryTargetModes(
    _In_  IDDCX_MONITOR                            Monitor,
    _In_  const IDARG_IN_QUERYTARGETMODES          *pIn,
    _Out_       IDARG_OUT_QUERYTARGETMODES          *pOut
)
{
    UNREFERENCED_PARAMETER(Monitor);

    pOut->TargetModeBufferOutputCount = IEXDD_MODE_COUNT;

    if (pIn->TargetModeBufferInputCount == 0) {
        return STATUS_BUFFER_TOO_SMALL;
    }

    if (pIn->TargetModeBufferInputCount < IEXDD_MODE_COUNT) {
        return STATUS_BUFFER_TOO_SMALL;
    }

    RtlCopyMemory(pIn->pTargetModes, g_SupportedModes,
                  IEXDD_MODE_COUNT * sizeof(IDDCX_TARGET_MODE));

    return STATUS_SUCCESS;
}
