//! Inverted-call IOCTL pipe — device-file open, HELLO handshake, and the
//! `DeviceIoControl` wrappers used by the frame pump.
//!
//! "Inverted call" means user-mode issues a `IOCTL_IEXDD_PULL_FRAME` and
//! blocks; the kernel completes the IRP when a frame is ready. This avoids
//! polling and keeps the capture latency as low as a single scheduling
//! round-trip (~100 µs on a healthy system).
//!
//! # Windows-only
//! This module is compiled only on `cfg(windows)`. All Windows API calls
//! go through the `windows` crate (0.58).

#![cfg(windows)]

use std::ffi::c_void;
use std::mem;
use std::os::windows::io::RawHandle;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;

use crate::error::{Error, Result};
use crate::iddcx_bindings::{
    GUID, IEXDD_FRAME_HEADER, IEXDD_FRAME_RELEASE, IEXDD_HELLO, IEXDD_MAX_DIRTY_RECTS,
    IEXDD_PROTOCOL_VERSION, IEXDD_STATS, IOCTL_IEXDD_HELLO, IOCTL_IEXDD_PULL_FRAME,
    IOCTL_IEXDD_QUERY_STATS, IOCTL_IEXDD_RELEASE_FRAME, RECT,
};

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// Owns the raw `HANDLE` to `\\.\IExtendDisplay` and provides typed IOCTL
/// wrappers. `Clone` is not derived — use `try_clone` which duplicates the
/// handle.
pub struct Connection {
    handle: HANDLE,
}

// SAFETY: HANDLE is a pointer-sized Windows kernel object reference. It is
// valid to send to another thread after CreateFile returns — the kernel
// serialises concurrent I/O on the same handle.
unsafe impl Send for Connection {}

impl Connection {
    /// Open `\\.\IExtendDisplay` and exchange the HELLO handshake.
    pub fn open() -> Result<Self> {
        let path: Vec<u16> = crate::iddcx_bindings::IEXDD_DEVICE_PATH
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        // SAFETY: CreateFileW is an FFI call; arguments are well-typed.
        let handle = unsafe {
            CreateFileW(
                windows::core::PCWSTR(path.as_ptr()),
                0x4000_0000u32 | 0x8000_0000u32, // GENERIC_READ | GENERIC_WRITE
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                HANDLE::default(),
            )
        };

        let handle = handle.map_err(|_| {
            Error::DriverNotInstalled(crate::iddcx_bindings::IEXDD_DEVICE_PATH.into())
        })?;

        let mut conn = Self { handle };
        conn.hello()?;
        Ok(conn)
    }

    /// Duplicate the underlying handle into a new `Connection`. Used to give
    /// the frame-pump thread its own handle.
    pub fn try_clone(&self) -> Result<Self> {
        use windows::Win32::Foundation::DuplicateHandle;
        use windows::Win32::System::Threading::GetCurrentProcess;

        let proc = unsafe { GetCurrentProcess() };
        let mut new_handle = HANDLE::default();

        let ok = unsafe {
            DuplicateHandle(
                proc,
                self.handle,
                proc,
                &mut new_handle,
                0,
                windows::Win32::Foundation::BOOL(0), // bInheritHandle = FALSE
                windows::Win32::Foundation::DUPLICATE_HANDLE_OPTIONS(0x0002), // DUPLICATE_SAME_ACCESS
            )
        };

        ok.map_err(Error::Windows)?;
        Ok(Self { handle: new_handle })
    }

    // -----------------------------------------------------------------------
    // IOCTL helpers
    // -----------------------------------------------------------------------

    /// Perform the HELLO handshake. Called exactly once per handle.
    fn hello(&mut self) -> Result<()> {
        // Generate a random nonce using the process ID + stack address.
        let nonce = GUID {
            Data1: std::process::id(),
            Data2: 0xCDAB,
            Data3: 0xEF01,
            Data4: [0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01],
        };

        let input = IEXDD_HELLO {
            ProtocolVersion: IEXDD_PROTOCOL_VERSION,
            ClientPid: std::process::id(),
            ClientNonce: nonce,
        };

        let mut output = IEXDD_HELLO {
            ProtocolVersion: 0,
            ClientPid: 0,
            ClientNonce: GUID::default(),
        };

        let returned = self.ioctl_buffered(
            IOCTL_IEXDD_HELLO,
            &input as *const _ as *const c_void,
            mem::size_of::<IEXDD_HELLO>() as u32,
            &mut output as *mut _ as *mut c_void,
            mem::size_of::<IEXDD_HELLO>() as u32,
        )?;

        if returned < mem::size_of::<IEXDD_HELLO>() as u32 {
            return Err(Error::Protocol("HELLO response truncated".into()));
        }

        if output.ProtocolVersion != IEXDD_PROTOCOL_VERSION {
            return Err(Error::ProtocolVersion {
                expected: IEXDD_PROTOCOL_VERSION,
                got: output.ProtocolVersion,
            });
        }

        tracing::info!(
            "iexdd HELLO complete: protocol={} pid={}",
            output.ProtocolVersion,
            input.ClientPid
        );

        Ok(())
    }

