//! Heartbeat state machine — 250 ms cadence, 4 missed = 1 s disconnect.
//!
//! ## Spec reference: §5.3 and §9
//! - Host fires `ControlMessage::Heartbeat { seq, sent_us }` every 250 ms.
//! - iPad replies with `ControlMessage::HeartbeatAck { ack_seq, recv_us, send_us }`.
//! - If 4 consecutive beats are not acked within the next 4 ticks, the 5th tick
//!   signals `HeartbeatEvent::Disconnected`. 4 × 250 ms = 1 s total silence.
//! - A single late ack (arrived after 1 or 2 missed beats) resets the counter.
//!
//! ## State machine
//! The heartbeat is driven by a `tick(now)` method. Each call may return a
//! `HeartbeatEvent`. Acks arrive via `on_ack(seq, recv_at)`.
//!
//! ## Design note
//! The missed-beat count is the number of currently pending (unacked) beats
//! that were sent in a previous tick. When that count reaches `MAX_MISSED` at
//! the start of a tick, the peer is considered disconnected.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Events emitted by [`Heartbeat::tick`].
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatEvent {
    /// Time to send a new heartbeat; embed the returned `seq` in the message.
    Send {
        /// Monotonic sequence number to put in the `Heartbeat` control message.
        seq: u32,
    },
    /// 4 consecutive beats missed — declare the peer disconnected.
    Disconnected,
}

impl HeartbeatEvent {
    /// Convenience: extract the seq if this is a `Send` event.
    pub fn seq(self) -> Option<u32> {
        if let HeartbeatEvent::Send { seq } = self {
            Some(seq)
        } else {
            None
        }
    }

    /// Panic with a useful message if this is not a `Send` event.
    /// Used in tests: `let seq = hb.tick(...).expect_send()`.
    pub fn expect_send(self) -> u32 {
        match self {
            HeartbeatEvent::Send { seq } => seq,
            other => panic!("expected HeartbeatEvent::Send, got {other:?}"),
        }
    }
}

/// Heartbeat state machine.
pub struct Heartbeat {
    /// Timer interval.
    interval: Duration,
    /// Monotonic sequence counter.
    next_seq: u32,
    /// Sent-but-not-yet-acked beats: seq → sent_at.
    pending: HashMap<u32, Instant>,
    /// Time of last tick.
    last_tick: Option<Instant>,
}

/// Maximum number of concurrent unacked beats before `Disconnected`.
const MAX_MISSED: usize = 4;

impl Heartbeat {
    /// Create a new heartbeat with the given interval.
    ///
    /// Call [`tick`][Self::tick] on every timer expiry and
    /// [`on_ack`][Self::on_ack] whenever a `HeartbeatAck` arrives.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            next_seq: 0,
            pending: HashMap::new(),
            last_tick: None,
        }
    }

    /// Process a timer tick at `now`.
    ///
    /// Returns:
    /// - `Some(HeartbeatEvent::Send { seq })` — send a heartbeat with this seq.
    /// - `Some(HeartbeatEvent::Disconnected)` — 4 beats unanswered; declare disconnect.
    /// - `None` — called too early (before the interval elapsed).
    pub fn tick(&mut self, now: Instant) -> Option<HeartbeatEvent> {
        if let Some(last) = self.last_tick {
            if now < last + self.interval {
                return None;
            }
        }
        self.last_tick = Some(now);

        // If we have MAX_MISSED unacked beats, declare disconnect.
        if self.pending.len() >= MAX_MISSED {
            return Some(HeartbeatEvent::Disconnected);
        }

        // Otherwise, send a new beat.
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        self.pending.insert(seq, now);
        Some(HeartbeatEvent::Send { seq })
    }

    /// Process an incoming ack.
    ///
    /// Removes the acked beat and all older beats from the pending set,
    /// treating the ack as cumulative up to `ack_seq`.
    pub fn on_ack(&mut self, ack_seq: u32, _recv_at: Instant) {
        self.pending.retain(|&seq, _| seq > ack_seq);
    }

    /// Number of currently pending (unacked) beats.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_missed_acks_signals_disconnect() {
        let interval = Duration::from_millis(250);
        let mut hb = Heartbeat::new(interval);
        let t0 = Instant::now();

        // Tick 4 times without acking — pending accumulates to 4.
        for i in 1..=4u32 {
            let evt = hb.tick(t0 + interval * i);
            assert!(
                matches!(evt, Some(HeartbeatEvent::Send { .. })),
                "tick {i} should produce Send while <4 pending"
            );
        }

        // 5th tick: pending.len() == 4 → Disconnected.
        let evt = hb.tick(t0 + interval * 5);
        assert_eq!(
            evt,
            Some(HeartbeatEvent::Disconnected),
            "5th tick with 4 unacked beats should signal Disconnected"
        );
    }

    #[test]
    fn one_late_ack_is_recoverable() {
        let interval = Duration::from_millis(250);
        let mut hb = Heartbeat::new(interval);
        let t0 = Instant::now();

        // First tick: send seq=0.
        let seq = hb.tick(t0 + interval).unwrap().expect_send();

        // Two more ticks without acking.
        let _ = hb.tick(t0 + interval * 2);
        let _ = hb.tick(t0 + interval * 3);

        // Late ack for seq=0 arrives — clears it from pending.
        hb.on_ack(seq, t0 + interval * 3 + Duration::from_millis(50));
        assert!(hb.pending_count() < MAX_MISSED, "ack should reduce pending count");

        // Next tick should NOT be Disconnected.
        let evt = hb.tick(t0 + interval * 4);
        assert_ne!(
            evt,
            Some(HeartbeatEvent::Disconnected),
            "single late ack should prevent disconnect"
        );
    }

    #[test]
    fn sends_increase_sequence() {
        let interval = Duration::from_millis(250);
        let mut hb = Heartbeat::new(interval);
        let t0 = Instant::now();

        let s0 = hb.tick(t0 + interval).unwrap().seq().unwrap();
        hb.on_ack(s0, t0 + interval + Duration::from_millis(5));
        let s1 = hb.tick(t0 + interval * 2).unwrap().seq().unwrap();
        assert_eq!(s1, s0 + 1, "sequence must be monotonically increasing");
    }

    #[test]
    fn early_tick_returns_none() {
        let interval = Duration::from_millis(250);
        let mut hb = Heartbeat::new(interval);
        let t0 = Instant::now();

        let _evt = hb.tick(t0 + interval);
        // Call again immediately (< interval elapsed) — should return None.
        let evt = hb.tick(t0 + interval + Duration::from_millis(10));
        assert_eq!(evt, None, "tick called too early should return None");
    }
}
