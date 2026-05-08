// wire.rs — 32-byte fixed-size packet codec for the input DataChannel.
//
// Spec §6.1 layout:
//   Offset  Size  Field
//   0       1     kind      (enum, see Kind)
//   1       8     time_us   (u64 LE, iPad mach_absolute_time in µs)
//   9       4     seq       (u32 LE, monotonic per-channel)
//   13      1     flags     (bit 0 = predicted, bit 1 = coalesced; bits 2-7 reserved/zero)
//   14      18    payload   (kind-specific; see PencilPayload, TouchPayload, KeyPayload)
//
// All integers are little-endian (WebRTC binary message convention).

use crate::q16::{q16_from_f32, q16_i16_from_f32, q16_i16_to_f32, q16_to_f32};

/// Packet kind discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    TouchBegin = 0x01,
    TouchMove = 0x02,
    TouchEnd = 0x03,
    PencilBegin = 0x10,
    PencilMove = 0x11,
    PencilEnd = 0x12,
    KeyDown = 0x20,
    KeyUp = 0x21,
    Modifier = 0x22,
}

impl Kind {
    /// Parse a raw byte into a Kind. Returns None for unknown values.
    pub fn from_u8(v: u8) -> Option<Kind> {
        match v {
            0x01 => Some(Kind::TouchBegin),
            0x02 => Some(Kind::TouchMove),
            0x03 => Some(Kind::TouchEnd),
            0x10 => Some(Kind::PencilBegin),
            0x11 => Some(Kind::PencilMove),
            0x12 => Some(Kind::PencilEnd),
            0x20 => Some(Kind::KeyDown),
            0x21 => Some(Kind::KeyUp),
            0x22 => Some(Kind::Modifier),
            _ => None,
        }
    }
}

/// Total on-wire size of every packet — always exactly 32 bytes.
pub const PACKET_LEN: usize = 32;

/// The 32-byte wire packet.
#[derive(Debug, Clone)]
pub struct Packet {
    pub kind: Kind,
    pub time_us: u64,
    pub seq: u32,
    /// Flags: bit 0 = predicted, bit 1 = coalesced. Bits 2-7 must be zero.
    pub flags: u8,
    /// 18 raw payload bytes; interpret with PencilPayload / TouchPayload / KeyPayload.
    pub payload: [u8; 18],
}

/// Decode errors.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("buffer shorter than 32 bytes")]
    ShortBuffer,
    #[error("unknown kind byte {0:#04x}")]
    UnknownKind(u8),
    #[error("reserved flag bits are set")]
    ReservedBitsSet,
}

impl Packet {
    /// Serialise to a 32-byte array.
    pub fn to_bytes(&self) -> [u8; PACKET_LEN] {
        let mut out = [0u8; PACKET_LEN];
        out[0] = self.kind as u8;
        out[1..9].copy_from_slice(&self.time_us.to_le_bytes());
        out[9..13].copy_from_slice(&self.seq.to_le_bytes());
        out[13] = self.flags;
        out[14..32].copy_from_slice(&self.payload);
        out
    }

    /// Parse a 32-byte slice.  Fails if the buffer is short, the kind is
    /// unrecognised, or any reserved flag bits are non-zero.
    pub fn from_bytes(b: &[u8]) -> Result<Self, DecodeError> {
        if b.len() < PACKET_LEN {
            return Err(DecodeError::ShortBuffer);
        }
        let kind = Kind::from_u8(b[0]).ok_or(DecodeError::UnknownKind(b[0]))?;
        if b[13] & 0b1111_1100 != 0 {
            return Err(DecodeError::ReservedBitsSet);
        }
        let time_us = u64::from_le_bytes(b[1..9].try_into().unwrap());
        let seq = u32::from_le_bytes(b[9..13].try_into().unwrap());
        let flags = b[13];
        let mut payload = [0u8; 18];
        payload.copy_from_slice(&b[14..32]);
        Ok(Self {
            kind,
            time_us,
            seq,
            flags,
            payload,
        })
    }
}

// ── Pencil payload ────────────────────────────────────────────────────────────
//
// PENCIL_* payload (18 bytes):
//   0-3   x_q16         i32 LE   iPad pixels
//   4-7   y_q16         i32 LE   iPad pixels
//   8-9   pressure_q16  i16 LE   0..=1.0  (saturates above 0.5)
//  10-11  tilt_q16      i16 LE   altitude angle 0..=π/2 (saturates)
//  12-13  azimuth_q16   i16 LE   0..=2π (saturates at i16::MAX for large values)
//  14-15  twist_q16     i16 LE   Pencil Pro only, else 0
//  16     buttons       bit 0 = barrel
//  17     hover         1 = hover

#[derive(Debug, Clone, Copy)]
pub struct PencilPayload {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt: f32,
    pub azimuth: f32,
    pub twist: f32,
    pub barrel: bool,
    pub hover: bool,
}