    /// Block until the kernel delivers the next frame, then return the raw
    /// frame header and dirty rects.
    ///
    /// This call is synchronous and blocks the calling thread. Use from a
    /// dedicated `frame_pump` thread only.
    pub fn pull_frame(&self) -> Result<(IEXDD_FRAME_HEADER, Vec<RECT>)> {
        // Allocate enough buffer for the header + maximum dirty rects.
        let max_buf =
            mem::size_of::<IEXDD_FRAME_HEADER>() + IEXDD_MAX_DIRTY_RECTS * mem::size_of::<RECT>();

        let mut buf: Vec<u8> = vec![0u8; max_buf];

        let returned = self.ioctl_buffered(
            IOCTL_IEXDD_PULL_FRAME,
            std::ptr::null(),
            0,
            buf.as_mut_ptr() as *mut c_void,
            max_buf as u32,
        )?;

        if (returned as usize) < mem::size_of::<IEXDD_FRAME_HEADER>() {
            return Err(Error::Protocol("PULL_FRAME response too short".into()));
        }

        // SAFETY: buf is large enough and initialised by the kernel.
        let header: IEXDD_FRAME_HEADER =
            unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const IEXDD_FRAME_HEADER) };

        let mut rects = Vec::new();
        let dirty_count = header.DirtyRectCount.min(IEXDD_MAX_DIRTY_RECTS as u32) as usize;
        let rects_offset = mem::size_of::<IEXDD_FRAME_HEADER>();
        let expected_total = rects_offset + dirty_count * mem::size_of::<RECT>();

        if returned as usize >= expected_total && dirty_count > 0 {
            // SAFETY: bytes [rects_offset..expected_total] are RECT structs
            // written by the kernel.
            for i in 0..dirty_count {
                let offset = rects_offset + i * mem::size_of::<RECT>();
                let r: RECT =
                    unsafe { std::ptr::read_unaligned(buf[offset..].as_ptr() as *const RECT) };
                rects.push(r);
            }
        }

        Ok((header, rects))
    }

    /// Post a frame release to the kernel, freeing the in-flight slot and
    /// allowing WDDM to reclaim the swapchain buffer.
    pub fn release_frame(&self, acquire_seq: u64) -> Result<()> {
        let input = IEXDD_FRAME_RELEASE {
            AcquireSeq: acquire_seq,
        };

        self.ioctl_buffered(
            IOCTL_IEXDD_RELEASE_FRAME,
            &input as *const _ as *const c_void,
            mem::size_of::<IEXDD_FRAME_RELEASE>() as u32,
            std::ptr::null_mut(),
            0,
        )?;

        Ok(())
    }

    /// Pull a snapshot of the driver's telemetry counters.
    pub fn query_stats(&self) -> Result<IEXDD_STATS> {
        let mut stats = IEXDD_STATS::default();

        self.ioctl_buffered(
            IOCTL_IEXDD_QUERY_STATS,
            std::ptr::null(),
            0,
            &mut stats as *mut _ as *mut c_void,
            mem::size_of::<IEXDD_STATS>() as u32,
        )?;

        Ok(stats)
    }

    // -----------------------------------------------------------------------
    // Low-level DeviceIoControl wrapper
    // -----------------------------------------------------------------------

    fn ioctl_buffered(
        &self,
        code: u32,
        in_buf: *const c_void,
        in_size: u32,
        out_buf: *mut c_void,
        out_size: u32,
    ) -> Result<u32> {
        let mut returned: u32 = 0;

        // SAFETY: all pointer arguments are either null or valid for their
        // respective sizes; the kernel will not write past out_size bytes.
        let ok = unsafe {
            DeviceIoControl(
                self.handle,
                code,
                if in_buf.is_null() { None } else { Some(in_buf) },
                in_size,
                if out_buf.is_null() {
                    None
                } else {
                    Some(out_buf)
                },
                out_size,
                Some(&mut returned),
                None, // no OVERLAPPED — synchronous
            )
        };

        ok.map_err(Error::Windows)?;
        Ok(returned)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // SAFETY: handle was opened by CreateFileW and is valid until here.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}
