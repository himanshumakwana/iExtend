// PairWire.swift
//
// Mirror of host/crates/ix-pair-wire/src/lib.rs. Wire-format codec used during
// the pre-WebRTC pairing handshake. Byte-for-byte identical to the Rust crate
// — see `PairWireTests.swift` for the shared vectors.
//
// Status: Plan 7 stub. Compiles and round-trips, but the SPAKE2 client and
// the Network.framework TCP plumbing live in PairingFlow.swift. macOS-side
// XCTest verification of the cross-language vectors is pending — Plan 7
// "complete pending macOS test".

import Foundation

public enum PairWireError: Error, Equatable {
    case truncated(got: Int, want: Int)
    case badMagic(UInt32)
    case unsupportedVersion(UInt8)
    case unknownKind(UInt8)
    case bodyTooLarge(Int)
}

public enum PairKind: UInt8 {
    case pStart    = 0x01
    case pResponse = 0x02
    case pCertReq  = 0x03
    case pCertOk   = 0x04
    case pErr      = 0xFF
}

public struct PairWire {
    public static let magic: UInt32 = 0x49585044   // "IXPD"
    public static let version: UInt8 = 1
    public static let maxBody: Int = 16 * 1024
    public static let headerLen: Int = 8

    public let kind: PairKind
    public let body: Data

    public init(kind: PairKind, body: Data) {
        self.kind = kind
        self.body = body
    }

    public func encode() throws -> Data {
        if body.count > Self.maxBody {
            throw PairWireError.bodyTooLarge(body.count)
        }
        var out = Data(capacity: Self.headerLen + body.count)
        var magicBE = Self.magic.bigEndian
        withUnsafeBytes(of: &magicBE) { out.append(contentsOf: $0) }
        out.append(Self.version)
        out.append(kind.rawValue)
        var lenBE = UInt16(body.count).bigEndian
        withUnsafeBytes(of: &lenBE) { out.append(contentsOf: $0) }
        out.append(body)
        return out
    }

    public static func decode(_ buf: Data) throws -> PairWire {
        guard buf.count >= headerLen else {
            throw PairWireError.truncated(got: buf.count, want: headerLen)
        }
        let magicBytes = buf.prefix(4)
        let m = magicBytes.withUnsafeBytes { $0.load(as: UInt32.self).bigEndian }
        guard m == magic else { throw PairWireError.badMagic(m) }
        let v = buf[4]
        guard v == version else { throw PairWireError.unsupportedVersion(v) }
        guard let kind = PairKind(rawValue: buf[5]) else {
            throw PairWireError.unknownKind(buf[5])
        }
        let lenBytes = buf.subdata(in: 6..<8)
        let len = Int(lenBytes.withUnsafeBytes { $0.load(as: UInt16.self).bigEndian })
        if len > maxBody { throw PairWireError.bodyTooLarge(len) }
        let want = headerLen + len
        guard buf.count >= want else {
            throw PairWireError.truncated(got: buf.count, want: want)
        }
        let body = buf.subdata(in: headerLen..<want)
        return PairWire(kind: kind, body: body)
    }
}
