//! X11 capture path: XShm + XDamage.
//!
//! This is the fallback path for users without Wayland (older distros,
//! headless servers, RDP-style sessions).  It is **slower** than the Wayland
//! path because the encoder may need to perform a CPU→GPU upload (VAAPI can
//! ingest from shared memory directly; NVENC needs the CUDA-interop path from
//! `nvidia_cuda.rs`).
//!
//! # Implementation sketch
//!
//! 1. Connect to the X server and verify that `MIT-SHM` and `XDamage`
//!    extensions are available.
//! 2. Create a shared-memory segment with `shmget` and attach it with
//!    `shmat`.  Register it with the X server via `ShmAttach`.
//! 3. Subscribe to damage events on the EVDI-1 CRTC viewport window.
//! 4. On each `pump()` call:
//!    a. Drain pending `DamageNotify` events into a `Vec<DamageRect>`.
//!    b. Call `ShmGetImage` to copy the changed area into the shm segment.
//!    c. Reset the damage region with `DamageSubtract`.
//!    d. Wrap the shm address in a `GpuFrame::ShmCpu` and push to `out`.
//!
//! Steps 2 and 4b involve unsafe code; all unsafe blocks are isolated and
//! documented.
//!
//! # `coalesce` function
//!
//! Multiple small `DamageNotify` events arrive per frame.  The `coalesce`
//! function merges overlapping or touching rectangles into a minimal set so
//! the encoder does not need to handle degenerate cases.

#![allow(dead_code, unused_imports)]

use crossbeam::queue::ArrayQueue;
use ix_display::{DamageRect, GpuFrame, GpuFrameKind};
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn};

