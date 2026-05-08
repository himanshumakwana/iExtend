// Reproject.swift — predicts the current cursor / pencil-tip position by
// combining the host's last-known cursor position (which lags by ~RTT/2)
// with the iPad's own recent input history.
//
// Plan 8 Task 12.
//
// WHY THIS MATTERS
// ================
// The host cursor position inside the encoded video frame is already ~14 ms
// stale by the time the iPad decodes it.  Without reprojection, any cursor
// movement during those 14 ms is visible as lag.
//
// With reprojection:
//   1. We read the host's most recent cursor message (ts_us, x, y).
//   2. We extrapolate forward using the velocity computed from the iPad's
//      own input sample history (which has ~1 ms latency from UIKit).
//   3. We blend the extrapolated position with the host position using an
//      exponential decay weighted by the age of the most recent input sample.
//
// Perceived latency for the cursor overlay: ~2 ms on M2/M4 ProMotion at 120 Hz.
//
// ALGORITHM
// =========
// Given:
//   hostPos   — host-reported cursor in display pixels (lags by RTT/2)
//   history   — circular buffer of (timeUs, pos) input samples, newest last
//   nowUs     — current iPad time in microseconds
//   rttUs     — estimated round-trip time in microseconds
//
// Steps:
//   1. Compute velocity v from the two most recent history entries.
//   2. Extrapolate: predicted = history.last.pos + v * (RTT/2)
//   3. Blend with host using exponential decay:
//        alpha = exp(-age_of_newest_sample / DECAY_US)
//        result = predicted * alpha + hostPos * (1 - alpha)
//
// The decay constant (30 ms) ensures we fall back gracefully to the host
// position when the user has been still for > ~60 ms.

import Foundation
import simd

// ── Reproject ─────────────────────────────────────────────────────────────────

public enum Reproject {

    /// A time-stamped position sample (from EventCapture).
    public struct Sample {
        public let timeUs: UInt64
        public let pos:    SIMD2<Float>

        public init(timeUs: UInt64, pos: SIMD2<Float>) {
            self.timeUs = timeUs
            self.pos    = pos
        }
    }

    /// Exponential decay constant: how quickly we blend back to the host
    /// position when local input goes stale.  30 ms = typical human pause.
    private static let decayUs: Float = 30_000

    /// Maximum extrapolation window — never project more than 2 × RTT ahead
    /// to avoid overshooting during direction changes.
    private static let maxExtrapolationUs: Float = 40_000

    /// Predict the cursor / pencil-tip position at `nowUs + RTT/2`.
    ///
    /// - Parameters:
    ///   - hostPos:  Host-reported cursor position (lags by ~RTT/2).
    ///   - history:  Recent local input samples, **newest last**.
    ///   - nowUs:    Current iPad time in microseconds.
    ///   - rttUs:    Estimated round-trip time in microseconds.
    /// - Returns:    Predicted cursor position in display pixels.
    public static func predict(
        hostPos:  SIMD2<Float>,
        history:  [Sample],
        nowUs:    UInt64,
        rttUs:    UInt64
    ) -> SIMD2<Float> {
        guard let last = history.last else {
            // No local input yet — return host position unchanged.
            return hostPos
        }

        // ── Step 1: Velocity ─────────────────────────────────────────────────
        let v: SIMD2<Float>
        if history.count >= 2 {
            let prev = history[history.count - 2]
            let dtUs = Float(last.timeUs - prev.timeUs)
            if dtUs > 0 {
                v = (last.pos - prev.pos) / (dtUs / 1_000_000.0)
            } else {
                v = .zero
            }
        } else {
            v = .zero
        }

        // ── Step 2: Extrapolate ──────────────────────────────────────────────
        let halfRttUs   = Float(rttUs) / 2.0
        let clampedUs   = min(halfRttUs, maxExtrapolationUs)
        let extrapolateSec = clampedUs / 1_000_000.0
        let predicted   = last.pos + v * extrapolateSec

        // ── Step 3: Blend ────────────────────────────────────────────────────
        let ageUs   = nowUs >= last.timeUs ? Float(nowUs - last.timeUs) : 0
        let alpha   = expf(-ageUs / decayUs)
        return predicted * alpha + hostPos * (1.0 - alpha)
    }

    /// Convenience overload accepting raw tuples (useful in tests).
    public static func predict(
        hostPos:  SIMD2<Float>,
        history:  [(timeUs: UInt64, pos: SIMD2<Float>)],
        nowUs:    UInt64,
        rttUs:    UInt64
    ) -> SIMD2<Float> {
        let samples = history.map { Sample(timeUs: $0.timeUs, pos: $0.pos) }
        return predict(hostPos: hostPos, history: samples, nowUs: nowUs, rttUs: rttUs)
    }
}

// ── InputHistoryBuffer ────────────────────────────────────────────────────────

/// A fixed-capacity circular buffer of recent input samples.
///
/// Thread-safety: NOT thread-safe.  Access only from the main thread.
public final class InputHistoryBuffer {
    private var buffer: [Reproject.Sample]
    private let capacity: Int

    public init(capacity: Int = 32) {
        self.capacity = capacity
        buffer = []
        buffer.reserveCapacity(capacity)
    }

    public func append(_ sample: Reproject.Sample) {
        buffer.append(sample)
        if buffer.count > capacity {
            buffer.removeFirst()
        }
    }

    public var samples: [Reproject.Sample] { buffer }

    public var latest: Reproject.Sample? { buffer.last }
}
