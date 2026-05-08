//! Wayland capture path.
//!
//! Once `EvdiMonitor` has plugged a virtual monitor, the compositor (Mutter,
//! KWin, Sway, Hyprland) sees a new connected output named e.g. `EVDI-1`.
//! We then open an `xdg-desktop-portal` ScreenCast session and ask the portal
//! to capture *just that output*.
//!
//! # Frame delivery
//!
//! Frames arrive as DMA-BUF file descriptors over a PipeWire stream.  We wrap
//! each one in an `ix_display::GpuFrame::DmaBuf` variant and push it to the
//! shared SPSC ring buffer.  The codec backend (Plan 5) ingests the fd
//! directly — no CPU copy, no intermediate buffer.
//!
//! # Permissions
//!
//! The portal asks the user to grant screen-capture permission once.  We
//! request `PersistMode::Application` so the grant is remembered across
//! daemon restarts.
//!
//! # Status
//!
//! The PipeWire stream-side receive loop (`run`) is scaffolded here with the
//! full interface; the `pipewire-rs` integration is completed in Plan 5 when
//! codec ingest is wired in.  All types and method signatures are final.

#![allow(dead_code, unused_imports)]

use crossbeam::queue::ArrayQueue;
use ix_display::{DamageRect, GpuFrame, GpuFrameKind};
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors arising from the Wayland / portal capture path.
#[derive(Debug, Error)]
pub enum WaylandError {
    /// The xdg-desktop-portal D-Bus service is not reachable.
    ///
    /// Typical fix: install `xdg-desktop-portal-wlr` (Sway/wlroots) or
    /// `xdg-desktop-portal-gnome` (GNOME) or `xdg-desktop-portal-kde` (KDE).
    #[error(
        "xdg-desktop-portal not available — \
         is xdg-desktop-portal-wlr, -gnome, or -kde installed and running?"
    )]
    NoPortal(String),

    /// The user explicitly denied the screencast permission request.
    #[error("user denied the screencast permission request in the portal dialog")]
    PermissionDenied,

    /// The portal handed us a session but no stream was selected.
    #[error("portal returned no PipeWire streams after the user confirmed the grant")]
    NoStreams,

    /// The PipeWire node associated with our screencast stream disappeared.
    #[error("PipeWire stream node disappeared — compositor may have closed the session")]
    NodeGone,

    /// An unexpected error in the portal D-Bus conversation.
    #[error("portal error: {0}")]
    Portal(String),
}

// ---------------------------------------------------------------------------
// WaylandCapture
// ---------------------------------------------------------------------------

/// Manages the lifecycle of a Wayland portal screencast session.
///
/// # Construction
///
/// Call `WaylandCapture::new` to create the struct.  Because portal calls are
/// asynchronous, the actual session is opened lazily when `start_session` is
/// called from an async context (e.g. from iextendd's Tokio runtime).
///
/// # Frame push
///
/// When the PipeWire stream loop runs (started by `run`), it pushes
/// `GpuFrame` values into `out`.  The encoder thread pops from the same queue.
pub struct WaylandCapture {
    out: Arc<ArrayQueue<GpuFrame>>,
    /// The connector name we will request in the portal (e.g. `"EVDI-1"`).
    connector_name: String,
    /// PipeWire node ID assigned by the portal after the user grants.
    /// `None` until the portal session has been established.
    pw_node_id: Option<u32>,
}

impl WaylandCapture {
    /// Create a new `WaylandCapture` targeting the given connector.
    ///
    /// This does **not** open the portal yet.  Call `start_session` from an
    /// async context to actually authenticate with xdg-desktop-portal.
    pub fn new(out: Arc<ArrayQueue<GpuFrame>>) -> Self {
        Self {
            out,
            connector_name: String::from("EVDI-1"),
            pw_node_id: None,
        }
    }

    /// Set the DRM connector name to request from the portal.
    ///
    /// Defaults to `"EVDI-1"`.  Change this before calling `start_session` if
    /// your compositor assigns a different name (run `xrandr | grep EVDI` or
    /// `wlr-randr | grep EVDI` to check).
    pub fn with_connector(mut self, name: impl Into<String>) -> Self {
        self.connector_name = name.into();
        self
    }