impl PencilPayload {
    pub fn into_bytes(self) -> [u8; 18] {
        let mut b = [0u8; 18];
        b[0..4].copy_from_slice(&q16_from_f32(self.x).to_le_bytes());
        b[4..8].copy_from_slice(&q16_from_f32(self.y).to_le_bytes());
        b[8..10].copy_from_slice(&q16_i16_from_f32(self.pressure).to_le_bytes());
        b[10..12].copy_from_slice(&q16_i16_from_f32(self.tilt).to_le_bytes());
        b[12..14].copy_from_slice(&q16_i16_from_f32(self.azimuth).to_le_bytes());
        b[14..16].copy_from_slice(&q16_i16_from_f32(self.twist).to_le_bytes());
        b[16] = self.barrel as u8;
        b[17] = self.hover as u8;
        b
    }

    pub fn from_bytes(b: &[u8; 18]) -> Self {
        Self {
            x: q16_to_f32(i32::from_le_bytes(b[0..4].try_into().unwrap())),
            y: q16_to_f32(i32::from_le_bytes(b[4..8].try_into().unwrap())),
            pressure: q16_i16_to_f32(i16::from_le_bytes(b[8..10].try_into().unwrap())),
            tilt: q16_i16_to_f32(i16::from_le_bytes(b[10..12].try_into().unwrap())),
            azimuth: q16_i16_to_f32(i16::from_le_bytes(b[12..14].try_into().unwrap())),
            twist: q16_i16_to_f32(i16::from_le_bytes(b[14..16].try_into().unwrap())),
            barrel: b[16] != 0,
            hover: b[17] != 0,
        }
    }
}

// ── Touch payload ─────────────────────────────────────────────────────────────
//
// TOUCH_* payload (18 bytes):
//   0-3   x_q16               i32 LE
//   4-7   y_q16               i32 LE
//   8-9   radius_major_q16    i16 LE
//  10-11  radius_minor_q16    i16 LE
//  12-13  force_q16           i16 LE
//  14-17  reserved (zero)

#[derive(Debug, Clone, Copy)]
pub struct TouchPayload {
    pub x: f32,
    pub y: f32,
    pub radius_major: f32,
    pub radius_minor: f32,
    pub force: f32,
}

impl TouchPayload {
    pub fn into_bytes(self) -> [u8; 18] {
        let mut b = [0u8; 18];
        b[0..4].copy_from_slice(&q16_from_f32(self.x).to_le_bytes());
        b[4..8].copy_from_slice(&q16_from_f32(self.y).to_le_bytes());
        b[8..10].copy_from_slice(&q16_i16_from_f32(self.radius_major).to_le_bytes());
        b[10..12].copy_from_slice(&q16_i16_from_f32(self.radius_minor).to_le_bytes());
        b[12..14].copy_from_slice(&q16_i16_from_f32(self.force).to_le_bytes());
        // b[14..18] remain zero (reserved)
        b
    }

    pub fn from_bytes(b: &[u8; 18]) -> Self {
        Self {
            x: q16_to_f32(i32::from_le_bytes(b[0..4].try_into().unwrap())),
            y: q16_to_f32(i32::from_le_bytes(b[4..8].try_into().unwrap())),
            radius_major: q16_i16_to_f32(i16::from_le_bytes(b[8..10].try_into().unwrap())),
            radius_minor: q16_i16_to_f32(i16::from_le_bytes(b[10..12].try_into().unwrap())),
            force: q16_i16_to_f32(i16::from_le_bytes(b[12..14].try_into().unwrap())),
        }
    }
}

// ── Key payload ───────────────────────────────────────────────────────────────
//
// KEY_* payload (18 bytes):
//   0-1   usage_page   u16 LE  HID usage page
//   2-3   usage        u16 LE  HID usage
//   4-5   modifiers    u16 LE  bitmask
//   6-17  reserved (zero)

#[derive(Debug, Clone, Copy)]
pub struct KeyPayload {
    pub usage_page: u16,
    pub usage: u16,
    pub modifiers: u16,
}

impl KeyPayload {
    pub fn into_bytes(self) -> [u8; 18] {
        let mut b = [0u8; 18];
        b[0..2].copy_from_slice(&self.usage_page.to_le_bytes());
        b[2..4].copy_from_slice(&self.usage.to_le_bytes());
        b[4..6].copy_from_slice(&self.modifiers.to_le_bytes());
        // b[6..18] remain zero (reserved)
        b
    }

