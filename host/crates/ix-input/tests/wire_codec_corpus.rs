// wire_codec_corpus.rs — JSON test vector corpus for the 32-byte wire format.
//
// These tests verify two properties:
//   1. The Rust encoder+decoder round-trips each vector byte-for-byte.
//   2. The expected_bytes fields match the actual encoder output.
//
// The same JSON file is consumed by the Swift PacketEncoderTests so that both
// sides are guaranteed to be bit-exact with each other.
//
// Run: `cargo test --test wire_codec_corpus`

use ix_input::wire::{Kind, KeyPayload, Packet, PencilPayload, TouchPayload, PACKET_LEN};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex"))
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── Pencil round-trip tests ───────────────────────────────────────────────────

#[test]
fn pencil_move_round_trip() {
    let pl = PencilPayload {
        x: 768.5,
        y: 1024.25,
        pressure: 0.3,
        tilt: 0.4,
        azimuth: 0.5,
        twist: 0.0,
        barrel: false,
        hover: false,
    };
    let p = Packet {
        kind: Kind::PencilMove,
        time_us: 1_700_000_000_000_000,
        seq: 42,
        flags: 0b01,
        payload: pl.into_bytes(),
    };
    let bytes = p.to_bytes();
    assert_eq!(bytes.len(), PACKET_LEN);
    let back = Packet::from_bytes(&bytes).unwrap();
    assert_eq!(back.kind, Kind::PencilMove);
    assert_eq!(back.seq, 42);
    assert_eq!(back.flags, 0b01);
    assert_eq!(back.time_us, 1_700_000_000_000_000);
    let pl2 = PencilPayload::from_bytes(&back.payload);
    assert!(
        (pl2.x - 768.5).abs() < 1.0 / 65_536.0,
        "x mismatch: got {}, want 768.5",
        pl2.x
    );
    assert!(
        (pl2.y - 1024.25).abs() < 1.0 / 65_536.0,
        "y mismatch: got {}, want 1024.25",
        pl2.y
    );
    assert!(
        (pl2.pressure - 0.3).abs() < 1.0 / 32_768.0,
        "pressure mismatch: got {}",
        pl2.pressure
    );
}

#[test]
fn pencil_barrel_hover_flags() {
    let pl = PencilPayload {
        x: 0.0, y: 0.0,
        pressure: 0.0, tilt: 0.0, azimuth: 0.0, twist: 0.0,
        barrel: true, hover: true,
    };
    let p = Packet {
        kind: Kind::PencilBegin,
        time_us: 0,
        seq: 1,
        flags: 0,
        payload: pl.into_bytes(),
    };
    let back = Packet::from_bytes(&p.to_bytes()).unwrap();
    let pl2 = PencilPayload::from_bytes(&back.payload);
    assert!(pl2.barrel, "barrel should survive");
    assert!(pl2.hover, "hover should survive");
}

#[test]
fn pencil_all_zero_payload() {
    let p = Packet {
        kind: Kind::PencilEnd,
        time_us: 0,
        seq: 0,
        flags: 0,
        payload: [0u8; 18],
    };
    let bytes = p.to_bytes();
    let back = Packet::from_bytes(&bytes).unwrap();
    assert_eq!(back.kind, Kind::PencilEnd);
    assert_eq!(back.payload, [0u8; 18]);
}

// ── Touch round-trip tests ────────────────────────────────────────────────────

#[test]
fn touch_begin_round_trip() {
    let pl = TouchPayload {
        x: 512.0,
        y: 768.0,
        radius_major: 8.0,
        radius_minor: 6.0,
        force: 0.4,
    };
    let p = Packet {
        kind: Kind::TouchBegin,
        time_us: 999,
        seq: 3,
        flags: 0,
        payload: pl.into_bytes(),
    };
    let bytes = p.to_bytes();
    let back = Packet::from_bytes(&bytes).unwrap();
    assert_eq!(back.kind, Kind::TouchBegin);
    let pl2 = TouchPayload::from_bytes(&back.payload);
    assert!((pl2.x - 512.0).abs() < 1.0 / 65_536.0);
    assert!((pl2.y - 768.0).abs() < 1.0 / 65_536.0);
}

#[test]
fn touch_reserved_bytes_are_zero() {
    let pl = TouchPayload {
        x: 100.0, y: 200.0,
        radius_major: 5.0, radius_minor: 4.0, force: 0.1,
    };
    let b = pl.into_bytes();
    // Bytes 14-17 are reserved and must be zero.
    assert_eq!(&b[14..18], &[0u8; 4], "reserved bytes should be zero");
}

// ── Key round-trip tests ──────────────────────────────────────────────────────

#[test]
fn key_down_a_round_trip() {
    let pl = KeyPayload { usage_page: 7, usage: 4, modifiers: 2 };
    let p = Packet {
        kind: Kind::KeyDown,
        time_us: 0,
        seq: 1,
        flags: 0,
        payload: pl.into_bytes(),
    };
    let bytes = p.to_bytes();
    let back = Packet::from_bytes(&bytes).unwrap();
    assert_eq!(back.kind, Kind::KeyDown);
    let pl2 = KeyPayload::from_bytes(&back.payload);
    assert_eq!(pl2.usage_page, 7);
    assert_eq!(pl2.usage, 4);
    assert_eq!(pl2.modifiers, 2);
}

