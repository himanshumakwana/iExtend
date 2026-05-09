// cursor_protocol.rs — per-frame cursor position message emitted on the
// control DataChannel by the host, and consumed by the iPad for reprojection.
//
// Spec §8.2 / Plan 8 Task 10.
//
// The host samples the OS cursor position once per capture tick (~8 ms at
// 120 Hz) and emits a `ControlMsg::Cursor` JSON message only when the
// position or sprite changed.  The iPad receives these messages and feeds
// them into `Reproject.predict()` alongside its own local input history to
// arrive at a ~2 ms perceived latency cursor position.
//
// Encoder cursor-overlay hint: `set_cursor_overlay()` tells the encoder to
// composite a magenta-core / cyan-border sprite at (x, y) during the next
// encode.  The iPad's `CursorMaskShader.metal` recognises this 2-colour
// signature and replaces those pixels with the surrounding background,
// then re-draws the cursor itself at the reprojected position.

use serde::{Deserialize, Serialize};

/// Messages sent over the control DataChannel from host → iPad.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMsg {
    /// Per-frame cursor position update.
    Cursor {
        /// Host-display pixel X.
        x: f32,
        /// Host-display pixel Y.
        y: f32,
        /// Opaque sprite identifier — changes when the cursor shape changes.
        sprite_id: u32,
        /// Hotspot X within the sprite image (pixels from sprite top-left).
        hotspot_x: f32,
        /// Hotspot Y within the sprite image.
        hotspot_y: f32,
        /// iPad mach_absolute_time equivalent, µs.  Used to correlate cursor
        /// timestamps with input sample timestamps for RTT estimation.
        ts_us: u64,
    },
    /// Tell the iPad whether the host-rendered cursor tip should be visible.
    /// iPad sends `{ tip: false }` when the live-screen is front-most so it
    /// can draw its own reprojected tip at zero latency.
    SetHostCursorVisible { tip: bool },
}

/// Samples the OS cursor and emits [`ControlMsg::Cursor`] when changed.
///
/// Call [`CursorEmitter::tick`] once per capture frame.  Returns `None` when
/// nothing changed (same position and same sprite).
pub struct CursorEmitter {
    last_pos: (f32, f32),
    last_sprite: u32,
    seq: u32,
}

impl Default for CursorEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorEmitter {
    pub fn new() -> Self {
        Self {
            last_pos: (-1.0, -1.0),
            last_sprite: u32::MAX,
            seq: 0,
        }
    }

    /// Sample the OS cursor; return a [`ControlMsg::Cursor`] if position or
    /// sprite changed.  Cheap — call every capture tick.
    pub fn tick(&mut self) -> Option<ControlMsg> {
        let (x, y) = sample_cursor_pos();
        let sprite = current_sprite_id();
        let ts = now_micros();

        if (x, y) != self.last_pos || sprite != self.last_sprite {
            self.last_pos = (x, y);
            self.last_sprite = sprite;
            self.seq += 1;
            Some(ControlMsg::Cursor {
                x,
                y,
                sprite_id: sprite,
                hotspot_x: 0.0,
                hotspot_y: 0.0,
                ts_us: ts,
            })
        } else {
            None
        }
    }
}

// ── Platform cursor sampling ──────────────────────────────────────────────────

/// Sample the current OS cursor position in display pixels.
/// Real implementations are platform-specific; the stubs return (0, 0) on
/// unsupported platforms so the module compiles everywhere (including CI Linux
/// where there is no display server).
#[cfg(windows)]
fn sample_cursor_pos() -> (f32, f32) {
    // Stub. Production impl will use windows::Win32::UI::WindowsAndMessaging::GetCursorPos.
    // POINT impls Default in windows = 0.58, so the call site won't need MaybeUninit.
    (0.0, 0.0)
}

#[cfg(target_os = "linux")]
fn sample_cursor_pos() -> (f32, f32) {
    // Wayland: ext-image-capture-source-v1 gives us cursor metadata per frame.
    // X11: XQueryPointer.
    // For now return a stable (0, 0) so CI compiles; Plan 9 wires real cursor
    // tracking once the display driver is integrated.
    (0.0, 0.0)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn sample_cursor_pos() -> (f32, f32) {
    (0.0, 0.0)
}

/// Return an opaque identifier for the current cursor sprite (shape).
/// Changes when the OS changes the active cursor (e.g. arrow → I-beam).
fn current_sprite_id() -> u32 {
    // Platform-specific in production.  Stub returns 0.
    0
}

/// Wall-clock time in microseconds.
fn now_micros() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_msg_serialises_to_json() {
        let msg = ControlMsg::Cursor {
            x: 100.0,
            y: 200.0,
            sprite_id: 1,
            hotspot_x: 0.0,
            hotspot_y: 0.0,
            ts_us: 42_000,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(
            json.contains("\"type\":\"cursor\""),
            "type field present: {json}"
        );
        assert!(json.contains("\"x\":100.0"), "x present: {json}");
    }

    #[test]
    fn set_host_cursor_visible_roundtrips() {
        let msg = ControlMsg::SetHostCursorVisible { tip: false };
        let json = serde_json::to_string(&msg).unwrap();
        let back: ControlMsg = serde_json::from_str(&json).unwrap();
        match back {
            ControlMsg::SetHostCursorVisible { tip } => assert!(!tip),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn emitter_emits_on_first_tick() {
        let mut e = CursorEmitter::new();
        // The stub sample_cursor_pos returns (0, 0) but last_pos starts at
        // (-1, -1) so the first tick always produces a message.
        let msg = e.tick();
        assert!(msg.is_some(), "first tick should emit a cursor message");
    }

    #[test]
    fn emitter_suppresses_duplicate_positions() {
        let mut e = CursorEmitter::new();
        e.tick(); // first tick — emits
                  // Second tick with same underlying cursor position (stub always 0,0)
                  // → should suppress.
        let msg = e.tick();
        assert!(
            msg.is_none(),
            "duplicate position should produce no message"
        );
    }
}
