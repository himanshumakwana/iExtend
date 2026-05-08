//! Transport-CC → encoder bitrate controller.
//!
//! A proportional controller with deadband, rate-of-change cap, and floor/ceiling.
//!
//! ## Algorithm
//! 1. **Error metric**: `error = -(loss_pct * loss_weight + rtt_delta_ms * rtt_weight)`
//!    where `loss_weight = 4.0` and `rtt_weight = 0.04`.
//! 2. **Proportional step**: `delta = error * P_GAIN * dt_secs * target`.
//! 3. **Deadband**: if cumulative `|delta|` < `DEADBAND_KBPS` for the last
//!    `DEADBAND_HOLD_MS` of accumulated dt, ignore. Prevents oscillation on a
//!    clean link with tiny RTT jitter.
//! 4. **Rate-of-change cap**: ±25% per second.
//!    `max_delta = target * 0.25 * dt_secs`.
//! 5. **Clamp** to `[floor, ceiling]`.
//!
//! ## Notes
//! - `on_feedback` is called 4 times per second (250 ms tick from the heartbeat).
//! - The deadband accumulator uses the `dt` argument, not wall-clock time, so
//!   unit tests work correctly without `tokio::time::pause`.

use std::time::Duration;
use tracing::debug;

/// Proportional gain. Tuned for ~4 calls/second.
const P_GAIN: f64 = 1.5;
/// Weight for packet-loss percentage (per 1 % point of loss).
const LOSS_WEIGHT: f64 = 4.0;
/// Weight for RTT delta (ms above baseline).
const RTT_WEIGHT: f64 = 0.04;
/// Deadband: accumulated |delta| below this (kbps) is suppressed.
const DEADBAND_KBPS: u32 = 1_500;
/// Accumulated dt (ms) a change must stay inside the deadband before ignoring.
const DEADBAND_HOLD_MS: u64 = 200;
/// Maximum bitrate change per second as a fraction of the current target.
const MAX_CHANGE_FRACTION_PER_SEC: f64 = 0.25;

/// Transport-CC feedback sample.
#[derive(Debug, Clone, Copy)]
pub struct CcFeedback {
    /// Packet loss as a percentage (0.0 = no loss, 100.0 = all lost).
    pub loss_pct: f64,
    /// Round-trip time in milliseconds.
    pub rtt_ms: f64,
}

impl CcFeedback {
    /// Create a feedback sample.
    pub fn new(loss_pct: f64, rtt_ms: f64) -> Self {
        Self { loss_pct, rtt_ms }
    }
}

/// Proportional bitrate controller with deadband and slew-rate limit.
pub struct BitrateController {
    /// Current target bitrate (kbps).
    target_kbps: u32,
    /// Absolute floor (kbps).
    floor_kbps: u32,
    /// Absolute ceiling (kbps).
    ceiling_kbps: u32,
    /// RTT baseline (smoothed exponentially, τ ≈ 2 s).
    rtt_baseline_ms: f64,
    /// Accumulated dt (ms) while change is inside the deadband.
    deadband_acc_ms: u64,
    /// Accumulated absolute delta (kbps) during the deadband period.
    deadband_abs_kbps: u32,
}

impl BitrateController {
    /// Create a new controller.
    pub fn new(initial_kbps: u32, floor_kbps: u32, ceiling_kbps: u32) -> Self {
        Self {
            target_kbps: initial_kbps,
            floor_kbps,
            ceiling_kbps,
            rtt_baseline_ms: 20.0,
            deadband_acc_ms: 0,
            deadband_abs_kbps: 0,
        }
    }

    /// Current target bitrate (kbps).
    pub fn target_kbps(&self) -> u32 {
        self.target_kbps
    }

