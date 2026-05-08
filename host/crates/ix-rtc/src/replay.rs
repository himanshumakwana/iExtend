//! Per-DataChannel sliding-window replay protection.
//!
//! Per spec §7.5, both control and input DataChannels carry a monotonic per-
//! channel sequence number; this module enforces that no seq is accepted
//! twice. Reorder within a 1024-slot window is allowed; older or duplicate
//! seqs are rejected.
//!
//! Implementation: bitset of size 1024 (`[u64; 16]`). Each insert advances the
//! window's high-water mark and shifts the bitset; bits represent "have we
//! already seen that seq". Constant-time per insert.

#![deny(missing_docs)]

use thiserror::Error;

/// Sliding-window size in slots. Must be a multiple of 64.
pub const WINDOW_BITS: usize = 1024;
const WORDS: usize = WINDOW_BITS / 64;

/// Reasons a sequence number is rejected.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReplayError {
    /// Seq is below the bottom of the window — it's gone.
    #[error("seq {0} too old (window is [{1}, {2}])")]
    TooOld(u64, u64, u64),
    /// Seq has already been seen within the window.
    #[error("seq {0} already seen")]
    Duplicate(u64),
}

/// Sliding-window replay-protection state. One per DataChannel direction.
#[derive(Debug)]
pub struct Window {
    /// Highest seq accepted so far. Zero = no seqs accepted yet (and seq 0 is
    /// reserved as a sentinel; the protocol starts at 1).
    high: u64,
    bits: [u64; WORDS],
}

impl Window {
    /// New empty window. Accepts the first seq ≥ 1.
    pub fn new() -> Self {
        Self {
            high: 0,
            bits: [0; WORDS],
        }
    }

    /// Try to accept `seq`. Returns `Ok(())` if it's new; updates internal
    /// state. Returns `Err(...)` if it's a replay or too old. Constant-time.
    ///
    /// The seq numbering is 1-based; `seq == 0` is rejected as TooOld since
    /// `high` starts at 0.
    pub fn check_and_advance(&mut self, seq: u64) -> Result<(), ReplayError> {
        if seq == 0 || (self.high >= WINDOW_BITS as u64 && seq < self.high - WINDOW_BITS as u64 + 1)
        {
            let lo = self.high.saturating_sub(WINDOW_BITS as u64).saturating_add(1);
            return Err(ReplayError::TooOld(seq, lo, self.high));
        }

        if seq > self.high {
            // Advance the window: shift the bitset right by (seq - high) bits.
            let shift = (seq - self.high) as usize;
            self.shift(shift);
            self.high = seq;
            self.set_bit(0); // newest position
            return Ok(());
        }

        // seq <= high: check if it's within the window and not already seen.
        let offset = (self.high - seq) as usize;
        debug_assert!(offset < WINDOW_BITS);
        if self.test_bit(offset) {
            return Err(ReplayError::Duplicate(seq));
        }
        self.set_bit(offset);
        Ok(())
    }

    /// Highest seq accepted so far.
    pub fn high(&self) -> u64 {
        self.high
    }

    fn shift(&mut self, n: usize) {
        if n >= WINDOW_BITS {
            self.bits = [0; WORDS];
            return;
        }
        let words = n / 64;
        let bits = n % 64;
        // Shift the bitset *up* (toward older slots, higher word indices).
        // Position 0 is the newest seq; shifting in zeros at position 0 happens
        // implicitly when the caller sets the new high bit.
        if words > 0 {
            for i in (words..WORDS).rev() {
                self.bits[i] = self.bits[i - words];
            }
            for i in 0..words {
                self.bits[i] = 0;
            }
        }
        if bits > 0 {
            let mut carry = 0u64;
            for w in self.bits.iter_mut() {
                let new_carry = *w >> (64 - bits);
                *w = (*w << bits) | carry;
                carry = new_carry;
            }
        }
    }

    fn set_bit(&mut self, offset: usize) {
        debug_assert!(offset < WINDOW_BITS);
        self.bits[offset / 64] |= 1u64 << (offset % 64);
    }

    fn test_bit(&self, offset: usize) -> bool {
        debug_assert!(offset < WINDOW_BITS);
        (self.bits[offset / 64] >> (offset % 64)) & 1 != 0
    }
}

impl Default for Window {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_seq_accepted() {
        let mut w = Window::new();
        assert!(w.check_and_advance(1).is_ok());
        assert_eq!(w.high(), 1);
    }

    #[test]
    fn duplicate_rejected() {
        let mut w = Window::new();
        w.check_and_advance(5).unwrap();
        assert_eq!(
            w.check_and_advance(5).unwrap_err(),
            ReplayError::Duplicate(5)
        );
    }

    #[test]
    fn out_of_order_within_window_accepted() {
        let mut w = Window::new();
        w.check_and_advance(10).unwrap();
        // seqs 1..10 are all still within the window.
        for s in [3, 1, 9, 2, 8].iter() {
            w.check_and_advance(*s).unwrap();
        }
        // duplicates of any of those are rejected
        assert!(matches!(
            w.check_and_advance(3),
            Err(ReplayError::Duplicate(3))
        ));
    }

    #[test]
    fn too_old_rejected_after_advance() {
        let mut w = Window::new();
        w.check_and_advance(1).unwrap();
        w.check_and_advance(2000).unwrap();
        // seq 1 is now well outside the 1024-slot window
        assert!(matches!(w.check_and_advance(1), Err(ReplayError::TooOld(_, _, _))));
    }

    #[test]
    fn boundary_seq_at_window_edge() {
        let mut w = Window::new();
        w.check_and_advance(1024).unwrap();
        // seq 1 is at the oldest edge of the window — accepted once.
        assert!(w.check_and_advance(1).is_ok());
        // seq 0 is below the window's bottom — rejected.
        assert!(matches!(w.check_and_advance(0), Err(ReplayError::TooOld(_, _, _))));
    }

    #[test]
    fn far_future_clears_bitset() {
        let mut w = Window::new();
        w.check_and_advance(1).unwrap();
        w.check_and_advance(1_000_000).unwrap();
        // Recent past relative to new high should still be empty.
        assert!(w.check_and_advance(999_999).is_ok());
        assert!(matches!(
            w.check_and_advance(999_999),
            Err(ReplayError::Duplicate(_))
        ));
    }
}
