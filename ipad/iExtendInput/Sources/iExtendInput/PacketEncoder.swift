// PacketEncoder.swift — 32-byte wire packet for the iExtend input DataChannel.
//
// This file mirrors host/crates/ix-input/src/wire.rs exactly.
// Both sides must round-trip the shared JSON corpus in
// host/crates/ix-input/tests/wire_vectors.json byte-for-byte.
//
// Wire format (spec §6.1):
//   Offset  Size  Field
//   0       1     kind     (PacketKind raw value)
//   1       8     timeUs   (UInt64 LE, mach_absolute_time in µs)
//   9       4     seq      (UInt32 LE, monotonic per-channel)
//   13      1     flags    (bit 0 = predicted, bit 1 = coalesced; bits 2-7 = 0)
//   14      18    payload  (kind-specific — see set* helpers below)
//
// Q16 fixed-point: signed integer with denominator 65 536.
//   Coordinates  : i32 Q16  (4 bytes, large range for pixel coords)
//   Pressure etc : i16 Q16  (2 bytes, saturates at 0.5 absolute value)

import Foundation

// ── PacketKind ────────────────────────────────────────────────────────────────

public enum PacketKind: UInt8 {
    case touchBegin  = 0x01
    case touchMove   = 0x02
    case touchEnd    = 0x03
    case pencilBegin = 0x10
    case pencilMove  = 0x11
    case pencilEnd   = 0x12
    case keyDown     = 0x20
    case keyUp       = 0x21
    case modifier    = 0x22
}

// ── Packet ────────────────────────────────────────────────────────────────────

/// A 32-byte fixed-size wire packet.
public struct Packet {
    /// Total on-wire size of every packet.
    public static let length = 32

    public var kind:    PacketKind = .touchMove
    public var timeUs:  UInt64 = 0
    public var seq:     UInt32 = 0
    /// Flags: bit 0 = predicted, bit 1 = coalesced.  Bits 2-7 must be zero.
    public var flags:   UInt8  = 0
    /// 18 raw payload bytes.
    public var payload: [UInt8] = Array(repeating: 0, count: 18)

    public init() {}

    // ── Deserialise ──────────────────────────────────────────────────────────

    public enum DecodeError: Error {
        case shortBuffer
        case unknownKind(UInt8)
        case reservedBitsSet
    }

    public init(bytes: [UInt8]) throws {
        guard bytes.count >= Packet.length else { throw DecodeError.shortBuffer }
        guard let k = PacketKind(rawValue: bytes[0]) else {
            throw DecodeError.unknownKind(bytes[0])
        }
        guard bytes[13] & 0b1111_1100 == 0 else { throw DecodeError.reservedBitsSet }
        kind = k
        // loadUnaligned requires Swift 5.7+; available on iOS 16+.
        timeUs = bytes.withUnsafeBytes {
            UInt64(littleEndian: $0.loadUnaligned(fromByteOffset: 1, as: UInt64.self))
        }
        seq = bytes.withUnsafeBytes {
            UInt32(littleEndian: $0.loadUnaligned(fromByteOffset: 9, as: UInt32.self))
        }
        flags   = bytes[13]
        payload = Array(bytes[14..<32])
    }

    // ── Serialise ────────────────────────────────────────────────────────────

    public func encoded() -> [UInt8] {
        var out = [UInt8](repeating: 0, count: 32)
        out[0] = kind.rawValue
        withUnsafeBytes(of: timeUs.littleEndian) { src in
            for (i, b) in src.enumerated() { out[1 + i] = b }
        }
        withUnsafeBytes(of: seq.littleEndian) { src in
            for (i, b) in src.enumerated() { out[9 + i] = b }
        }
        out[13] = flags
        for (i, b) in payload.enumerated() { out[14 + i] = b }
        return out
    }
}

// ── Q16 helpers ───────────────────────────────────────────────────────────────

/// Q16 fixed-point encoding utilities.  Internal; exposed for test verification.
public enum Q16 {
    /// Encode a Float as a 4-byte i32 Q16 (little-endian).
    /// Range: ±32 767 integer part — sufficient for pixel coordinates up to
    /// 2732 px (iPad Pro 12.9") with fractional precision of 1/65536.
    public static func writeI32(_ buf: inout [UInt8], offset: Int, value: Float) {
        let q = Int32((value * 65_536.0).rounded()).littleEndian
        withUnsafeBytes(of: q) { src in
            for (i, b) in src.enumerated() { buf[offset + i] = b }
        }
    }

