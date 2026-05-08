// EventCapture.swift — captures UITouch, Pencil, and keyboard events from the
// active UIWindow and serialises each event into a 32-byte wire Packet.
//
// Plan 8 fills in the Plan 6 stub.  The class is intentionally thin:
//   - It does NOT manage the DataChannel — the caller's onPacket closure does.
//   - It does NOT own the window — it observes it.
//
// THREADING: onPacket is called on the main thread (UIKit's thread requirement).
//
// TESTING: The public didReceive(touches:) / didReceive(pencils:) /
//          didReceive(presses:) entry points accept protocol types (TouchLike,
//          PencilLike, PressLike) so tests can pass synthetic events without
//          needing real UITouch / UIPress instances.

import UIKit

// ── Protocols for testability ─────────────────────────────────────────────────

/// Abstraction over UITouch used by EventCapture so tests can inject synthetic events.
public protocol TouchLike: AnyObject {
    var phase:        UITouch.Phase { get }
    var location:     CGPoint       { get }
    var force:        CGFloat       { get }
    var majorRadius:  CGFloat       { get }
}

/// Abstraction over Apple Pencil touches.
public protocol PencilLike: AnyObject {
    var phase:         UITouch.Phase { get }
    var location:      CGPoint       { get }
    var force:         CGFloat       { get }
    var majorRadius:   CGFloat       { get }
    var altitudeAngle: CGFloat       { get }   // 0..π/2 = vertical; platform altitude
    var azimuthAngle:  CGFloat       { get }   // 0..2π
    var twistRoll:     CGFloat       { get }   // Pencil Pro; 0 otherwise
    var hover:         Bool          { get }
    var predicted:     Bool          { get }
    var barrelButton:  Bool          { get }
}

/// Abstraction over UIPress for keyboard capture.
public protocol PressLike {
    var phase: UIPress.Phase { get }
    var key:   KeyData       { get }
}

/// Decoded key data (HID page + usage + modifiers).
public struct KeyData {
    public let usagePage: UInt16
    public let usage:     UInt16
    public let mods:      UInt16

    public init(usagePage: UInt16, usage: UInt16, mods: UInt16) {
        self.usagePage = usagePage
        self.usage     = usage
        self.mods      = mods
    }
}

// ── EventCapture ─────────────────────────────────────────────────────────────

/// Captures UIWindow input events and serialises them as wire Packets.
///
/// Usage:
/// ```swift
/// let cap = EventCapture()
/// cap.onPacket = { pkt in channel.send(pkt.encoded()) }
/// cap.attach(to: window)
/// ```
public final class EventCapture {

    /// Called on the main thread for each serialised packet.
    public var onPacket: ((Packet) -> Void)?

    /// Weak reference to the attached window (set by attach(to:)).
    public weak var window: UIWindow?

    /// Most recent pencil sample (x, y) — exposed so PencilHud can read it.
    public private(set) var lastPencilLocation: CGPoint = .zero

    private var _seq: UInt32 = 0

    public init() {}

    // ── Attach ────────────────────────────────────────────────────────────────

    /// Attach the capture to a UIWindow.  In production use `IExtendWindow`
    /// (a UIWindow subclass that overrides sendEvent); this method exists so
    /// the class can store the window reference for coordinate system queries.
    public func attach(to window: UIWindow) {
        self.window = window
    }

    // ── UITouch dispatch ──────────────────────────────────────────────────────

    /// Dispatch finger touch events.  Called by IExtendWindow.sendEvent for
    /// non-pencil touches.
    public func didReceive(touches: [TouchLike]) {
        for t in touches {
            var p = Packet()
            p.kind   = mapPhase(t.phase, isPencil: false)
            p.timeUs = nowMicros()
            p.seq    = nextSeq()
            p.flags  = 0
            p.setTouch(
                x: Float(t.location.x),
                y: Float(t.location.y),
                radiusMajor: Float(t.majorRadius),
                radiusMinor: 0,
                force: Float(t.force)
            )
            onPacket?(p)
        }
    }

    // ── Apple Pencil dispatch ─────────────────────────────────────────────────

    /// Dispatch Pencil events including predicted touches.
    ///
    /// - Parameter pencils: Real + predicted pencil touches.  Set the
    ///   `predicted` flag on predicted samples — the Dispatcher on the host
    ///   will not drop them, but the host may choose to process them at lower
    ///   priority.
    public func didReceive(pencils: [PencilLike]) {
        for pen in pencils {
            lastPencilLocation = pen.location

            var pkt = Packet()
            pkt.kind   = mapPhase(pen.phase, isPencil: true)
            pkt.timeUs = nowMicros()
            pkt.seq    = nextSeq()
            pkt.flags  = pen.predicted ? 0b01 : 0

            // Convert UIKit altitude (0 = horizontal, π/2 = vertical) to
            // the wire tilt convention (0 = vertical, π/2 = horizontal):
            //   wire_tilt = π/2 − altitude
            let wireTilt = Float(Float.pi / 2 - pen.altitudeAngle)

            pkt.setPencil(
                x: Float(pen.location.x),
                y: Float(pen.location.y),
                pressure: Float(pen.force),
                tilt:     wireTilt,
                azimuth:  Float(pen.azimuthAngle),
                twist:    Float(pen.twistRoll),
                barrel:   pen.barrelButton,
                hover:    pen.hover
            )
            onPacket?(pkt)
        }
    }

