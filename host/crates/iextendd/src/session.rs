//! Session: wires capture → encode → transport.
#![allow(dead_code)]
//!
//! `Session` owns the three pumps that keep the streaming pipeline running:
//!
//! 1. **Capture pump** — polls `DisplaySource::capture_frame()` at 120 Hz and
//!    pushes frames to a lock-free ring buffer.
//! 2. **Encode pump** — drains the ring buffer, calls `Encoder::encode()`, and
//!    pushes `EncodedSlice`s to a `tokio::mpsc` channel.
//! 3. **Transport pump** — reads `EncodedSlice`s from the mpsc and forwards
//!    them to `VideoSink::write_slice()`. On a 250 ms tick it also reads
//!    the latest transport-CC feedback and calls `BitrateController::on_feedback()`
//!    → `Encoder::set_bitrate()`.
//!
//! All three pumps run as independent tokio tasks on the multi-thread runtime.
//! The ring buffer is sized at 8 frames; frames are dropped (not blocked) when
//! the encoder falls behind.
//!
//! ## Lifecycle
//! ```text
//! Session::new(capture, encoder, peer)
//!   └─ run() → spawns pumps, awaits shutdown signal
//! ```
//!
//! ## Plan-5 scope
//! The real `DisplaySource` (Plans 3/4) and the real WebRTC `Peer` (Plan 6)
//! plug in here once those land. For the smoke-loopback test a `FakeSource`
//! and the scaffold `Peer` are used instead.

use crossbeam_queue::ArrayQueue;
use ix_codec::Encoder;
use ix_display::{DisplaySource, GpuFrame};
use ix_rtc::bitrate_controller::CcFeedback;
use ix_rtc::Peer;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time;
use tracing::{debug, info, warn};

/// Capacity of the capture→encode ring buffer (frames).
const RING_CAPACITY: usize = 8;

/// Capacity of the encode→transport mpsc channel (slices).
const TRANSPORT_CHAN_DEPTH: usize = 16;

/// Bitrate-controller feedback tick interval.
const BITRATE_TICK: Duration = Duration::from_millis(250);

// ── fake frame source for smoke-loopback and unit tests ─────────────────────

/// A `DisplaySource` that emits a deterministic synthetic frame at 120 Hz.
///
/// Used by `smoke_loopback` and any test that needs a capture source without
/// real hardware. Produces 1920×1200 ShmCpu frames backed by a static buffer.
pub struct FakeSource {
    width: u32,
    height: u32,
    /// Pre-allocated pixel buffer (YUV420, 1.5 bytes/pixel).
    buf: Vec<u8>,
    /// Frame counter.
    count: u64,
}

impl FakeSource {
    /// Create a 1920×1200 fake source.
    pub fn default_1080p() -> Self {
        let w = 1920u32;
        let h = 1200u32;
        let len = w as usize * h as usize * 3 / 2;
        let mut buf = vec![0u8; len];
        // Simple grey ramp so each frame has different content in the Y plane.
        for (i, b) in buf.iter_mut().take(w as usize * h as usize).enumerate() {
            *b = (i % 256) as u8;
        }
        Self {
            width: w,
            height: h,
            buf,
            count: 0,
        }
    }
}

impl DisplaySource for FakeSource {
    fn create_virtual_monitor(
        &mut self,
        _mode: ix_display::DisplayMode,
    ) -> Result<ix_display::MonitorHandle, ix_display::DisplayError> {
        Ok(ix_display::MonitorHandle(0))
    }

    fn capture_frame(&mut self) -> Option<GpuFrame> {
        use ix_display::GpuFrameKind;

        self.count += 1;
        // Embed a simple timecode into the first 8 bytes of the Y plane so each
        // frame is unique and can be matched in the loopback test.
        let ts_bytes = self.count.to_le_bytes();
        self.buf[..8].copy_from_slice(&ts_bytes);

        Some(GpuFrame {
            kind: GpuFrameKind::ShmCpu {
                addr: self.buf.as_mut_ptr(),
                stride: self.width as usize,
            },
            width: self.width,
            height: self.height,
            damage: vec![ix_display::DamageRect::full(self.width, self.height)],
            timestamp_us: self.count * 8_333, // ~120 fps
        })
    }

    fn destroy(&mut self) {}
}

// ── Session ──────────────────────────────────────────────────────────────────