    pub fn from_bytes(b: &[u8; 18]) -> Self {
        Self {
            usage_page: u16::from_le_bytes(b[0..2].try_into().unwrap()),
            usage: u16::from_le_bytes(b[2..4].try_into().unwrap()),
            modifiers: u16::from_le_bytes(b[4..6].try_into().unwrap()),
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    fn make_pencil_packet() -> Packet {
        let pl = PencilPayload {
            x: 768.5,
            y: 1024.25,
            pressure: 0.3,
            tilt: 0.4,
            azimuth: 0.5,
            twist: 0.0,
            barrel: true,
            hover: false,
        };
        Packet {
            kind: Kind::PencilMove,
            time_us: 1_700_000_000_000_000,
            seq: 7,
            flags: 0b01,
            payload: pl.into_bytes(),
        }
    }

    #[test]
    fn packet_len_is_32() {
        let p = make_pencil_packet();
        assert_eq!(p.to_bytes().len(), PACKET_LEN);
    }

    #[test]
    fn pencil_roundtrip() {
        let p = make_pencil_packet();
        let bytes = p.to_bytes();
        let back = Packet::from_bytes(&bytes).unwrap();
        assert_eq!(back.kind, Kind::PencilMove);
        assert_eq!(back.seq, 7);
        assert_eq!(back.flags, 0b01);
        assert_eq!(back.time_us, 1_700_000_000_000_000);
        let pl = PencilPayload::from_bytes(&back.payload);
        assert!(
            (pl.x - 768.5).abs() < 1.0 / 65_536.0,
            "x mismatch: {}",
            pl.x
        );
        assert!(
            (pl.y - 1024.25).abs() < 1.0 / 65_536.0,
            "y mismatch: {}",
            pl.y
        );
        assert!(pl.barrel, "barrel should be set");
        assert!(!pl.hover, "hover should be clear");
    }

    #[test]
    fn touch_roundtrip() {
        let pl = TouchPayload {
            x: 100.0,
            y: 200.0,
            radius_major: 5.0,
            radius_minor: 4.0,
            force: 0.25,
        };
        let payload_bytes = pl.into_bytes();
        let packet = Packet {
            kind: Kind::TouchBegin,
            time_us: 42,
            seq: 1,
            flags: 0,
            payload: payload_bytes,
        };
        let bytes = packet.to_bytes();
        let back = Packet::from_bytes(&bytes).unwrap();
        let pl2 = TouchPayload::from_bytes(&back.payload);
        assert!((pl2.x - 100.0).abs() < 1.0 / 65_536.0);
        assert!((pl2.force - 0.25).abs() < 1.0 / 32_768.0);
    }

    #[test]
    fn key_roundtrip() {
        let pl = KeyPayload {
            usage_page: 7,
            usage: 4,
            modifiers: 2,
        };
        let payload_bytes = pl.into_bytes();
        let packet = Packet {
            kind: Kind::KeyDown,
            time_us: 0,
            seq: 1,
            flags: 0,
            payload: payload_bytes,
        };
        let bytes = packet.to_bytes();
        let back = Packet::from_bytes(&bytes).unwrap();
        let pl2 = KeyPayload::from_bytes(&back.payload);
        assert_eq!(pl2.usage_page, 7);
        assert_eq!(pl2.usage, 4);
        assert_eq!(pl2.modifiers, 2);
    }

    #[test]
    fn rejects_short_buffer() {
        assert_eq!(
            Packet::from_bytes(&[0u8; 31]).unwrap_err(),
            DecodeError::ShortBuffer
        );
    }

    #[test]
    fn rejects_unknown_kind() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0xFF;
        assert!(matches!(
            Packet::from_bytes(&bytes).unwrap_err(),
            DecodeError::UnknownKind(0xFF)
        ));
    }

    #[test]
    fn rejects_reserved_flag_bits() {
        let p = make_pencil_packet();
        let mut bytes = p.to_bytes();
        bytes[13] = 0b0000_0100; // set a reserved bit
        assert_eq!(
            Packet::from_bytes(&bytes).unwrap_err(),
            DecodeError::ReservedBitsSet
        );
    }

    #[test]
    fn all_kinds_parse() {
        for kind_byte in [0x01u8, 0x02, 0x03, 0x10, 0x11, 0x12, 0x20, 0x21, 0x22] {
            let mut bytes = [0u8; 32];
            bytes[0] = kind_byte;
            assert!(
                Packet::from_bytes(&bytes).is_ok(),
                "kind {kind_byte:#04x} should parse"
            );
        }
    }

    #[test]
    fn zero_seq_is_valid() {
        // seq == 0 is a valid first packet
        let p = Packet {
            kind: Kind::TouchBegin,
            time_us: 0,
            seq: 0,
            flags: 0,
            payload: [0; 18],
        };
        let bytes = p.to_bytes();
        let back = Packet::from_bytes(&bytes).unwrap();
        assert_eq!(back.seq, 0);
    }

    #[test]
    fn predicted_flag_survives() {
        let mut p = make_pencil_packet();
        p.flags = 0b11; // predicted + coalesced
        let back = Packet::from_bytes(&p.to_bytes()).unwrap();
        assert_eq!(back.flags & 0b11, 0b11);
    }
}