    // ── Keyboard dispatch ─────────────────────────────────────────────────────

    /// Dispatch hardware keyboard press events.
    public func didReceive(presses: [PressLike]) {
        for press in presses {
            var pkt = Packet()
            pkt.kind   = press.phase == .ended ? .keyUp : .keyDown
            pkt.timeUs = nowMicros()
            pkt.seq    = nextSeq()
            pkt.flags  = 0
            pkt.setKey(
                usagePage: press.key.usagePage,
                usage:     press.key.usage,
                mods:      press.key.mods
            )
            onPacket?(pkt)
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    private func mapPhase(_ phase: UITouch.Phase, isPencil: Bool) -> PacketKind {
        switch phase {
        case .began:
            return isPencil ? .pencilBegin : .touchBegin
        case .moved, .stationary:
            return isPencil ? .pencilMove  : .touchMove
        case .ended, .cancelled:
            return isPencil ? .pencilEnd   : .touchEnd
        @unknown default:
            return isPencil ? .pencilMove  : .touchMove
        }
    }

    private func nextSeq() -> UInt32 {
        _seq = _seq &+ 1
        return _seq
    }

    private func nowMicros() -> UInt64 {
        var info = mach_timebase_info_data_t()
        mach_timebase_info(&info)
        let abs = mach_absolute_time()
        // Convert mach ticks → nanoseconds → microseconds.
        return (abs * UInt64(info.numer)) / (UInt64(info.denom) * 1_000)
    }
}

// ── Production UIWindow subclass ──────────────────────────────────────────────

/// UIWindow subclass that feeds all events to an attached EventCapture.
///
/// Usage in AppDelegate / SceneDelegate:
/// ```swift
/// let window = IExtendWindow(windowScene: scene)
/// window.capture.onPacket = { pkt in channel.send(pkt.encoded()) }
/// ```
public final class IExtendWindow: UIWindow {
    public let capture = EventCapture()

    override init(frame: CGRect) {
        super.init(frame: frame)
        capture.attach(to: self)
    }

    @available(iOS 13.0, *)
    override init(windowScene: UIWindowScene) {
        super.init(windowScene: windowScene)
        capture.attach(to: self)
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        capture.attach(to: self)
    }

    override public func sendEvent(_ event: UIEvent) {
        if let touches = event.allTouches {
            var pencils: [RealPencilTouch] = []
            var fingers: [RealTouch]       = []

            for t in touches {
                if t.type == .pencil {
                    // Real samples
                    pencils.append(RealPencilTouch(t, predicted: false))
                    // Predicted samples (Apple Pencil only from iOS 9+)
                    for pred in event.predictedTouches(for: t) ?? [] {
                        pencils.append(RealPencilTouch(pred, predicted: true))
                    }
                } else {
                    fingers.append(RealTouch(t))
                }
            }

            if !pencils.isEmpty { capture.didReceive(pencils: pencils) }
            if !fingers.isEmpty { capture.didReceive(touches: fingers) }
        }
        super.sendEvent(event)
    }
}

// ── Real UITouch adapters ─────────────────────────────────────────────────────

private final class RealTouch: TouchLike {
    let phase:       UITouch.Phase
    let location:    CGPoint
    let force:       CGFloat
    let majorRadius: CGFloat

    init(_ t: UITouch) {
        phase       = t.phase
        location    = t.preciseLocation(in: nil)
        force       = t.force
        majorRadius = t.majorRadius
    }
}

private final class RealPencilTouch: PencilLike {
    let phase:         UITouch.Phase
    let location:      CGPoint
    let force:         CGFloat
    let majorRadius:   CGFloat
    let altitudeAngle: CGFloat
    let azimuthAngle:  CGFloat
    let twistRoll:     CGFloat
    let hover:         Bool
    let predicted:     Bool
    let barrelButton:  Bool

    init(_ t: UITouch, predicted: Bool) {
        phase         = t.phase
        location      = t.preciseLocation(in: nil)
        force         = t.force
        majorRadius   = t.majorRadius
        altitudeAngle = t.altitudeAngle
        azimuthAngle  = t.azimuthAngle(in: nil)
        barrelButton  = false // real barrel detection via UIPencilInteraction
        hover         = t.phase == .regionEntered || t.phase == .regionMoved

        // Pencil Pro twist (iPadOS 17.5+)
        if #available(iOS 17.5, *) {
            twistRoll = CGFloat(t.rollAngle)
        } else {
            twistRoll = 0
        }

        self.predicted = predicted
    }
}
