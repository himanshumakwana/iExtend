// cursor_protocol.rs — per-frame cursor messages emitted on the control
// DataChannel and encoder cursor-overlay hint plumbing.
//
// Plan 8 Task 10.  This module re-exports the shared `ControlMsg` type from
// `ix-input` and adds the glue that wires it into the iextendd capture loop.
//
// Design:
//   - `CursorEmitter` samples the OS cursor once per capture tick.
//   - When position or sprite changes it serialises a `ControlMsg::Cursor`
//     JSON string and hands it to the control DataChannel sender.
//   - The encoder receives a `CursorOverlayHint` telling it to composite a
//     2-colour (magenta/cyan) marker at (x, y) so the iPad can mask it out
//     post-decode and render its own reprojected cursor with ~2 ms latency.
//
// Spec §8.2 cursor-mask signature:
//   Core:   fully-saturated magenta #FF00FF (R=255, G=0, B=255)
//   Border: 1-pixel cyan border #00FFFF (R=0, G=255, B=255)
//
// The iPad's `CursorMaskShader.metal` recognises this 2-colour pattern and
// replaces those pixels with the surrounding background median, then re-draws
// the cursor at the reprojected position.

pub use ix_input::cursor_protocol::{ControlMsg, CursorEmitter};

/// Hint passed from the capture loop to the encoder each frame.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CursorOverlayHint {
    /// Cursor sprite identifier (0 = default arrow).
    pub sprite_id: u32,
    /// Cursor hotspot X in display pixels.
    pub x: f32,
    /// Cursor hotspot Y in display pixels.
    pub y: f32,
    /// True if the iPad has signalled it wants to render its own cursor and
    /// the host encoder should suppress its normal cursor rendering.
    pub host_cursor_suppressed: bool,
}

impl Default for CursorOverlayHint {
    fn default() -> Self {
        Self {
            sprite_id: 0,
            x: 0.0,
            y: 0.0,
            host_cursor_suppressed: false,
        }
    }
}

/// Serialise a [`ControlMsg`] to a UTF-8 JSON string suitable for sending over
/// the control DataChannel.
#[allow(dead_code)]
///
/// Returns `None` if serde serialisation fails (should never happen with our
/// well-formed types, but we prefer a soft failure over a panic in the hot path).
pub fn encode_control_msg(msg: &ControlMsg) -> Option<String> {
    serde_json::to_string(msg).ok()
}

/// Parse a control message received from the iPad (e.g. `SetHostCursorVisible`).
#[allow(dead_code)]
pub fn decode_control_msg(json: &str) -> Option<ControlMsg> {
    serde_json::from_str(json).ok()
}

// ── Integration helpers for session.rs ───────────────────────────────────────

/// Call once per capture tick inside the session capture loop.
#[allow(dead_code)]
///
/// Returns a JSON string if the cursor changed, plus an updated
/// [`CursorOverlayHint`] for the encoder.  Both may be `None` if nothing
/// changed since the last tick.
///
/// Usage in `session.rs`:
/// ```ignore
/// let (json_opt, hint_opt) = cursor_protocol::tick(
///     &mut cursor_emitter,
///     &cursor_overlay_state,
/// );
/// if let Some(json) = json_opt {
///     control_channel.send(json).ok();
/// }
/// if let Some(hint) = hint_opt {
///     encoder.set_cursor_overlay(hint.sprite_id, hint.x, hint.y);
/// }
/// ```
pub fn tick(
    emitter: &mut CursorEmitter,
    suppressed: bool,
) -> (Option<String>, Option<CursorOverlayHint>) {
    let msg_opt = emitter.tick();
    let (json_opt, hint_opt) = match &msg_opt {
        Some(ControlMsg::Cursor {
            x, y, sprite_id, ..
        }) => {
            let hint = CursorOverlayHint {
                sprite_id: *sprite_id,
                x: *x,
                y: *y,
                host_cursor_suppressed: suppressed,
            };
            (encode_control_msg(msg_opt.as_ref().unwrap()), Some(hint))
        }
        Some(_) => (encode_control_msg(msg_opt.as_ref().unwrap()), None),
        None => (None, None),
    };
    (json_opt, hint_opt)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip_cursor() {
        let msg = ControlMsg::Cursor {
            x: 42.0,
            y: 99.5,
            sprite_id: 3,
            hotspot_x: 1.0,
            hotspot_y: 1.0,
            ts_us: 12_345_678,
        };
        let json = encode_control_msg(&msg).unwrap();
        let back = decode_control_msg(&json).unwrap();
        match back {
            ControlMsg::Cursor {
                x,
                y,
                sprite_id,
                ts_us,
                ..
            } => {
                assert_eq!(x, 42.0);
                assert_eq!(y, 99.5);
                assert_eq!(sprite_id, 3);
                assert_eq!(ts_us, 12_345_678);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn encode_decode_set_host_cursor_visible() {
        let msg = ControlMsg::SetHostCursorVisible { tip: false };
        let json = encode_control_msg(&msg).unwrap();
        let back = decode_control_msg(&json).unwrap();
        match back {
            ControlMsg::SetHostCursorVisible { tip } => assert!(!tip),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tick_produces_json_on_first_call() {
        let mut emitter = CursorEmitter::new();
        let (json, hint) = tick(&mut emitter, false);
        assert!(json.is_some(), "first tick should produce a message");
        assert!(hint.is_some(), "first tick should produce a hint");
    }

    #[test]
    fn cursor_overlay_hint_default() {
        let h = CursorOverlayHint::default();
        assert_eq!(h.sprite_id, 0);
        assert!(!h.host_cursor_suppressed);
    }
}