    /// Encode a Float as a 2-byte i16 Q16 (little-endian), saturating.
    /// Values outside −0.5..+0.5 (in the original scale) saturate at i16 limits.
    /// Callers must scale their units so that pressure, tilt, etc. fit.
    public static func writeI16(_ buf: inout [UInt8], offset: Int, value: Float) {
        let q = Int16(clamping: Int64((value * 65_536.0).rounded())).littleEndian
        withUnsafeBytes(of: q) { src in
            for (i, b) in src.enumerated() { buf[offset + i] = b }
        }
    }

    /// Decode a 4-byte LE i32 Q16 back to Float.
    public static func readI32(_ buf: [UInt8], offset: Int) -> Float {
        let raw = buf.withUnsafeBytes { ptr in
            Int32(littleEndian: ptr.loadUnaligned(fromByteOffset: offset, as: Int32.self))
        }
        return Float(raw) / 65_536.0
    }

    /// Decode a 2-byte LE i16 Q16 back to Float.
    public static func readI16(_ buf: [UInt8], offset: Int) -> Float {
        let raw = buf.withUnsafeBytes { ptr in
            Int16(littleEndian: ptr.loadUnaligned(fromByteOffset: offset, as: Int16.self))
        }
        return Float(raw) / 65_536.0
    }
}

// ── Payload setters ───────────────────────────────────────────────────────────

public extension Packet {
    /// Set the 18-byte pencil payload.
    ///
    /// - Parameters:
    ///   - x, y:      iPad logical pixels (i32 Q16).
    ///   - pressure:  0..=1 (i16 Q16, saturates above 0.5).
    ///   - tilt:      altitude angle in radians, 0..=π/2 (i16 Q16, saturates).
    ///   - azimuth:   azimuth in radians, 0..=2π (i16 Q16, saturates for > π/2).
    ///   - twist:     Pencil Pro roll, 0..=2π (i16 Q16).  Pass 0 for Pencil 2.
    ///   - barrel:    true if the barrel button is pressed.
    ///   - hover:     true if the pencil is hovering (not in contact).
    mutating func setPencil(
        x: Float, y: Float,
        pressure: Float, tilt: Float,
        azimuth: Float, twist: Float,
        barrel: Bool, hover: Bool
    ) {
        var p = [UInt8](repeating: 0, count: 18)
        Q16.writeI32(&p, offset: 0,  value: x)
        Q16.writeI32(&p, offset: 4,  value: y)
        Q16.writeI16(&p, offset: 8,  value: pressure)
        Q16.writeI16(&p, offset: 10, value: tilt)
        Q16.writeI16(&p, offset: 12, value: azimuth)
        Q16.writeI16(&p, offset: 14, value: twist)
        p[16] = barrel ? 1 : 0
        p[17] = hover  ? 1 : 0
        payload = p
    }

    /// Set the 18-byte touch payload.
    mutating func setTouch(
        x: Float, y: Float,
        radiusMajor: Float, radiusMinor: Float, force: Float
    ) {
        var p = [UInt8](repeating: 0, count: 18)
        Q16.writeI32(&p, offset: 0,  value: x)
        Q16.writeI32(&p, offset: 4,  value: y)
        Q16.writeI16(&p, offset: 8,  value: radiusMajor)
        Q16.writeI16(&p, offset: 10, value: radiusMinor)
        Q16.writeI16(&p, offset: 12, value: force)
        // bytes 14-17 remain zero (reserved)
        payload = p
    }

    /// Set the 18-byte key payload.
    mutating func setKey(usagePage: UInt16, usage: UInt16, mods: UInt16) {
        var p = [UInt8](repeating: 0, count: 18)
        withUnsafeBytes(of: usagePage.littleEndian) {
            for (i, b) in $0.enumerated() { p[0 + i] = b }
        }
        withUnsafeBytes(of: usage.littleEndian) {
            for (i, b) in $0.enumerated() { p[2 + i] = b }
        }
        withUnsafeBytes(of: mods.littleEndian) {
            for (i, b) in $0.enumerated() { p[4 + i] = b }
        }
        // bytes 6-17 remain zero (reserved)
        payload = p
    }
}

// ── String extension for hex decoding (test support) ─────────────────────────

public extension String {
    /// Decode a hex string (even length, no separators) to bytes.
    func hexBytes() -> [UInt8] {
        var out: [UInt8] = []
        out.reserveCapacity(count / 2)
        var i = startIndex
        while i < endIndex {
            let j = index(i, offsetBy: 2)
            guard let byte = UInt8(self[i..<j], radix: 16) else { return [] }
            out.append(byte)
            i = j
        }
        return out
    }
}