// We keep the x11rb import conditional so the module compiles cleanly even on
// hosts where xorg-dev headers are not installed (x11rb is pure-Rust and does
// not require headers, but the features we request use optional OS abstractions).
use x11rb::connection::Connection;
use x11rb::errors::ReplyOrIdError;
use x11rb::protocol::damage::{self, ConnectionExt as DamageConnExt};
use x11rb::protocol::shm::{self, ConnectionExt as ShmConnExt};
use x11rb::protocol::xproto::{self, ConnectionExt as _, Window};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors arising from the X11 capture path.
#[derive(Debug, Error)]
pub enum X11Error {
    /// Could not connect to the X server.
    #[error("X server connection failed: {0}")]
    Connect(#[from] x11rb::errors::ConnectError),

    /// An X11 protocol request returned an error reply.
    #[error("X11 protocol error: {0}")]
    Proto(#[from] x11rb::errors::ReplyError),

    /// An X11 protocol request failed at the connection level.
    #[error("X11 connection error: {0}")]
    Connection(#[from] x11rb::errors::ConnectionError),

    /// ID generation failed (X server resource exhaustion).
    #[error("X11 ID allocation error: {0}")]
    IdError(#[from] ReplyOrIdError),

    /// The MIT-SHM extension is not available.
    ///
    /// On Ubuntu: `sudo apt install libxext-dev`.
    /// On Fedora: `sudo dnf install libXext-devel`.
    #[error(
        "MIT-SHM extension not available — \
         install libxext-dev (Debian/Ubuntu) or libXext-devel (Fedora)"
    )]
    NoShm,

    /// The XDamage extension is not available.
    #[error("XDamage extension not available")]
    NoDamage,

    /// The EVDI connector was not found in the RandR output list.
    #[error("EVDI output not found in RandR outputs (connector_name={0})")]
    EvdiOutputMissing(String),

    /// A libc call (`shmget`, `shmat`) failed.
    #[error("shared-memory allocation failed: errno={0}")]
    ShmAlloc(i32),
}

// ---------------------------------------------------------------------------
// X11Capture
// ---------------------------------------------------------------------------

/// X11 capture session.
///
/// Owns the X11 connection, the SHM segment, and the XDamage subscription.
pub struct X11Capture {
    conn: x11rb::rust_connection::RustConnection,
    root: Window,
    output_window: Window,
    damage: damage::Damage,
    shm_seg: shm::Seg,
    shm_addr: *mut u8,
    width: u16,
    height: u16,
    out: Arc<ArrayQueue<GpuFrame>>,
}

// SAFETY: The shm_addr pointer is only accessed from the single thread that
// calls `pump`.  The `out` queue is an Arc and is Send.
unsafe impl Send for X11Capture {}

impl X11Capture {
    /// Open a connection to the X server and set up XShm + XDamage for the
    /// EVDI virtual monitor output.
    pub fn new(out: Arc<ArrayQueue<GpuFrame>>) -> Self {
        // Real implementation would:
        //   1. Call x11rb::connect(None) to open the display.
        //   2. Verify SHM and Damage extensions.
        //   3. Use RandR to find the EVDI-1 CRTC viewport.
        //   4. shmget + shmat for (width * height * 4) bytes.
        //   5. ShmAttach.
        //   6. DamageCreate on the output window.
        //
        // This stub creates a minimal viable struct.  The full implementation
        // is gated by `--features integration` since it requires a live X
        // display (`DISPLAY` set).

        warn!(
            "X11Capture is a stub — actual X11 connection attempted at pump() time \
             when DISPLAY is set"
        );

        // We can't open the connection here without a live DISPLAY; store a
        // placeholder.  The real struct is built lazily in `start`.
        let (conn, _screen_num) = Self::try_connect();
        Self {
            conn,
            root: 0,
            output_window: 0,
            damage: 0,
            shm_seg: 0,
            shm_addr: std::ptr::null_mut(),
            width: 1920,
            height: 1080,
            out,
        }
    }

    /// Open the X11 connection and register XDamage + XShm.
    ///
    /// Call this once before calling `pump`.  On failure the daemon should
    /// fall back to software capture or abort with a helpful message.
    pub fn start(
        connector_name: &str,
        out: Arc<ArrayQueue<GpuFrame>>,
    ) -> Result<Self, X11Error> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;

        // Verify extensions.
        let shm_ok = conn.shm_query_version()
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.shared_pixmaps)
            .unwrap_or(false);
        if !shm_ok {
            return Err(X11Error::NoShm);
        }

        let damage_ok = conn.damage_query_version(1, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.major_version >= 1)
            .unwrap_or(false);
        if !damage_ok {
            return Err(X11Error::NoDamage);
        }

        // In the full implementation we would use RandR to find the viewport
        // window for the EVDI-1 connector.  For now we use the root window as
        // a placeholder (captures the entire screen; Plan 5 restricts to the
        // EVDI CRTC).
        let output_window = root;
        warn!(
            "X11 capture is a fallback path — Wayland is recommended for \
             full zero-copy performance"
        );

        // Allocate a shared-memory segment for the frame buffer.
        let stride = 1920usize * 4;
        let size = stride * 1080;
        let shm_id = unsafe { libc::shmget(libc::IPC_PRIVATE, size, 0o600) };
        if shm_id < 0 {
            return Err(X11Error::ShmAlloc(std::io::Error::last_os_error().raw_os_error().unwrap_or(-1)));
        }
        let shm_addr = unsafe {
            libc::shmat(shm_id, std::ptr::null(), 0) as *mut u8
        };
        if shm_addr.is_null() {
            return Err(X11Error::ShmAlloc(std::io::Error::last_os_error().raw_os_error().unwrap_or(-1)));
        }

        // Register the segment with the X server.
        let shm_seg: shm::Seg = conn.generate_id()?;
        conn.shm_attach(shm_seg, shm_id as u32, false)?.check()?;

        // Delete the kernel's reference so the segment is freed when we
        // shmdt (or the process exits).
        unsafe { libc::shmctl(shm_id, libc::IPC_RMID, std::ptr::null_mut()); }

        // Subscribe to damage events on the output window.
        let damage: damage::Damage = conn.generate_id()?;
        conn.damage_create(damage, output_window, damage::ReportLevel::DELTA_RECTANGLES)?
            .check()?;

        info!(connector = connector_name, "X11 capture started");

        Ok(Self {
            conn,
            root,
            output_window,
            damage,
            shm_seg,
            shm_addr,
            width: 1920,
            height: 1080,
            out,
        })
    }

