//! Tests for the runtime encoder probe and M4-iPad AV1 gating logic.

use ix_codec::probe::Probe;
use ix_codec::{EncoderKind, PeerCaps, PeerKind};

fn m4_peer() -> PeerCaps {
    PeerCaps {
        av1_decode: true,
        hevc_decode: true,
        max_resolution: (3840, 2160),
        peer_kind: PeerKind::IpadProM4,
    }
}

fn m2_peer() -> PeerCaps {
    PeerCaps {
        av1_decode: false,
        hevc_decode: true,
        max_resolution: (2732, 2048),
        peer_kind: PeerKind::IpadProM2OrM1,
    }
}

#[test]
fn priority_order_is_stable_across_calls() {
    let o1 = Probe::synthetic_for_test(&[
        EncoderKind::NvencHevc,
        EncoderKind::QsvHevc,
        EncoderKind::X264SoftwareUlllSw,
    ]);
    let o2 = Probe::synthetic_for_test(&[
        EncoderKind::NvencHevc,
        EncoderKind::QsvHevc,
        EncoderKind::X264SoftwareUlllSw,
    ]);

    let a: Vec<_> = o1.iter().copied().collect();
    let b: Vec<_> = o2.iter().copied().collect();
    assert_eq!(a, b, "probe outcome must be deterministic");
}

#[test]
fn av1_candidate_only_offered_to_m4_peers() {
    let outcome = Probe::synthetic_for_test(&[
        EncoderKind::NvencAv1,
        EncoderKind::NvencHevc,
    ]);

    let for_m4 = outcome.candidates_for(&m4_peer());
    let for_m2 = outcome.candidates_for(&m2_peer());

    assert_eq!(
        for_m4.first(),
        Some(&EncoderKind::NvencAv1),
        "NvencAv1 should be first for M4 peer"
    );
    assert!(
        for_m2.iter().all(|k| !matches!(k, EncoderKind::NvencAv1)),
        "NvencAv1 should not be offered to M2 peer"
    );
}

#[test]
fn software_fallback_emits_warning_flag() {
    let outcome = Probe::synthetic_for_test(&[EncoderKind::X264SoftwareUlllSw]);
    assert!(
        outcome.software_fallback_only(),
        "probe with only software encoder must set software_fallback_only()"
    );
}

#[test]
fn hardware_plus_software_is_not_sw_only() {
    let outcome = Probe::synthetic_for_test(&[
        EncoderKind::NvencHevc,
        EncoderKind::X264SoftwareUlllSw,
    ]);
    assert!(!outcome.software_fallback_only());
}

#[test]
fn candidates_for_hevc_only_peer_excludes_av1() {
    let outcome = Probe::synthetic_for_test(&[
        EncoderKind::NvencAv1,
        EncoderKind::NvencHevc,
        EncoderKind::X264SoftwareUlllSw,
    ]);
    // A peer that reports hevc_decode=true but av1_decode=false.
    let peer = PeerCaps {
        av1_decode: false,
        hevc_decode: true,
        max_resolution: (1920, 1080),
        peer_kind: PeerKind::IpadAirM,
    };
    let candidates = outcome.candidates_for(&peer);
    assert!(
        candidates.iter().all(|k| !matches!(k, EncoderKind::NvencAv1)),
        "AV1 must not appear when peer declares no AV1 decode capability"
    );
}

#[test]
fn empty_probe_returns_empty_candidates() {
    let outcome = Probe::synthetic_for_test(&[]);
    assert!(outcome.candidates_for(&m4_peer()).is_empty());
    assert!(!outcome.software_fallback_only(), "empty list is not sw-only");
}

#[test]
fn candidates_sorted_by_priority() {
    let outcome = Probe::synthetic_for_test(&[
        EncoderKind::X264SoftwareUlllSw, // prio 99
        EncoderKind::NvencHevc,           // prio 1
        EncoderKind::VaapiHevc,           // prio 2
    ]);
    let candidates = outcome.candidates_for(&m2_peer());
    let priorities: Vec<u8> = candidates.iter().map(|k| k.priority()).collect();
    let mut sorted = priorities.clone();
    sorted.sort();
    assert_eq!(priorities, sorted, "candidates must be sorted ascending by priority");
}
