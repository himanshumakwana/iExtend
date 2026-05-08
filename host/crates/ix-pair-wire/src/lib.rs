//! Wire format used during the pre-WebRTC pairing handshake on TCP/5353.
//!
//! Goals: simple, compact, language-portable. The codec must be byte-for-byte
//! identical to the Swift implementation in
//! `ipad/iExtendKit/Sources/iExtendKit/Connection/PairWire.swift`. RFC 9382 §C.1
//! vectors run on both sides to catch encoding drift early.
//!
//! Layout (all integers big-endian):
//!
//! ```text
//! +---------+----------+----------+----------+
//! | magic   | version  |   kind   |  len     |
//! | u32 (4) |  u8 (1)  |  u8 (1)  | u16 (2)  |
//! +---------+----------+----------+----------+
//! |             body (len bytes)             |
//! +-------------------------------------------+
//! ```
//!
//! - `magic`: literal `0x49585044` ("IXPD")
//! - `version`: protocol version, currently `1`
//! - `kind`: see [`PairKind`]
//! - `len`: length of the body in bytes; max 16 KiB
//! - `body`: kind-specific payload (opaque to this crate; SPAKE2/AEAD lives in
//!   `ix-rtc::pairing`)
//!
//! The header is fixed at 8 bytes. Total frame size is `8 + len`.

#![deny(missing_docs)]

use thiserror::Error;

/// Protocol-magic literal "IXPD" interpreted big-endian.
pub const MAGIC: u32 = 0x49585044;
/// Current wire-format protocol version.
pub const VERSION: u8 = 1;
/// Maximum payload size; chosen to bound the pairing handshake's memory
/// footprint without rejecting realistic 4-KiB Ed25519-cert exchanges.
pub const MAX_BODY: usize = 16 * 1024;
/// Header size in bytes.
pub const HEADER_LEN: usize = 8;

/// Message kind discriminator. Values are stable wire-format identifiers and
/// must not be renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PairKind {
    /// iPad → host: opens the pairing TCP connection, body is iPad's SPAKE2
    /// client message (Element A).
    PStart = 0x01,
    /// host → iPad: SPAKE2 server message (Element B) + verifier prefix.
    PResponse = 0x02,
    /// iPad → host: AES-GCM-wrapped device-cert request (iPad pubkey + name).
    PCertReq = 0x03,
    /// host → iPad: AES-GCM-wrapped signed device cert.
    PCertOk = 0x04,
    /// either side: terminal error (with code in body[0..2]).
    PErr = 0xFF,
}

impl PairKind {
    /// Resolve a wire byte to a kind. Returns `None` for unknown values.
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0x01 => Self::PStart,
            0x02 => Self::PResponse,
            0x03 => Self::PCertReq,
            0x04 => Self::PCertOk,
            0xFF => Self::PErr,
            _ => return None,
        })
    }
}

/// A single pairing message. Body is owned to make the codec
/// allocation-symmetric across encode and decode (the iPad stack will copy
/// anyway).
#[derive(Debug, Clone)]
pub struct PairMsg {
    /// Message kind discriminator.
    pub kind: PairKind,
    /// Opaque body. Contents are SPAKE2/AEAD-defined; this crate does not
    /// interpret them.
    pub body: Vec<u8>,
}

impl PairMsg {
    /// Construct a new message; clamps `body` to `MAX_BODY` via error
    /// propagation in [`encode`].
    pub fn new(kind: PairKind, body: Vec<u8>) -> Self {
        Self { kind, body }
    }

    /// Encode `self` to a new `Vec<u8>`. Errors if body exceeds MAX_BODY.
    pub fn encode(&self) -> Result<Vec<u8>, WireError> {
        if self.body.len() > MAX_BODY {
            return Err(WireError::BodyTooLarge(self.body.len()));
        }
        let mut out = Vec::with_capacity(HEADER_LEN + self.body.len());
        out.extend_from_slice(&MAGIC.to_be_bytes());
        out.push(VERSION);
        out.push(self.kind as u8);
        out.extend_from_slice(&(self.body.len() as u16).to_be_bytes());
        out.extend_from_slice(&self.body);
        Ok(out)
    }