    /// Process one transport-CC feedback sample.
    ///
    /// Returns the new target bitrate if it changed, or `None` if the
    /// controller suppressed the change (deadband or no-op).
    pub fn on_feedback(&mut self, fb: CcFeedback, dt: Duration) -> Option<u32> {
        let dt_secs = dt.as_secs_f64().max(0.001);
        let dt_ms = dt.as_millis() as u64;

        // Update RTT baseline with exponential smoothing (α = dt/τ, τ=2 s).
        let alpha = (dt_secs / 2.0).min(1.0);
        self.rtt_baseline_ms = self.rtt_baseline_ms * (1.0 - alpha) + fb.rtt_ms * alpha;

        // Error metric: negative = congested, positive = bandwidth available.
        // Scale: 5% loss → error ≈ -20, which with P_GAIN=1.5 gives a meaningful step.
        let rtt_delta = fb.rtt_ms - self.rtt_baseline_ms;
        let error = -(fb.loss_pct * LOSS_WEIGHT + rtt_delta * RTT_WEIGHT);

        // Proportional step.
        let step_kbps = (error * self.target_kbps as f64 * P_GAIN * dt_secs) as i64;

        // Rate-of-change cap: ±25% per second.
        let max_delta = (self.target_kbps as f64 * MAX_CHANGE_FRACTION_PER_SEC * dt_secs) as i64;
        let capped = step_kbps.clamp(-max_delta, max_delta);

        // Deadband: suppress tiny changes to prevent oscillation on clean links.
        let abs_capped = capped.unsigned_abs() as u32;
        if abs_capped < DEADBAND_KBPS {
            self.deadband_acc_ms += dt_ms;
            self.deadband_abs_kbps = self.deadband_abs_kbps.saturating_add(abs_capped);
            if self.deadband_acc_ms < DEADBAND_HOLD_MS {
                return None; // hold: not yet enough accumulated time
            }
            // Deadband hold expired without exceeding threshold — suppress.
            self.deadband_acc_ms = 0;
            self.deadband_abs_kbps = 0;
            return None;
        }

        // Outside deadband: reset accumulator and apply the change.
        self.deadband_acc_ms = 0;
        self.deadband_abs_kbps = 0;

        let new_target = ((self.target_kbps as i64 + capped)
            .clamp(self.floor_kbps as i64, self.ceiling_kbps as i64))
            as u32;

        if new_target == self.target_kbps {
            return None;
        }

        debug!(
            old = self.target_kbps,
            new = new_target,
            loss_pct = fb.loss_pct,
            rtt_ms = fb.rtt_ms,
            "bitrate controller update"
        );

        self.target_kbps = new_target;
        Some(new_target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loss_drops_bitrate() {
        let mut bc = BitrateController::new(25_000, 6_000, 80_000);
        // Apply 5% loss 10 times at 250ms intervals — should push bitrate down.
        for _ in 0..10 {
            bc.on_feedback(CcFeedback::new(5.0, 20.0), Duration::from_millis(250));
        }
        assert!(
            bc.target_kbps() < 25_000,
            "5% loss should reduce bitrate; got {} kbps",
            bc.target_kbps()
        );
        assert!(
            bc.target_kbps() >= 6_000,
            "bitrate must not drop below floor 6 000 kbps; got {}",
            bc.target_kbps()
        );
    }

    #[test]
    fn no_loss_clean_link_stays_near_initial() {
        let mut bc = BitrateController::new(25_000, 6_000, 80_000);
        let initial = bc.target_kbps();
        // 100 iterations of zero-loss, stable RTT at 250ms intervals.
        for _ in 0..100 {
            bc.on_feedback(CcFeedback::new(0.0, 20.0), Duration::from_millis(250));
        }
        let delta = (bc.target_kbps() as i64 - initial as i64).abs();
        assert!(
            delta < 1_500,
            "clean link should not move bitrate by >1500 kbps; delta = {delta}"
        );
    }

    #[test]
    fn floor_is_enforced() {
        let mut bc = BitrateController::new(7_000, 6_000, 80_000);
        for _ in 0..100 {
            bc.on_feedback(CcFeedback::new(100.0, 200.0), Duration::from_millis(250));
        }
        assert!(
            bc.target_kbps() >= 6_000,
            "floor must be enforced even under extreme loss"
        );
    }

    #[test]
    fn ceiling_is_enforced() {
        let mut bc = BitrateController::new(79_000, 6_000, 80_000);
        for _ in 0..50 {
            bc.on_feedback(CcFeedback::new(0.0, 5.0), Duration::from_millis(250));
        }
        assert!(bc.target_kbps() <= 80_000, "ceiling must be enforced");
    }
}
