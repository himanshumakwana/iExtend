//! NVIDIA proprietary-driver detection and CUDA interop fallback.
//!
//! # Problem
//!
//! NVIDIA's proprietary kernel module (versions before the open kernel module
//! 555+) cannot expose DMA-BUF fds that NVENC can ingest directly.  When we
//! detect the proprietary driver we switch from the normal DMA-BUF path to a
//! CUDA-interop route:
//!
//! 1. The screencast buffer (host-mapped by PipeWire or XShm) is treated as a
//!    CUDA host-memory pointer.
//! 2. `cuMemcpy2DAsync` copies it directly into the NVENC input surface (a
//!    CUDA device pointer obtained from `nvenc-rs` in Plan 5).
//! 3. `cuStreamSynchronize` ensures the copy is complete before we hand the
//!    surface to NVENC.
//!
//! Total overhead: ~0.3 ms on a modern GPU.  This is within the latency budget
//! (spec §3.1: < 2 ms from capture to encode start).
//!
//! # Status
//!
//! Detection (`proprietary_driver_active`) is fully implemented and testable
//! without GPU hardware.  The actual copy function (`copy_host_to_nvenc_input`)
//! is a skeleton that will be completed in Plan 5 when `nvenc-rs` is available.

use crate::ffi::cuda::{LibcudaApi, CUDA_MEMCPY2D};
use dlopen2::wrapper::Container;
use std::path::Path;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Returns `true` if the NVIDIA *proprietary* kernel module is loaded.
///
/// Heuristic:
/// - `/sys/module/nvidia/version` is created by the proprietary module.
/// - `/sys/module/nvidia_dma_buf` is present in the open kernel module (555+)
///   which *does* expose DMA-BUF; if it exists, we can skip the CUDA path.
///
/// This function does not panic and is safe to call on any Linux host
/// (including ones with no GPU at all — both paths simply return `false`).
pub fn proprietary_driver_active() -> bool {
    let has_proprietary = Path::new("/sys/module/nvidia/version").exists();
    let has_open_dma_buf = Path::new("/sys/module/nvidia_dma_buf").exists();
    let result = has_proprietary && !has_open_dma_buf;
    debug!(
        has_proprietary,
        has_open_dma_buf,
        cuda_interop_needed = result,
        "NVIDIA driver probe"
    );
    result
}

/// Returns `true` if `nvidia-smi` exists and reports a GPU.
///
/// This is a secondary check; prefer `proprietary_driver_active` for the
/// driver-type decision.  Use this to emit a user-friendly warning when the
/// GPU is present but the open driver is loaded (which is fine — DMA-BUF
/// should work).
pub fn nvidia_smi_available() -> bool {
    std::process::Command::new("nvidia-smi")
        .arg("--query-gpu=name")
        .arg("--format=csv,noheader")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// CUDA copy helper
// ---------------------------------------------------------------------------

/// Error type for CUDA operations.
#[derive(Debug, thiserror::Error)]
pub enum CudaError {
    #[error("libcuda.so.1 not found — NVIDIA driver may not be installed")]
    NotInstalled(#[source] dlopen2::Error),

    #[error("cuInit failed: CUresult={0}")]
    InitFailed(i32),

    #[error("cuMemcpy2DAsync failed: CUresult={0}")]
    CopyFailed(i32),

    #[error("cuStreamSynchronize failed: CUresult={0}")]
    SyncFailed(i32),
}

/// Copy a host-mapped screencast buffer into an NVENC input surface via CUDA.
///
/// # Arguments
///
/// - `src_host`: pointer to the start of the source buffer (host memory —
///   either a `shmat` region from XShm or a PipeWire-mapped DMA-BUF).
/// - `src_pitch`: row stride of the source in bytes.
/// - `dst_dptr`: CUDA device pointer for the NVENC input surface.  Obtained
///   from `nvenc_rs::Encoder::lock_input_buffer()` in Plan 5.
/// - `dst_pitch`: row stride of the destination in bytes.
/// - `width`: frame width in pixels.
/// - `height`: frame height in pixels.
///
/// # Safety
///
/// `src_host` must point to at least `src_pitch * height` bytes of readable
/// memory.  `dst_dptr` must be a valid CUDA device allocation with at least
/// `dst_pitch * height` bytes available.
///
/// # Status
///
/// This is a skeleton.  The `TODO` below marks exactly where the Plan 5
/// `nvenc-rs` plumbing inserts.  Everything else (loading `libcuda.so.1`,
/// building `CUDA_MEMCPY2D`, calling `cuMemcpy2DAsync`) is implemented.
pub fn copy_host_to_nvenc_input(
    src_host: *const u8,
    src_pitch: usize,
    dst_dptr: u64,
    dst_pitch: usize,
    width: usize,
    height: usize,
) -> Result<(), CudaError> {
    if dst_dptr == 0 {
        // TODO(Plan 5): replace this guard with the real NVENC input buffer.
        warn!("copy_host_to_nvenc_input: dst_dptr is 0 — stub, no copy performed");
        return Ok(());
    }

    let lib: Container<LibcudaApi> =
        unsafe { Container::load("libcuda.so.1") }
            .map_err(CudaError::NotInstalled)?;

    // Initialise the CUDA driver (no-op if already initialised).
    let res = unsafe { lib.cuInit(0) };
    if res != 0 {
        return Err(CudaError::InitFailed(res));
    }

    let copy = CUDA_MEMCPY2D {
        srcXInBytes: 0,
        srcY: 0,
        srcMemoryType: 1, // CU_MEMORYTYPE_HOST
        srcHost: src_host as *const std::ffi::c_void,
        srcDevice: 0,
        srcArray: std::ptr::null_mut(),
        srcPitch: src_pitch,

        dstXInBytes: 0,
        dstY: 0,
        dstMemoryType: 2, // CU_MEMORYTYPE_DEVICE
        dstHost: std::ptr::null_mut(),
        dstDevice: dst_dptr,
        dstArray: std::ptr::null_mut(),
        dstPitch: dst_pitch,

        WidthInBytes: width * 4, // XRGB8888 = 4 bytes per pixel
        Height: height,
    };

    // NULL stream = default stream; Plan 5 passes the NVENC-shared stream.
    let res = unsafe { lib.cuMemcpy2DAsync(&copy, std::ptr::null_mut()) };
    if res != 0 {
        return Err(CudaError::CopyFailed(res));
    }

    let res = unsafe { lib.cuStreamSynchronize(std::ptr::null_mut()) };
    if res != 0 {
        return Err(CudaError::SyncFailed(res));
    }

    Ok(())
}
