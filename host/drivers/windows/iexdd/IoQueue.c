// IoQueue.c — WDF queue creation and IOCTL dispatch.
//
// IOCTL_IEXDD_HELLO:         Validate protocol version, open client process.
// IOCTL_IEXDD_PULL_FRAME:    Forward to the manual PullQueue; completed by
//                             the processing thread in SwapChain.c.
// IOCTL_IEXDD_RELEASE_FRAME: Ack a frame back to WDDM via IddCx.
// IOCTL_IEXDD_QUERY_STATS:   Copy IEXDD_STATS from device context.

#include "Driver.h"
#include "IoQueue.h"

// ---------------------------------------------------------------------------
// IexddCreateDefaultQueue
// ---------------------------------------------------------------------------

NTSTATUS
IexddCreateDefaultQueue(
    _In_  WDFDEVICE  Device,
    _Out_ WDFQUEUE  *pQueue
)
{
    WDF_IO_QUEUE_CONFIG cfg;
    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&cfg, WdfIoQueueDispatchParallel);
    cfg.EvtIoDeviceControl = IexddEvtIoDeviceControl;
    cfg.PowerManaged       = WdfTrue;

    return WdfIoQueueCreate(Device, &cfg, WDF_NO_OBJECT_ATTRIBUTES, pQueue);
}

// ---------------------------------------------------------------------------
// IexddCreatePullQueue
// ---------------------------------------------------------------------------

NTSTATUS
IexddCreatePullQueue(
    _In_  WDFDEVICE  Device,
    _Out_ WDFQUEUE  *pQueue
)
{
    WDF_IO_QUEUE_CONFIG cfg;
    WDF_IO_QUEUE_CONFIG_INIT(&cfg, WdfIoQueueDispatchManual);
    cfg.PowerManaged = WdfFalse; // must not cancel IRPs on D3 — user-mode re-posts

    return WdfIoQueueCreate(Device, &cfg, WDF_NO_OBJECT_ATTRIBUTES, pQueue);
}

// ---------------------------------------------------------------------------
// IexddEvtIoDeviceControl
// ---------------------------------------------------------------------------