/// The main streaming session.
pub struct Session {
    capture: Arc<Mutex<Box<dyn DisplaySource>>>,
    encoder: Arc<Mutex<Box<dyn Encoder>>>,
    peer: Arc<Peer>,
    queue: Arc<ArrayQueue<GpuFrame>>,
}

impl Session {
    /// Create a new session from its three components.
    ///
    /// Does not start any tasks — call [`run`][Self::run] to begin streaming.
    pub fn new(capture: Box<dyn DisplaySource>, encoder: Box<dyn Encoder>, peer: Peer) -> Self {
        Self {
            capture: Arc::new(Mutex::new(capture)),
            encoder: Arc::new(Mutex::new(encoder)),
            peer: Arc::new(peer),
            queue: Arc::new(ArrayQueue::new(RING_CAPACITY)),
        }
    }

    /// Run the session: spawn the three pumps and await until all complete.
    ///
    /// Each pump runs until it receives a cancellation signal or the
    /// underlying channel closes (i.e. `peer` is dropped).
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (slice_tx, slice_rx) = mpsc::channel::<ix_codec::EncodedSlice>(TRANSPORT_CHAN_DEPTH);

        let cap_handle = self.spawn_capture(slice_tx.clone());
        let enc_handle = self.spawn_encode(slice_rx);
        let bc_handle = self.spawn_bitrate_controller();

        info!("session: all pumps running");
        let _ = tokio::join!(cap_handle, enc_handle, bc_handle);
        info!("session: all pumps stopped");
        Ok(())
    }

    // ── capture pump ─────────────────────────────────────────────────────────

    fn spawn_capture(
        &self,
        _slice_tx: mpsc::Sender<ix_codec::EncodedSlice>,
    ) -> tokio::task::JoinHandle<()> {
        let capture = self.capture.clone();
        let queue = self.queue.clone();
        let _encoder = self.encoder.clone();

        tokio::spawn(async move {
            // 120 Hz → ~8.33 ms between frames.
            let mut interval = time::interval(Duration::from_micros(8_333));
            loop {
                interval.tick().await;

                let frame_opt = {
                    let mut src = capture.lock().await;
                    src.capture_frame()
                };

                if let Some(frame) = frame_opt {
                    // Push to ring. If the ring is full, drop the oldest and push.
                    if queue.push(frame).is_err() {
                        debug!("capture: ring buffer full, dropping oldest frame");
                        let _ = queue.pop();
                        // The new frame is lost — acceptable; encoder will get
                        // the next one. (In production we'd track drop count.)
                    }
                }
            }
        })
    }

    // ── encode pump ──────────────────────────────────────────────────────────

    fn spawn_encode(
        &self,
        slice_rx: mpsc::Receiver<ix_codec::EncodedSlice>,
    ) -> tokio::task::JoinHandle<()> {
        let queue = self.queue.clone();
        let encoder = self.encoder.clone();
        let peer = self.peer.clone();

        tokio::spawn(async move {
            let mut _rx = slice_rx; // keep alive
            loop {
                // Pop a frame from the capture ring.
                let frame = match queue.pop() {
                    Some(f) => f,
                    None => {
                        // Ring is empty; yield before retrying.
                        tokio::task::yield_now().await;
                        continue;
                    }
                };

                // Encode the frame.
                let slice_result = {
                    let mut enc = encoder.lock().await;
                    enc.encode(&frame, &frame.damage)
                };

                match slice_result {
                    Ok(slice) => {
                        if let Err(e) = peer.video.write_slice(&slice).await {
                            warn!("encode: transport write failed: {e}");
                        }
                    }
                    Err(e) => {
                        warn!("encode: encode failed: {e}");
                    }
                }
            }
        })
    }

    // ── bitrate controller pump ───────────────────────────────────────────────

    fn spawn_bitrate_controller(&self) -> tokio::task::JoinHandle<()> {
        let peer = self.peer.clone();
        let encoder = self.encoder.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(BITRATE_TICK);
            loop {
                interval.tick().await;

                // Synthetic feedback (real impl: read from RTCPeerConnection stats).
                let fb = CcFeedback::new(0.0, 20.0);

                let new_kbps = {
                    let mut bc = peer.bitrate.lock().await;
                    bc.on_feedback(fb, BITRATE_TICK)
                };

                if let Some(kbps) = new_kbps {
                    let mut enc = encoder.lock().await;
                    enc.set_bitrate(kbps);
                    debug!(kbps, "bitrate controller updated encoder");
                }
            }
        })
    }
}