#[test]
fn key_reserved_bytes_are_zero() {
    let pl = KeyPayload { usage_page: 1, usage: 2, modifiers: 0 };
    let b = pl.into_bytes();
    assert_eq!(&b[6..18], &[0u8; 12], "reserved bytes 6-17 should be zero");
}

#[test]
fn modifier_kind_parses() {
    let pl = KeyPayload { usage_page: 0xFF, usage: 0x01, modifiers: 0x08 };
    let p = Packet {
        kind: Kind::Modifier,
        time_us: 0, seq: 0, flags: 0,
        payload: pl.into_bytes(),
    };
    let back = Packet::from_bytes(&p.to_bytes()).unwrap();
    assert_eq!(back.kind, Kind::Modifier);
}

// ── Decode error tests ────────────────────────────────────────────────────────

#[test]
fn rejects_short_buffer() {
    let err = Packet::from_bytes(&[0u8; 31]).unwrap_err();
    assert_eq!(err, ix_input::wire::DecodeError::ShortBuffer);
}

#[test]
fn rejects_unknown_kind() {
    let mut bytes = [0u8; 32];
    bytes[0] = 0xFF;
    assert!(matches!(
        Packet::from_bytes(&bytes).unwrap_err(),
        ix_input::wire::DecodeError::UnknownKind(0xFF)
    ));
}

#[test]
fn rejects_unknown_kind_zero() {
    let bytes = [0u8; 32]; // kind 0x00 is unassigned
    assert!(Packet::from_bytes(&bytes).is_err());
}

#[test]
fn rejects_reserved_flag_bits() {
    let mut bytes = [0u8; 32];
    bytes[0] = 0x11; // PencilMove
    bytes[13] = 0b0000_1000; // reserved bit 3
    let err = Packet::from_bytes(&bytes).unwrap_err();
    assert_eq!(err, ix_input::wire::DecodeError::ReservedBitsSet);
}

// ── JSON corpus round-trip ────────────────────────────────────────────────────
//
// Reads tests/wire_vectors.json and verifies each entry's `expected_bytes`
// round-trips through the Rust codec without modification.
//
// This is the canonical cross-language compatibility test: the same JSON file
// is loaded by Swift PacketEncoderTests.

#[test]
fn shared_vector_corpus_round_trips() {
    // Path is relative to the crate root (where `cargo test` runs).
    let json_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("wire_vectors.json");

    let raw = std::fs::read_to_string(&json_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", json_path.display()));

    let entries: serde_json::Value = serde_json::from_str(&raw)
        .expect("wire_vectors.json must be valid JSON");

    let arr = entries.as_array().expect("top-level must be an array");
    assert!(!arr.is_empty(), "corpus must contain at least one entry");

    for entry in arr {
        let name = entry["name"].as_str().unwrap_or("<unnamed>");
        let bytes_hex = entry["expected_bytes"]
            .as_str()
            .unwrap_or_else(|| panic!("entry {name}: missing expected_bytes"));

        let bytes = hex_decode(bytes_hex);
        assert_eq!(
            bytes.len(),
            PACKET_LEN,
            "entry {name}: expected_bytes length must be {PACKET_LEN}"
        );

        let p = Packet::from_bytes(&bytes)
            .unwrap_or_else(|e| panic!("entry {name}: decode failed: {e}"));

        let re_encoded = p.to_bytes();
        let re_hex = hex_encode(&re_encoded);

        assert_eq!(
            re_hex, bytes_hex,
            "entry {name}: re-encoded bytes differ\n  want: {bytes_hex}\n   got: {re_hex}"
        );
    }
}

// ── Endianness smoke test ─────────────────────────────────────────────────────

#[test]
fn little_endian_time_us() {
    // time_us = 1 should appear as 01 00 00 00 00 00 00 00 at bytes 1-8.
    let p = Packet {
        kind: Kind::TouchMove,
        time_us: 1,
        seq: 0,
        flags: 0,
        payload: [0; 18],
    };
    let bytes = p.to_bytes();
    assert_eq!(&bytes[1..9], &[1, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn little_endian_seq() {
    // seq = 0x01020304 → 04 03 02 01 at bytes 9-12.
    let p = Packet {
        kind: Kind::TouchMove,
        time_us: 0,
        seq: 0x01020304,
        flags: 0,
        payload: [0; 18],
    };
    let bytes = p.to_bytes();
    assert_eq!(&bytes[9..13], &[0x04, 0x03, 0x02, 0x01]);
}

// ── All kinds exercise ────────────────────────────────────────────────────────

#[test]
fn all_nine_kinds_round_trip() {
    let kinds = [
        Kind::TouchBegin,
        Kind::TouchMove,
        Kind::TouchEnd,
        Kind::PencilBegin,
        Kind::PencilMove,
        Kind::PencilEnd,
        Kind::KeyDown,
        Kind::KeyUp,
        Kind::Modifier,
    ];
    for kind in kinds {
        let p = Packet { kind, time_us: 0, seq: 0, flags: 0, payload: [0; 18] };
        let back = Packet::from_bytes(&p.to_bytes())
            .unwrap_or_else(|e| panic!("kind {:?} round-trip failed: {e}", kind));
        assert_eq!(back.kind, kind);
    }
}