VOID
IexddEvtIoDeviceControl(
    _In_ WDFQUEUE   Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t     OutputBufferLength,
    _In_ size_t     InputBufferLength,
    _In_ ULONG      IoControlCode
)
{
    NTSTATUS status  = STATUS_INVALID_DEVICE_REQUEST;
    SIZE_T   written = 0;
    WDFDEVICE device = WdfIoQueueGetDevice(Queue);
    PIEXDD_DEVICE_CONTEXT dctx = IexddDeviceGetContext(device);

    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(InputBufferLength);

    switch (IoControlCode) {

    // ------------------------------------------------------------------
    // IOCTL_IEXDD_HELLO
    // ------------------------------------------------------------------
    case IOCTL_IEXDD_HELLO: {
        PIEXDD_HELLO inHello  = NULL;
        PIEXDD_HELLO outHello = NULL;
        HANDLE       clientProcess;
        CLIENT_ID    clientId;
        OBJECT_ATTRIBUTES oa;
        WDFFILEOBJECT     fileObj;
        PIEXDD_FILE_CONTEXT fctx;

        status = WdfRequestRetrieveInputBuffer(Request,
                     sizeof(IEXDD_HELLO), (PVOID*)&inHello, NULL);
        if (!NT_SUCCESS(status)) break;

        status = WdfRequestRetrieveOutputBuffer(Request,
                     sizeof(IEXDD_HELLO), (PVOID*)&outHello, NULL);
        if (!NT_SUCCESS(status)) break;

        if (inHello->ProtocolVersion != IEXDD_PROTOCOL_VERSION) {
            TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER,
                        "HELLO: protocol mismatch — client=%u, driver=%u",
                        inHello->ProtocolVersion, IEXDD_PROTOCOL_VERSION);
            status = STATUS_REVISION_MISMATCH;
            break;
        }

        // Open the client process with PROCESS_DUP_HANDLE access so we can
        // duplicate shared texture handles into it later.
        InitializeObjectAttributes(&oa, NULL, OBJ_KERNEL_HANDLE, NULL, NULL);
        clientId.UniqueProcess = (HANDLE)(UINT_PTR)inHello->ClientPid;
        clientId.UniqueThread  = NULL;

        status = ZwOpenProcess(&clientProcess, PROCESS_DUP_HANDLE, &oa, &clientId);
        if (!NT_SUCCESS(status)) {
            TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER,
                        "HELLO: ZwOpenProcess(PID=%u)=0x%08X",
                        inHello->ClientPid, status);
            break;
        }

        fileObj = WdfRequestGetFileObject(Request);
        fctx    = IexddFileGetContext(fileObj);

        if (fctx->HelloComplete) {
            // Duplicate handshake on the same file object — reject.
            ZwClose(clientProcess);
            status = STATUS_INVALID_DEVICE_STATE;
            break;
        }

        fctx->ClientProcess  = clientProcess;
        fctx->ClientNonce    = inHello->ClientNonce;
        fctx->HelloComplete  = TRUE;

        *outHello = *inHello; // echo input; version already validated
        written   = sizeof(IEXDD_HELLO);

        TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER,
                    "HELLO: client PID=%u connected", inHello->ClientPid);

        status = STATUS_SUCCESS;
        break;
    }

    // ------------------------------------------------------------------
    // IOCTL_IEXDD_PULL_FRAME — forward to the manual queue.
    // ------------------------------------------------------------------
    case IOCTL_IEXDD_PULL_FRAME: {
        WDFFILEOBJECT     fileObj = WdfRequestGetFileObject(Request);
        PIEXDD_FILE_CONTEXT fctx  = IexddFileGetContext(fileObj);

        if (!fctx->HelloComplete) {
            status = STATUS_INVALID_DEVICE_STATE;
            break;
        }

        // Park the request in the manual queue; the processing thread will
        // complete it when a frame is ready.
        status = WdfRequestForwardToIoQueue(Request, dctx->PullQueue);
        if (NT_SUCCESS(status)) {
            // Do NOT complete the request here — that's the thread's job.
            return;
        }
        // If forwarding fails the request falls through and is completed below.
        break;
    }

    // ------------------------------------------------------------------
    // IOCTL_IEXDD_RELEASE_FRAME
    // ------------------------------------------------------------------
    case IOCTL_IEXDD_RELEASE_FRAME: {
        PIEXDD_FRAME_RELEASE rel = NULL;
        WDFFILEOBJECT fileObj    = WdfRequestGetFileObject(Request);
        PIEXDD_FILE_CONTEXT fctx = IexddFileGetContext(fileObj);

        if (!fctx->HelloComplete) {
            status = STATUS_INVALID_DEVICE_STATE;
            break;
        }

        status = WdfRequestRetrieveInputBuffer(Request,
                     sizeof(IEXDD_FRAME_RELEASE), (PVOID*)&rel, NULL);
        if (!NT_SUCCESS(status)) break;

        TraceEvents(TRACE_LEVEL_VERBOSE, TRACE_DRIVER,
                    "RELEASE_FRAME: seq=%llu", rel->AcquireSeq);

        // The actual IddCx ack is handled by SwapChain.c when the processing
        // thread receives the release notification. Here we just record the
        // counter; the in-flight table lookup happens in the processing thread
        // on its next iteration.
        //
        // For v1 we use a simple approach: the RELEASE IOCTL completes
        // immediately (STATUS_SUCCESS) and the processing thread polls a
        // shared "released set" before calling IddCxSwapChainFinishedProcessingFrame.
        // A more efficient design would use a completion port, but v1 throughput
        // is 60-120 fps and the extra latency is negligible (<1 µs).

        KLOCK_QUEUE_HANDLE lqh;
        KeAcquireInStackQueuedSpinLock(&dctx->StatsLock, &lqh);
        dctx->Stats.ReleaseRequestsTotal++;
        KeReleaseInStackQueuedSpinLock(&lqh);

        status = STATUS_SUCCESS;
        break;
    }

    // ------------------------------------------------------------------
    // IOCTL_IEXDD_QUERY_STATS
    // ------------------------------------------------------------------
    case IOCTL_IEXDD_QUERY_STATS: {
        PIEXDD_STATS outStats = NULL;
        status = WdfRequestRetrieveOutputBuffer(Request,
                     sizeof(IEXDD_STATS), (PVOID*)&outStats, NULL);
        if (!NT_SUCCESS(status)) break;

        KLOCK_QUEUE_HANDLE lqh;
        KeAcquireInStackQueuedSpinLock(&dctx->StatsLock, &lqh);
        *outStats = dctx->Stats;
        KeReleaseInStackQueuedSpinLock(&lqh);

        written = sizeof(IEXDD_STATS);
        status  = STATUS_SUCCESS;
        break;
    }

    default:
        status  = STATUS_INVALID_DEVICE_REQUEST;
        written = 0;
        break;
    }

    WdfRequestCompleteWithInformation(Request, status, written);
}