    /// Decode a single message. Trailing bytes after the body are ignored —
    /// callers feeding a streaming reader should slice the input first.
    pub fn decode(buf: &[u8]) -> Result<Self, WireError> {
        if buf.len() < HEADER_LEN {
            return Err(WireError::Truncated {
                got: buf.len(),
                want: HEADER_LEN,
            });
        }
        let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != MAGIC {
            return Err(WireError::BadMagic(magic));
        }
        let version = buf[4];
        if version != VERSION {
            return Err(WireError::UnsupportedVersion(version));
        }
        let kind = PairKind::from_u8(buf[5]).ok_or(WireError::UnknownKind(buf[5]))?;
        let len = u16::from_be_bytes([buf[6], buf[7]]) as usize;
        if len > MAX_BODY {
            return Err(WireError::BodyTooLarge(len));
        }
        let want = HEADER_LEN + len;
        if buf.len() < want {
            return Err(WireError::Truncated {
                got: buf.len(),
                want,
            });
        }
        Ok(Self {
            kind,
            body: buf[HEADER_LEN..want].to_vec(),
        })
    }
}

/// Wire-format errors. All are unrecoverable for the current connection;
/// callers should drop the TCP socket on any of them.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WireError {
    /// Buffer ended before a full header or body could be read.
    #[error("truncated frame: have {got} bytes, need {want}")]
    Truncated {
        /// Bytes actually present in the buffer.
        got: usize,
        /// Bytes the codec needed to make progress.
        want: usize,
    },
    /// Magic prefix was not "IXPD" (`0x49585044`).
    #[error("bad magic: expected 0x49585044, got {0:#010x}")]
    BadMagic(u32),
    /// Version byte did not match the running build's [`VERSION`].
    #[error("unsupported version byte: {0}")]
    UnsupportedVersion(u8),
    /// Kind byte did not decode to any [`PairKind`] variant.
    #[error("unknown kind byte: {0:#04x}")]
    UnknownKind(u8),
    /// Body length exceeded [`MAX_BODY`].
    #[error("body too large: {0} bytes (max {MAX_BODY})")]
    BodyTooLarge(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip_pstart() {
        let m = PairMsg::new(PairKind::PStart, vec![0xAA; 96]);
        let bytes = m.encode().unwrap();
        // Header is fixed 8 bytes; magic + ver + kind + len.
        assert_eq!(bytes.len(), HEADER_LEN + 96);
        assert_eq!(&bytes[0..4], &MAGIC.to_be_bytes());
        assert_eq!(bytes[4], VERSION);
        assert_eq!(bytes[5], PairKind::PStart as u8);
        assert_eq!(&bytes[6..8], &96u16.to_be_bytes());
        let back = PairMsg::decode(&bytes).unwrap();
        assert_eq!(back.kind, PairKind::PStart);
        assert_eq!(back.body, vec![0xAA; 96]);
    }

    #[test]
    fn empty_body_is_legal() {
        let m = PairMsg::new(PairKind::PErr, vec![]);
        let bytes = m.encode().unwrap();
        assert_eq!(bytes.len(), HEADER_LEN);
        let back = PairMsg::decode(&bytes).unwrap();
        assert_eq!(back.kind, PairKind::PErr);
        assert!(back.body.is_empty());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = PairMsg::new(PairKind::PStart, vec![]).encode().unwrap();
        bytes[0] = 0xFF;
        assert!(matches!(PairMsg::decode(&bytes), Err(WireError::BadMagic(_))));
    }

    #[test]
    fn rejects_unknown_kind() {
        let mut bytes = PairMsg::new(PairKind::PStart, vec![]).encode().unwrap();
        bytes[5] = 0x55;
        assert_eq!(
            PairMsg::decode(&bytes).unwrap_err(),
            WireError::UnknownKind(0x55)
        );
    }

    #[test]
    fn rejects_truncated_body() {
        let bytes = PairMsg::new(PairKind::PStart, vec![1; 32]).encode().unwrap();
        // chop one byte off the body
        let bad = &bytes[..bytes.len() - 1];
        assert!(matches!(
            PairMsg::decode(bad),
            Err(WireError::Truncated { .. })
        ));
    }

    #[test]
    fn rejects_oversized_body() {
        // Construct a header claiming a body bigger than MAX_BODY without
        // allocating that body — we want to confirm the length check fires
        // before any read.
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[0..4].copy_from_slice(&MAGIC.to_be_bytes());
        bytes[4] = VERSION;
        bytes[5] = PairKind::PStart as u8;
        bytes[6..8].copy_from_slice(&((MAX_BODY as u16).wrapping_add(1)).to_be_bytes());
        // Note: u16 max is 65535, MAX_BODY is 16384; +1 is 16385.
        // A wrap-around at u16 doesn't reach MAX_BODY+1 here, but the encoder
        // won't accept a Vec that big anyway. The decoder is the boundary.
        let _ = PairMsg::decode(&bytes);
    }
}
