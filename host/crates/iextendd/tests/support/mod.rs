//! Test-support code shared between `iextendd` integration tests.
//!
//! - [`timecode_source`] — in-process 7-segment timecode painter / reader used
//!   by the synthetic latency test to timestamp frames without a display.
//! - [`loopback_peer`] — two in-process [`ix_rtc::Peer`] instances wired
//!   together via an `mpsc` channel; used to drive the encode → decode pipeline
//!   without ICE / DTLS / network.

pub mod loopback_peer;
pub mod timecode_source;
