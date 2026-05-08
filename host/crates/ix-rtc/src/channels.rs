//! Typed DataChannel wrappers: `input` (unreliable) and `control` (reliable).
//!
//! ## Wire layout
//! - **input** DataChannel: `ordered=false`, `max_retransmits=0`. Used for
//!   pointer/touch/pencil events from the iPad. Latency beats reliability here;
//!   a dropped input packet is immediately superseded by the next one.
//! - **control** DataChannel: `ordered=true`, no retransmit limit. Used for
//!   heartbeat, cursor sprites, mode changes, latency probes, and session
//!   management messages.
//!
//! ## Message format
//! `ControlMessage` is JSON-tagged (`serde_json`). Input packets are a fixed
//! 32-byte binary frame (efficient; the iPad sends ~120/s).
//!
//! ## Scaffold
//! In this scaffold the DataChannels are backed by in-process `mpsc` queues
//! rather than real WebRTC `RTCDataChannel` handles. The wiring matches the
//! real API shape so that Plan 6 can swap the backing without touching callers.

use crate::RtcError;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::warn;

// ── Control message type ─────────────────────────────────────────────────────

/// Messages sent on the reliable control DataChannel.
///
/// Serialised as JSON with a `"kind"` discriminant tag (snake_case).
#[allow(missing_docs)] // variant fields documented at the enum level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlMessage {
    /// Host → iPad: liveness probe; iPad replies with `heartbeat_ack`.
    /// `seq` monotonically increases; `sent_us` is the host clock in µs.
    Heartbeat {
        /// Monotonic sequence number.
        seq: u32,
        /// Sender's clock in microseconds.
        sent_us: i64,
    },
    /// iPad → host: ack for a `heartbeat` or latency probe.
    HeartbeatAck {
        /// Echo of the `seq` being acked.
        ack_seq: u32,
        /// iPad receive timestamp (µs).
        recv_us: i64,
        /// iPad send timestamp (µs).
        send_us: i64,
    },
    /// Host → iPad: change display mode (e.g. switch to 60 fps).
    SetMode {
        /// New mode identifier string.
        mode: String,
    },
    /// Host → iPad: cursor position + sprite update.
    Cursor {
        /// X position in Q16.16 fixed-point display coordinates.
        x_q16: i32,
        /// Y position in Q16.16 fixed-point display coordinates.
        y_q16: i32,
        /// Sprite atlas ID for the cursor image.
        sprite_id: u32,
        /// Cursor hotspot (x, y) in pixels within the sprite.
        hotspot: (i16, i16),
    },
    /// Host → iPad: one-way latency probe (iPad echoes this back).
    LatencyProbe {
        /// Probe identifier (echoed in ack).
        id: u32,
        /// Host send timestamp in µs.
        sent_us: i64,
    },
    /// iPad → host: echo of a `latency_probe`.
    LatencyProbeAck {
        /// Echoed probe id.
        id: u32,
        /// iPad receive timestamp in µs.
        recv_us: i64,
    },
    /// Either side: graceful session termination.
    EndSession {
        /// Human-readable termination reason.
        reason: String,
    },
    /// iPad → host: app backgrounded (pause encode when safe to do so).
    Background,
    /// iPad → host: app foregrounded again.
    Foreground,
}

// ── Input channel ────────────────────────────────────────────────────────────

/// Unreliable, unordered DataChannel for input events (pointer, touch, Pencil).
///
/// Packets are fixed 32 bytes (spec §8.1). In Plan 8 the layout is:
/// `[kind:u8, buttons:u8, x:f32, y:f32, pressure:f32, tilt_x:f32, tilt_y:f32,
///  azimuth:f32, twist:f32, timestamp:u32, reserved:2]` = 32 bytes.
pub struct InputChannel {
    tx: mpsc::UnboundedSender<[u8; 32]>,
    rx: Arc<Mutex<mpsc::UnboundedReceiver<[u8; 32]>>>,
}

impl InputChannel {
    /// Create an in-process loopback input channel.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Send a 32-byte input packet.
    pub async fn send(&self, packet: &[u8; 32]) -> Result<(), RtcError> {
        self.tx.send(*packet).map_err(|_| RtcError::NotConnected)?;
        Ok(())
    }

    /// Receive the next input packet. Used by the host-side input pump.
    pub async fn recv(&self) -> Option<[u8; 32]> {
        self.rx.lock().await.recv().await
    }
}

impl Default for InputChannel {
    fn default() -> Self {
        Self::new()
    }
}

// ── Control channel ──────────────────────────────────────────────────────────

/// Reliable, ordered DataChannel for control messages.
pub struct ControlChannel {
    tx: mpsc::UnboundedSender<ControlMessage>,
    rx: Arc<Mutex<mpsc::UnboundedReceiver<ControlMessage>>>,
}

impl ControlChannel {
    /// Create an in-process loopback control channel.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Send a control message.
    pub async fn send(&self, msg: ControlMessage) -> Result<(), RtcError> {
        self.tx.send(msg).map_err(|_| RtcError::NotConnected)?;
        Ok(())
    }

    /// Receive the next control message.
    pub async fn recv(&self) -> Option<ControlMessage> {
        self.rx.lock().await.recv().await
    }

    /// Serialize `msg` to JSON and send as raw bytes. Used by the real
    /// webrtc-rs `RTCDataChannel::send(&Bytes)` call path.
    pub fn send_raw_bytes(msg: &ControlMessage) -> Result<Bytes, RtcError> {
        serde_json::to_vec(msg)
            .map(Bytes::from)
            .map_err(|e| {
                warn!("control message serialization error: {e}");
                RtcError::NotConnected
            })
    }

    /// Deserialize a raw-bytes control message.
    pub fn parse_raw_bytes(data: &[u8]) -> Option<ControlMessage> {
        serde_json::from_slice(data).ok()
    }
}

impl Default for ControlChannel {
    fn default() -> Self {
        Self::new()
    }
}

// ── Loopback wiring helper ───────────────────────────────────────────────────

/// Connect two `ControlChannel` instances so messages sent on one are received
/// on the other. Used by `smoke_loopback` to wire peer A and peer B together.
pub fn wire_control_channels(a: &ControlChannel, b: &ControlChannel) {
    // In a full loopback scenario the mpsc sender of A goes to the rx of B and
    // vice versa. For the scaffold we just check that the type compiles;
    // the actual wiring is done in smoke_loopback by reading one and writing
    // to the other in a tokio task.
    let _ = (a, b); // intentional no-op for the scaffold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn control_round_trip() {
        let ch = ControlChannel::new();
        let msg = ControlMessage::Heartbeat { seq: 42, sent_us: 12345 };
        ch.send(msg.clone()).await.unwrap();
        let received = ch.recv().await.unwrap();
        assert_eq!(received, msg);
    }

    #[tokio::test]
    async fn input_round_trip() {
        let ch = InputChannel::new();
        let pkt = [0xABu8; 32];
        ch.send(&pkt).await.unwrap();
        let received = ch.recv().await.unwrap();
        assert_eq!(received, pkt);
    }

    #[test]
    fn control_message_json_round_trip() {
        let msg = ControlMessage::HeartbeatAck { ack_seq: 1, recv_us: 100, send_us: 200 };
        let bytes = ControlChannel::send_raw_bytes(&msg).unwrap();
        let parsed = ControlChannel::parse_raw_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }
}
