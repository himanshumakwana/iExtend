// FrameQueue.swift
// Lock-free single-producer / single-consumer ring buffer for decoded video frames.
// The producer is the VTDecompressionSession callback thread (DecodeSession._onDecodedFrame).
// The consumer is the CADisplayLink callback on the main thread (MetalRenderer.displayLinkFired).
//
// Design:
//   - Power-of-two capacity so modulo is a bitmask operation.
//   - Swift Atomics (apple/swift-atomics 1.2.0) for the head/tail indices.
//   - No allocation on the hot path — the ring slots are pre-allocated.
//   - When full, the oldest unread frame is silently dropped (oldest-drop policy).
//
// Spec §8.4: "FrameQueue capacity MUST be ≥ 4 and SHOULD be ≤ 16 to bound latency."
// We default to 8.

import Foundation
import Atomics
import CoreVideo
import CoreMedia

// MARK: - Frame slot

private struct FrameSlot: @unchecked Sendable {
    var pixelBuffer: CVPixelBuffer?
    var pts: CMTime = .invalid
}

// MARK: - FrameQueue

/// Lock-free SPSC ring buffer. All methods are safe to call from any thread;
/// enqueue from exactly one thread and dequeue from exactly one (different) thread.
public final class FrameQueue: @unchecked Sendable {

    // MARK: Public constants
    public let capacity: Int

    // MARK: Private storage
    private let slots: UnsafeMutableBufferPointer<FrameSlot>
    private let mask: Int
    private let head = ManagedAtomic<Int>(0)   // producer writes here
    private let tail = ManagedAtomic<Int>(0)   // consumer reads from here

    // MARK: Init / deinit

    /// - Parameter capacity: Must be a power of two, ≥ 4. Rounded up if not.
    public init(capacity: Int = 8) {
        let cap = capacity.nextPowerOfTwo(minimum: 4)
        self.capacity = cap
        self.mask = cap - 1
        self.slots = UnsafeMutableBufferPointer.allocate(capacity: cap)
        self.slots.initialize(repeating: FrameSlot())
    }

    deinit {
        slots.deinitialize()
        slots.deallocate()
    }

    // MARK: - Producer API (call from decode thread only)

    /// Enqueue a decoded frame. Drops the oldest frame if full.
    public func enqueue(_ pixelBuffer: CVPixelBuffer, pts: CMTime) {
        let h = head.load(ordering: .relaxed)
        let t = tail.load(ordering: .acquiring)

        if h - t == capacity {
            // Full: advance tail to drop oldest (oldest-drop policy).
            _ = tail.loadThenWrappingIncrement(ordering: .releasing)
        }

        slots[h & mask] = FrameSlot(pixelBuffer: pixelBuffer, pts: pts)
        head.wrappingIncrement(ordering: .releasing)
    }

    // MARK: - Consumer API (call from render thread only)

    /// Dequeue the oldest frame. Returns nil if empty.
    public func dequeue() -> (pixelBuffer: CVPixelBuffer, pts: CMTime)? {
        let t = tail.load(ordering: .relaxed)
        let h = head.load(ordering: .acquiring)

        guard h != t else { return nil }

        let slot = slots[t & mask]
        tail.wrappingIncrement(ordering: .releasing)

        guard let pb = slot.pixelBuffer else { return nil }
        return (pb, slot.pts)
    }

    /// Peek without consuming. Useful for the renderer to check freshness.
    public func peek() -> (pixelBuffer: CVPixelBuffer, pts: CMTime)? {
        let t = tail.load(ordering: .relaxed)
        let h = head.load(ordering: .acquiring)
        guard h != t else { return nil }
        guard let pb = slots[t & mask].pixelBuffer else { return nil }
        return (pb, slots[t & mask].pts)
    }

    /// Number of frames currently in the ring.
    public var count: Int {
        let h = head.load(ordering: .relaxed)
        let t = tail.load(ordering: .relaxed)
        return h - t
    }

    /// True if no frames are available.
    public var isEmpty: Bool { count == 0 }

    /// Drain all frames (called on stream teardown).
    public func flush() {
        let h = head.load(ordering: .relaxed)
        for i in 0..<capacity {
            slots[i] = FrameSlot()
        }
        tail.store(h, ordering: .releasing)
    }
}

// MARK: - Int power-of-two helper

private extension Int {
    func nextPowerOfTwo(minimum: Int) -> Int {
        var n = Swift.max(self, minimum)
        // Already a power of two?
        if n & (n - 1) == 0 { return n }
        n -= 1
        n |= n >> 1; n |= n >> 2; n |= n >> 4; n |= n >> 8; n |= n >> 16
        return n + 1
    }
}
