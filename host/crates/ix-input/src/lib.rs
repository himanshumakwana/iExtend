//! `ix-input` — virtual stylus / touch / keyboard injection.
//!
//! # Architecture
//!
//! ```text
//! iPad ──[DataChannel]──► Dispatcher ──► Injector (trait)
//!                                              │
//!                                  ┌───────────┴───────────┐
//!                            LinuxInjector         WindowsInjector
//!                            (/dev/uinput)          (vhf.sys IOCTL)
//! ```
//!
//! The [`Dispatcher`] parses 32-byte packets (see [`wire`]), applies
//! sequence-number–based dedup (the `input` DataChannel is unreliable /
//! unordered), and forwards to a [`Injector`] implementation.

pub mod q16;
pub mod wire;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(windows)]
pub mod windows;

pub mod cursor_protocol;

// ── Injector trait ────────────────────────────────────────────────────────────

/// OS-specific input injector.  Implementations: [`linux::LinuxInjector`],
/// [`windows::WindowsInjector`].
pub trait Injector: Send + Sync {
    /// Inject a single decoded packet into the OS input subsystem.
    fn inject(&self, p: &wire::Packet);
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Parses raw 32-byte buffers received from the iPad `input` DataChannel and
/// forwards decoded packets to an [`Injector`].
///
/// Sequence-number dedup: on an unreliable/unordered channel packets may
/// arrive out of order or be duplicated.  The dispatcher accepts a packet only
/// if its `seq` is strictly greater than the last accepted seq for the same
/// `Kind`.  `seq == 0` is always accepted (first packet or channel reset).
pub struct Dispatcher<I: Injector> {
    injector: std::sync::Arc<I>,
    /// Last accepted seq per kind byte (0x01..=0x22).
    last_seq: std::collections::HashMap<u8, u32>,
}

impl<I: Injector> Dispatcher<I> {
    /// Create a new dispatcher backed by `injector`.
    pub fn new(injector: std::sync::Arc<I>) -> Self {
        Self { injector, last_seq: Default::default() }
    }

    /// Handle a raw byte slice from the DataChannel.
    ///
    /// Returns `Ok(true)` if the packet was forwarded, `Ok(false)` if it was
    /// dropped as a duplicate / out-of-order, or an error if decoding failed.
    pub fn handle(&mut self, bytes: &[u8]) -> Result<bool, wire::DecodeError> {
        let p = wire::Packet::from_bytes(bytes)?;
        let key = p.kind as u8;
        let last = self.last_seq.get(&key).copied().unwrap_or(0);
        // Accept if seq advances, or if this is the very first packet (seq 0).
        if p.seq > last || p.seq == 0 {
            self.last_seq.insert(key, p.seq);
            self.injector.inject(&p);
            Ok(true)
        } else {
            tracing::trace!(kind = key, seq = p.seq, last, "dropping out-of-order packet");
            Ok(false)
        }
    }
}

// ── No-op injector for tests and unsupported platforms ────────────────────────

/// A no-op injector that discards all packets.  Used in unit tests and on
/// platforms where a real injector is not yet implemented.
pub struct NoopInjector;

impl Injector for NoopInjector {
    fn inject(&self, _p: &wire::Packet) {}
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Records every injected packet for test assertions.
    struct Recorder(Mutex<Vec<wire::Packet>>);

    impl Injector for Recorder {
        fn inject(&self, p: &wire::Packet) {
            self.0.lock().unwrap().push(p.clone());
        }
    }

    fn pencil_move_bytes(seq: u32) -> [u8; wire::PACKET_LEN] {
        wire::Packet {
            kind: wire::Kind::PencilMove,
            time_us: 1_000_000,
            seq,
            flags: 0,
            payload: wire::PencilPayload {
                x: 100.0, y: 200.0, pressure: 0.5,
                tilt: 0.3, azimuth: 1.0, twist: 0.0,
                barrel: false, hover: false,
            }.into_bytes(),
        }.to_bytes()
    }

    #[test]
    fn dispatch_routes_to_injector() {
        let r = Arc::new(Recorder(Mutex::new(vec![])));
        let mut d = Dispatcher::new(r.clone());
        let accepted = d.handle(&pencil_move_bytes(1)).unwrap();
        assert!(accepted, "first packet should be accepted");
        assert_eq!(r.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn drops_out_of_order() {
        let r = Arc::new(Recorder(Mutex::new(vec![])));
        let mut d = Dispatcher::new(r.clone());
        // seq 5 first, then seq 3 — out of order
        d.handle(&pencil_move_bytes(5)).unwrap();
        let accepted = d.handle(&pencil_move_bytes(3)).unwrap();
        assert!(!accepted, "out-of-order packet should be dropped");
        assert_eq!(r.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn accepts_seq_zero_as_reset() {
        let r = Arc::new(Recorder(Mutex::new(vec![])));
        let mut d = Dispatcher::new(r.clone());
        d.handle(&pencil_move_bytes(10)).unwrap();
        // seq 0 is treated as a channel reset / always accepted
        let accepted = d.handle(&pencil_move_bytes(0)).unwrap();
        assert!(accepted, "seq 0 should always be accepted");
        assert_eq!(r.0.lock().unwrap().len(), 2);
    }

    #[test]
    fn different_kinds_track_independently() {
        let r = Arc::new(Recorder(Mutex::new(vec![])));
        let mut d = Dispatcher::new(r.clone());
        // Send touch seq 5 then pencil seq 3 — different kinds, both valid
        let touch = wire::Packet {
            kind: wire::Kind::TouchMove, time_us: 0, seq: 5, flags: 0,
            payload: wire::TouchPayload { x: 0.0, y: 0.0, radius_major: 0.0, radius_minor: 0.0, force: 0.0 }.into_bytes(),
        }.to_bytes();
        d.handle(&touch).unwrap();
        d.handle(&pencil_move_bytes(3)).unwrap();
        assert_eq!(r.0.lock().unwrap().len(), 2, "different kinds accepted independently");
    }

    #[test]
    fn propagates_decode_error() {
        let r = Arc::new(Recorder(Mutex::new(vec![])));
        let mut d = Dispatcher::new(r.clone());
        let err = d.handle(&[0u8; 10]).unwrap_err();
        assert_eq!(err, wire::DecodeError::ShortBuffer);
    }

    #[test]
    fn noop_injector_compiles() {
        let inj = Arc::new(NoopInjector);
        let mut d = Dispatcher::new(inj);
        d.handle(&pencil_move_bytes(1)).unwrap();
    }
}