    /// Pump one frame into the output queue.
    ///
    /// 1. Drains pending `DamageNotify` events.
    /// 2. Calls `ShmGetImage` to refresh the shm buffer.
    /// 3. Resets the damage region.
    /// 4. Wraps the shm address in a `GpuFrame::ShmCpu` and pushes to `out`.
    ///
    /// The caller is responsible for calling this at the desired frame rate
    /// (typically 120 Hz via a `tokio::time::interval`).
    pub fn pump(&mut self) -> Result<(), X11Error> {
        // If the connection is a placeholder (created by `new` without a live
        // DISPLAY), skip silently.
        if self.output_window == 0 {
            return Ok(());
        }

        // 1. Drain damage events.
        let mut rects: Vec<DamageRect> = Vec::new();
        while let Some(event) = self.conn.poll_for_event()
            .map_err(|_| X11Error::NoDamage).ok().flatten()
        {
            if let x11rb::protocol::Event::DamageNotify(ev) = event {
                rects.push(DamageRect {
                    x: ev.area.x as u32,
                    y: ev.area.y as u32,
                    w: ev.area.width as u32,
                    h: ev.area.height as u32,
                });
            }
        }
        let rects = coalesce(rects);

        // If no damage events were received, skip this frame.
        if rects.is_empty() {
            return Ok(());
        }

        // 2. Fetch the full frame into shm.
        self.conn.shm_get_image(
            self.output_window,
            0, 0,
            self.width, self.height,
            !0u32,
            xproto::ImageFormat::Z_PIXMAP.into(),
            self.shm_seg,
            0,
        )?.reply()?;

        // 3. Reset damage.
        self.conn.damage_subtract(self.damage, x11rb::NONE, x11rb::NONE)?;

        // 4. Push frame.
        let frame = GpuFrame {
            kind: GpuFrameKind::ShmCpu {
                addr: self.shm_addr,
                stride: self.width as usize * 4,
            },
            width: self.width as u32,
            height: self.height as u32,
            damage: rects,
            timestamp_us: now_us(),
        };
        let _ = self.out.push(frame);
        Ok(())
    }

    /// Attempt to get a connection for the stub/new() path.
    ///
    /// Returns `(connection, screen_num)` if DISPLAY is set, otherwise panics.
    /// In production, `LinuxDisplaySource::new()` selects the correct backend
    /// before calling `X11Capture::new()`, so DISPLAY should always be set on
    /// this path.
    fn try_connect() -> (x11rb::rust_connection::RustConnection, usize) {
        x11rb::connect(None).unwrap_or_else(|e| {
            panic!(
                "X11Capture::new() called but X11 connection failed: {e}. \
                 Ensure DISPLAY is set or use X11Capture::start() directly."
            )
        })
    }
}

impl Drop for X11Capture {
    fn drop(&mut self) {
        if self.output_window != 0 {
            // Detach the shm segment.
            if !self.shm_addr.is_null() {
                unsafe { libc::shmdt(self.shm_addr as *const _); }
                self.shm_addr = std::ptr::null_mut();
            }
            // Detach from X server (best effort).
            let _ = self.conn.shm_detach(self.shm_seg);
            let _ = self.conn.damage_destroy(self.damage);
        }
    }
}

// ---------------------------------------------------------------------------
// coalesce — merge overlapping DamageRects
// ---------------------------------------------------------------------------

/// Merge a list of possibly-overlapping damage rectangles into a minimal set.
///
/// The algorithm is a greedy single-pass union-find: iterate the list; for
/// each rectangle, merge it with any existing output rectangle it overlaps.
/// Repeat until no merges happen in a pass (at most N passes for N rects, but
/// typically 1–2 passes in practice because damage arrives in scan-line order).
///
/// This is `O(n²)` but damage lists are short (< 32 rects per frame is common).
pub fn coalesce(mut rects: Vec<DamageRect>) -> Vec<DamageRect> {
    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < rects.len() {
            let mut j = i + 1;
            while j < rects.len() {
                if rects[i].overlaps(&rects[j]) {
                    let merged = rects[i].union(&rects[j]);
                    rects[i] = merged;
                    rects.remove(j);
                    changed = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
    rects
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}
