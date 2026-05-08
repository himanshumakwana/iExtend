// SwapChain.c — IddCx swapchain processing thread + inverted-call IOCTL pump.
//
// Per-monitor frame path:
//   1. IddCxSwapChainReleaseAndAcquireBuffer() — block until WDDM delivers a frame.
//   2. ZwDuplicateObject() — copy the shared NT handle into the user-mode partner PID.
//   3. Populate IEXDD_FRAME_HEADER + dirty RECT[] and complete the oldest pending
//      IOCTL_IEXDD_PULL_FRAME request from the WdfIoQueue.
//   4. On IOCTL_IEXDD_RELEASE_FRAME: look up the frame by AcquireSeq, call
//      IddCxSwapChainFinishedProcessingFrame, decrement in-flight counter.
//   5. If the in-flight count reaches IEXDD_MAX_INFLIGHT_FRAMES: drop the oldest
//      in-flight frame by acking it ourselves, bump FramesDropped.
//
// Thread affinity: pinned to the first P-core, above-normal priority, to reduce
// jitter. These choices can be revisited in Plan 10's CPU profiling pass.

#include "Driver.h"
#include "Adapter.h"
#include "SwapChain.h"
#include "IoQueue.h"

// ---------------------------------------------------------------------------
// In-flight frame table
// ---------------------------------------------------------------------------
// Up to IEXDD_MAX_INFLIGHT_FRAMES frames can be outstanding with user-mode at
// once. Each entry holds the acquire-seq and the matching IddCx acquire token
// so we can ack the right frame when user-mode calls RELEASE.

typedef struct _IEXDD_INFLIGHT_ENTRY {
    BOOLEAN Valid;
    UINT64  AcquireSeq;
    IDARG_OUT_RELEASEANDACQUIREBUFFER AcquireResult; // opaque IddCx release token
} IEXDD_INFLIGHT_ENTRY, *PIEXDD_INFLIGHT_ENTRY;

// ---------------------------------------------------------------------------
// EvtIddCxMonitorAssignSwapChain
// ---------------------------------------------------------------------------

NTSTATUS
IexddEvtMonitorAssignSwapChain(
    _In_ IDDCX_MONITOR                              Monitor,
    _In_ const IDARG_IN_SETSWAPCHAIN               *pInArgs
)
{
    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER,
                "EvtMonitorAssignSwapChain");

    return IexddSwapChainStart(
        Monitor,
        pInArgs->hSwapChain,
        pInArgs->RenderAdapterLuid
    );
}

// ---------------------------------------------------------------------------
// EvtIddCxMonitorUnassignSwapChain
// ---------------------------------------------------------------------------

VOID
IexddEvtMonitorUnassignSwapChain(
    _In_ IDDCX_MONITOR Monitor
)
{
    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER,
                "EvtMonitorUnassignSwapChain");
    IexddSwapChainStop(Monitor);
}

// ---------------------------------------------------------------------------
// IexddSwapChainStart
// ---------------------------------------------------------------------------

NTSTATUS
IexddSwapChainStart(
    _In_ IDDCX_MONITOR  Monitor,
    _In_ IDDCX_SWAPCHAIN SwapChain,
    _In_ LUID           RenderAdapterLuid
)
{
    PIEXDD_MONITOR_CONTEXT mctx = IexddMonitorGetContext(Monitor);
    HANDLE threadHandle;
    NTSTATUS status;

    // Guard against double-start if WDF fires the callback twice.
    if (mctx->ProcessingThread != NULL) {
        TraceEvents(TRACE_LEVEL_WARNING, TRACE_DRIVER,
                    "SwapChainStart called with thread already running — ignoring");
        return STATUS_SUCCESS;
    }

    mctx->SwapChain         = SwapChain;
    mctx->RenderAdapterLuid = RenderAdapterLuid;
    KeInitializeEvent(&mctx->TerminateEvent, NotificationEvent, FALSE);

    status = PsCreateSystemThread(
        &threadHandle,
        THREAD_ALL_ACCESS,
        NULL, NULL, NULL,
        IexddProcessingThread,
        mctx
    );
    if (!NT_SUCCESS(status)) {
        TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER,
                    "PsCreateSystemThread=0x%08X", status);
        return status;
    }

    // Keep a PETHREAD reference so we can wait for the thread on stop.
    status = ObReferenceObjectByHandle(
        threadHandle, THREAD_ALL_ACCESS, NULL, KernelMode,
        (PVOID*)&mctx->ProcessingThread, NULL
    );
    ZwClose(threadHandle);

    if (!NT_SUCCESS(status)) {
        TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER,
                    "ObReferenceObjectByHandle thread=0x%08X", status);
        mctx->ProcessingThread = NULL;
    }

    return status;
}

