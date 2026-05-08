//! Minimal libcuda binding for the NVIDIA proprietary-driver fallback.
//!
//! We only declare the four symbols we actually call; the full CUDA driver API
//! has thousands of entry points we don't need.  Loading happens at runtime so
//! there is no link-time dependency on `libcuda.so.1`.

#![allow(non_camel_case_types, non_snake_case, dead_code)]

use dlopen2::wrapper::WrapperApi;
use std::os::raw::{c_int, c_uint, c_void};

// ---------------------------------------------------------------------------
// Type aliases matching the CUDA driver API headers.
// ---------------------------------------------------------------------------

/// Error code returned by every CUDA driver call.  0 = CUDA_SUCCESS.
pub type CUresult = c_int;

/// A GPU-side device memory address (virtual address in the CUDA context).
pub type CUdeviceptr = u64;

/// Opaque handle to a CUDA context.
pub type CUcontext = *mut c_void;

/// Opaque handle to a CUDA stream (command queue).
pub type CUstream = *mut c_void;

/// CUDA memory-type selector used in `CUDA_MEMCPY2D`.
#[repr(u32)]
#[allow(dead_code)]
pub enum CUmemorytype {
    Host = 1,
    Device = 2,
    Array = 3,
    Unified = 4,
}

// ---------------------------------------------------------------------------
// CUDA_MEMCPY2D — describes a 2-D memory copy operation.
// ---------------------------------------------------------------------------

/// Parameters for `cuMemcpy2DAsync`.
///
/// Fill `srcMemoryType`/`dstMemoryType` from `CUmemorytype`, then set the
/// appropriate source/destination union fields.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct CUDA_MEMCPY2D {
    // Source
    pub srcXInBytes: usize,
    pub srcY: usize,
    pub srcMemoryType: c_uint,
    pub srcHost: *const c_void,
    pub srcDevice: CUdeviceptr,
    pub srcArray: *mut c_void,
    pub srcPitch: usize,
    // Destination
    pub dstXInBytes: usize,
    pub dstY: usize,
    pub dstMemoryType: c_uint,
    pub dstHost: *mut c_void,
    pub dstDevice: CUdeviceptr,
    pub dstArray: *mut c_void,
    pub dstPitch: usize,
    // Extents
    pub WidthInBytes: usize,
    pub Height: usize,
}

// SAFETY: The raw pointers are only valid for the duration of a single
// cuMemcpy2DAsync call; callers must ensure lifetimes.
unsafe impl Send for CUDA_MEMCPY2D {}

// ---------------------------------------------------------------------------
// WrapperApi — populated by Container::load("libcuda.so.1")
// ---------------------------------------------------------------------------

/// Dynamically-loaded libcuda API surface.
///
/// Instantiate with:
/// ```rust,ignore
/// let cuda: Container<LibcudaApi> =
///     unsafe { Container::load("libcuda.so.1") }?;
/// ```
#[derive(WrapperApi)]
pub struct LibcudaApi {
    /// Initialise the CUDA driver.  Flags must be 0.
    cuInit: unsafe extern "C" fn(flags: c_uint) -> CUresult,

    /// Query the CUDA context bound to the calling thread.
    cuCtxGetCurrent: unsafe extern "C" fn(ctx: *mut CUcontext) -> CUresult,

    /// Asynchronous 2-D memcpy.
    ///
    /// Used to copy from a host-mapped screencast buffer (srcMemoryType=HOST)
    /// into an NVENC input surface (dstMemoryType=DEVICE).
    cuMemcpy2DAsync: unsafe extern "C" fn(copy: *const CUDA_MEMCPY2D, stream: CUstream) -> CUresult,

    /// Block the CPU until all operations on `stream` have completed.
    cuStreamSynchronize: unsafe extern "C" fn(stream: CUstream) -> CUresult,
}
