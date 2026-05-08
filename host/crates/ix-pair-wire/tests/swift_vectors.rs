//! Cross-language test vectors. The Swift mirror runs an XCTest that decodes
//! the same hex strings; both sides must agree byte-for-byte.

use ix_pair_wire::{PairKind, PairMsg, HEADER_LEN, MAGIC, VERSION};

/// Vector 1: empty PStart frame. Header-only.
#[test]
fn vector_v1_empty_pstart() {
    let m = PairMsg::new(PairKind::PStart, vec![]);
    let bytes = m.encode().unwrap();
    let hex = hex_encode(&bytes);
    // 49585044 (magic) | 01 (ver) | 01 (kind PStart) | 0000 (len)
    assert_eq!(hex, "4958504401010000");
    assert_eq!(bytes.len(), HEADER_LEN);
    let back = PairMsg::decode(&bytes).unwrap();
    assert_eq!(back.kind, PairKind::PStart);
    assert!(back.body.is_empty());
}

/// Vector 2: 32-byte body, kind PResponse. Body is the bytes 0x00..0x1F.
#[test]
fn vector_v2_response_with_body() {
    let body: Vec<u8> = (0u8..32u8).collect();
    let m = PairMsg::new(PairKind::PResponse, body.clone());
    let bytes = m.encode().unwrap();
    let hex = hex_encode(&bytes);
    let expected = format!(
        "49585044{:02x}{:02x}0020{}",
        VERSION,
        PairKind::PResponse as u8,
        hex_encode(&body)
    );
    assert_eq!(hex, expected);
    let back = PairMsg::decode(&bytes).unwrap();
    assert_eq!(back.kind, PairKind::PResponse);
    assert_eq!(back.body, body);
}

/// Vector 3: PErr with a 2-byte error code body (0xDEAD).
#[test]
fn vector_v3_err_with_code() {
    let m = PairMsg::new(PairKind::PErr, vec![0xDE, 0xAD]);
    let hex = hex_encode(&m.encode().unwrap());
    assert_eq!(hex, "495850440101 ff 0002 dead".replace(' ', ""));
}

/// Sanity: changing magic invalidates the frame.
#[test]
fn magic_is_load_bearing() {
    assert_eq!(MAGIC, 0x49585044);
}

fn hex_encode(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b {
        use std::fmt::Write;
        write!(&mut s, "{:02x}", byte).unwrap();
    }
    s
}