    /// Open a portal ScreenCast session and return the PipeWire node ID.
    ///
    /// Must be called from within a Tokio (or compatible async) runtime.
    ///
    /// After this call returns `Ok(node_id)`, pass `node_id` to `run` to
    /// start pulling frames.
    ///
    /// # Portal interaction
    ///
    /// 1. Opens a portal `ScreenCast` session.
    /// 2. Calls `SelectSources` requesting `Monitor`-type sources only,
    ///    cursor hidden, single source.
    /// 3. Calls `Start` — the portal may show a picker dialog to the user.
    /// 4. Calls `OpenPipeWireRemote` to get the PW fd.
    ///
    /// On subsequent runs (after the first grant), step 3 is silent because
    /// the grant was persisted with `PersistMode::Application`.
    ///
    /// # Note on ashpd version
    ///
    /// The plan specified `ashpd 0.8`; the workspace uses `0.9` which has a
    /// slightly different async executor interface.  The stub below uses the
    /// 0.9 API; the actual D-Bus calls are identical under the hood.
    pub async fn start_session(&mut self) -> Result<u32, WaylandError> {
        // ---------------------------------------------------------------
        // STUB — full implementation goes here in Plan 5 when we have a
        // running Tokio runtime and a real Wayland session to test against.
        //
        // Pseudocode:
        //   let proxy = ashpd::desktop::screencast::Screencast::new().await?;
        //   let session = proxy.create_session().await?;
        //   proxy.select_sources(&session, CursorMode::Hidden,
        //       BitFlags::from(SourceType::Monitor),
        //       false, Some(&self.connector_name), PersistMode::Application).await?;
        //   let response = proxy.start(&session, &Default::default()).await?;
        //   let streams = response.response()?.streams();
        //   let node_id = streams.first().ok_or(WaylandError::NoStreams)?.pipe_wire_node_id();
        //   let pw_fd = proxy.open_pipe_wire_remote(&session, Default::default()).await?;
        //   // hand pw_fd and node_id to pipewire-rs stream setup
        //   Ok(node_id)
        // ---------------------------------------------------------------
        info!(
            connector = %self.connector_name,
            "WaylandCapture::start_session called (stub — Plan 5 wires this)"
        );
        Err(WaylandError::Portal(
            "stub: portal integration not yet wired (Plan 5)".into(),
        ))
    }

    /// Run the PipeWire stream receive loop.
    ///
    /// This function blocks until the stream ends or an error occurs.  It is
    /// intended to run on a dedicated thread spawned by the daemon.
    ///
    /// For each frame received over PipeWire:
    ///
    /// 1. Extract the DMA-BUF fd from `buffer.datas[0]`.
    /// 2. Read damage rectangles from `SPA_META_VideoDamage` metadata if
    ///    present (fall back to full-frame damage if absent — happens on
    ///    PipeWire < 0.3.65).
    /// 3. Wrap in `GpuFrame::DmaBuf` and push to `self.out`.
    ///
    /// # Plan 5 hook
    ///
    /// The encoder calls `out.pop()` in a tight loop on its own thread.  No
    /// additional synchronisation is required.
    pub fn run(&self, _pw_node_id: u32) -> Result<(), WaylandError> {
        // STUB — pipewire-rs integration is completed in Plan 5.
        warn!("WaylandCapture::run is a stub — Plan 5 wires the PipeWire stream loop");
        Ok(())
    }

    /// Push a synthetic test frame.  Used by integration tests on hosts that
    /// have evdi but may not have a real Wayland portal.
    #[cfg(test)]
    pub fn push_test_frame(&self, width: u32, height: u32) {
        let frame = GpuFrame {
            kind: GpuFrameKind::DmaBuf {
                fd: -1,
                stride: width * 4,
                modifier: 0,
                format_fourcc: 0x34325258, // DRM_FORMAT_XRGB8888
            },
            width,
            height,
            damage: vec![DamageRect::full(width, height)],
            timestamp_us: now_us(),
        };
        let _ = self.out.push(frame);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Current time in microseconds since UNIX epoch.
fn now_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}