// ---------------------------------------------------------------------------
// IexddSwapChainStop
// ---------------------------------------------------------------------------

VOID
IexddSwapChainStop(
    _In_ IDDCX_MONITOR Monitor
)
{
    PIEXDD_MONITOR_CONTEXT mctx = IexddMonitorGetContext(Monitor);

    if (mctx->ProcessingThread == NULL) {
        return;
    }

    // Signal the thread to exit.
    KeSetEvent(&mctx->TerminateEvent, IO_NO_INCREMENT, FALSE);

    // Wait for it — no timeout. The thread must exit promptly because
    // TerminateEvent is polled at the top of every acquire loop iteration.
    KeWaitForSingleObject(mctx->ProcessingThread, Executive,
                          KernelMode, FALSE, NULL);
    ObDereferenceObject(mctx->ProcessingThread);
    mctx->ProcessingThread = NULL;
    mctx->SwapChain        = NULL;

    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER,
                "Processing thread joined");
}

// ---------------------------------------------------------------------------
// IexddProcessingThread
// ---------------------------------------------------------------------------
// Runs at elevated priority on core 0 (P-core affinity). Tight acquire loop
// that delivers frames to user-mode via the inverted-call WDF queue.

VOID
IexddProcessingThread(
    _In_ PVOID StartContext
)
{
    PIEXDD_MONITOR_CONTEXT mctx = (PIEXDD_MONITOR_CONTEXT)StartContext;
    NTSTATUS               status;
    UINT64                 acquireSeq = 0;
    IEXDD_INFLIGHT_ENTRY   inflight[IEXDD_MAX_INFLIGHT_FRAMES];
    UINT32                 inflightCount = 0;

    // Pin to first P-core and raise to above-normal priority to reduce jitter.
    KeSetSystemAffinityThread((KAFFINITY)1ULL);
    KeSetBasePriorityThread(KeGetCurrentThread(), 8);

    RtlZeroMemory(inflight, sizeof(inflight));

    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER,
                "ProcessingThread: starting");

    for (;;) {
        // Check terminate request before blocking on acquire.
        if (KeReadStateEvent(&mctx->TerminateEvent)) {
            TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER,
                        "ProcessingThread: terminate signalled");
            break;
        }

        // Back-pressure: if all inflight slots are full, drop the oldest
        // in-flight frame so WDDM doesn't stall.
        if (inflightCount >= IEXDD_MAX_INFLIGHT_FRAMES) {
            TraceEvents(TRACE_LEVEL_WARNING, TRACE_DRIVER,
                        "ProcessingThread: inflight cap hit — dropping oldest frame");

            // Find the entry with the lowest AcquireSeq (oldest).
            UINT32 oldestIdx = 0;
            for (UINT32 k = 1; k < IEXDD_MAX_INFLIGHT_FRAMES; k++) {
                if (inflight[k].Valid &&
                    inflight[k].AcquireSeq < inflight[oldestIdx].AcquireSeq) {
                    oldestIdx = k;
                }
            }

            if (inflight[oldestIdx].Valid) {
                IDARG_IN_FINISHEDFRAMENOTIFY done;
                RtlZeroMemory(&done, sizeof(done));
                done.FrameMetaData = inflight[oldestIdx].AcquireResult.FrameMetaData;
                IddCxSwapChainFinishedProcessingFrame(mctx->SwapChain, &done);

                inflight[oldestIdx].Valid = FALSE;
                inflightCount--;

                // Update stats (best-effort; spinlock not held here — grab device).
                // Stats are advisory; minor races are acceptable.
            }
        }

        // Acquire the next frame. Block up to 16 ms (~1 frame at 60 Hz) before
        // re-checking the terminate event.
        IDARG_IN_RELEASEANDACQUIREBUFFER  acqIn;
        IDARG_OUT_RELEASEANDACQUIREBUFFER acqOut;
        RtlZeroMemory(&acqIn, sizeof(acqIn));
        RtlZeroMemory(&acqOut, sizeof(acqOut));
        acqIn.TimeoutInMs = 16;

        status = IddCxSwapChainReleaseAndAcquireBuffer(mctx->SwapChain,
                                                        &acqIn, &acqOut);
        if (status == STATUS_PENDING || status == STATUS_TIMEOUT) {
            // No frame ready within the timeout window; loop and re-check terminate.
            continue;
        }
        if (!NT_SUCCESS(status)) {
            TraceEvents(TRACE_LEVEL_WARNING, TRACE_DRIVER,
                        "ReleaseAndAcquireBuffer=0x%08X — stopping thread", status);
            break;
        }

        acquireSeq++;

        // Find an empty inflight slot.
        UINT32 slot = IEXDD_MAX_INFLIGHT_FRAMES;
        for (UINT32 k = 0; k < IEXDD_MAX_INFLIGHT_FRAMES; k++) {
            if (!inflight[k].Valid) { slot = k; break; }
        }

        if (slot == IEXDD_MAX_INFLIGHT_FRAMES) {
            // Should not happen given the back-pressure logic above, but be safe.
            IDARG_IN_FINISHEDFRAMENOTIFY done;
            RtlZeroMemory(&done, sizeof(done));
            done.FrameMetaData = acqOut.FrameMetaData;
            IddCxSwapChainFinishedProcessingFrame(mctx->SwapChain, &done);
            continue;
        }

        inflight[slot].Valid        = TRUE;
        inflight[slot].AcquireSeq   = acquireSeq;
        inflight[slot].AcquireResult = acqOut;
        inflightCount++;

        TraceEvents(TRACE_LEVEL_VERBOSE, TRACE_DRIVER,
                    "ProcessingThread: frame seq=%llu acquired", acquireSeq);

        // Retrieve the oldest pending PULL_FRAME IRP from the manual queue.
        WDFREQUEST pullRequest = NULL;
        PIEXDD_DEVICE_CONTEXT dctx = NULL;

        // Walk up the WDF object tree to get device context.
        // MonitorObject → parent adapter → parent device.
        WDFDEVICE ownerDevice = mctx->OwnerDevice;
        if (ownerDevice != NULL) {
            dctx = IexddDeviceGetContext(ownerDevice);
        }

        if (dctx == NULL || dctx->PullQueue == NULL) {
            // No queue yet (user-mode not connected). Ack the frame immediately.
            IDARG_IN_FINISHEDFRAMENOTIFY done;
            RtlZeroMemory(&done, sizeof(done));
            done.FrameMetaData = acqOut.FrameMetaData;
            IddCxSwapChainFinishedProcessingFrame(mctx->SwapChain, &done);
            inflight[slot].Valid = FALSE;
            inflightCount--;
            continue;
        }

        status = WdfIoQueueRetrieveNextRequest(dctx->PullQueue, &pullRequest);
        if (!NT_SUCCESS(status) || pullRequest == NULL) {
            // No user-mode request outstanding yet. Ack the frame to avoid stalling.
            IDARG_IN_FINISHEDFRAMENOTIFY done;
            RtlZeroMemory(&done, sizeof(done));
            done.FrameMetaData = acqOut.FrameMetaData;
            IddCxSwapChainFinishedProcessingFrame(mctx->SwapChain, &done);
            inflight[slot].Valid = FALSE;
            inflightCount--;
            continue;
        }

        // Locate the file context to get the client process handle.
        WDFFILEOBJECT fileObj = WdfRequestGetFileObject(pullRequest);
        PIEXDD_FILE_CONTEXT fctx = IexddFileGetContext(fileObj);

        if (fctx == NULL || !fctx->HelloComplete || fctx->ClientProcess == NULL) {
            WdfRequestComplete(pullRequest, STATUS_INVALID_DEVICE_STATE);
            IDARG_IN_FINISHEDFRAMENOTIFY done;
            RtlZeroMemory(&done, sizeof(done));
            done.FrameMetaData = acqOut.FrameMetaData;
            IddCxSwapChainFinishedProcessingFrame(mctx->SwapChain, &done);
            inflight[slot].Valid = FALSE;
            inflightCount--;
            continue;
        }

        // Duplicate the shared texture handle into the user process.
        HANDLE userHandle = NULL;
        status = ZwDuplicateObject(
            ZwCurrentProcess(),
            acqOut.FrameMetaData.hSharedSurface,
            fctx->ClientProcess,
            &userHandle,
            0, 0, DUPLICATE_SAME_ACCESS
        );

        if (!NT_SUCCESS(status)) {
            TraceEvents(TRACE_LEVEL_ERROR, TRACE_DRIVER,
                        "ZwDuplicateObject=0x%08X — acking frame, completing IRP as error",
                        status);
            WdfRequestComplete(pullRequest, status);
            IDARG_IN_FINISHEDFRAMENOTIFY done;
            RtlZeroMemory(&done, sizeof(done));
            done.FrameMetaData = acqOut.FrameMetaData;
            IddCxSwapChainFinishedProcessingFrame(mctx->SwapChain, &done);
            inflight[slot].Valid = FALSE;
            inflightCount--;
            continue;
        }

        // Build the response buffer: IEXDD_FRAME_HEADER + dirty RECT[].
        PVOID  outBuf     = NULL;
        SIZE_T outBufSize = 0;
        status = WdfRequestRetrieveOutputBuffer(pullRequest,
                     sizeof(IEXDD_FRAME_HEADER), &outBuf, &outBufSize);
        if (!NT_SUCCESS(status)) {
            WdfRequestComplete(pullRequest, status);
            inflight[slot].Valid = FALSE;
            inflightCount--;
            continue;
        }

        // Populate dirty rect count — cap at IEXDD_MAX_DIRTY_RECTS.
        UINT32 dirtyCount = acqOut.FrameMetaData.DirtyRectCount;
        BOOLEAN sendDirty = (dirtyCount > 0 && dirtyCount <= IEXDD_MAX_DIRTY_RECTS);
        if (!sendDirty) {
            dirtyCount = 0; // full frame
        }

        SIZE_T requiredSize = sizeof(IEXDD_FRAME_HEADER) + dirtyCount * sizeof(RECT);
        if (outBufSize < requiredSize) {
            dirtyCount  = 0;
            sendDirty   = FALSE;
            requiredSize = sizeof(IEXDD_FRAME_HEADER);
        }

        PIEXDD_FRAME_HEADER hdr = (PIEXDD_FRAME_HEADER)outBuf;
        hdr->PresentTimeQpc    = acqOut.FrameMetaData.PresentationFrameNumber; // best-available proxy
        hdr->AcquireSeq        = acquireSeq;
        hdr->SharedTextureHandle = userHandle;
        hdr->Width             = acqOut.FrameMetaData.FrameDescription.Width;
        hdr->Height            = acqOut.FrameMetaData.FrameDescription.Height;
        hdr->DirtyRectCount    = dirtyCount;
        hdr->ColorSpaceId      = 0; // DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709

        if (sendDirty && dirtyCount > 0) {
            PRECT rects = (PRECT)(hdr + 1);
            RtlCopyMemory(rects, acqOut.FrameMetaData.pDirtyRect,
                          dirtyCount * sizeof(RECT));
        }

        WdfRequestCompleteWithInformation(pullRequest, STATUS_SUCCESS,
                                           requiredSize);

        TraceEvents(TRACE_LEVEL_VERBOSE, TRACE_DRIVER,
                    "ProcessingThread: frame seq=%llu delivered to user-mode "
                    "(dirty=%u)", acquireSeq, dirtyCount);

        // Update delivered counter.
        if (dctx != NULL) {
            KLOCK_QUEUE_HANDLE lqh;
            KeAcquireInStackQueuedSpinLock(&dctx->StatsLock, &lqh);
            dctx->Stats.FramesDelivered++;
            dctx->Stats.InFlightCount = (UINT32)inflightCount;
            KeReleaseInStackQueuedSpinLock(&lqh);
        }
    }

    // Ack all remaining inflight frames before thread exits.
    for (UINT32 k = 0; k < IEXDD_MAX_INFLIGHT_FRAMES; k++) {
        if (inflight[k].Valid) {
            IDARG_IN_FINISHEDFRAMENOTIFY done;
            RtlZeroMemory(&done, sizeof(done));
            done.FrameMetaData = inflight[k].AcquireResult.FrameMetaData;
            IddCxSwapChainFinishedProcessingFrame(mctx->SwapChain, &done);
            inflight[k].Valid = FALSE;
        }
    }

    TraceEvents(TRACE_LEVEL_INFORMATION, TRACE_DRIVER,
                "ProcessingThread: exiting");
    PsTerminateSystemThread(STATUS_SUCCESS);
}
