//! Unit tests for `x11::coalesce` — the damage-rectangle merger.
//!
//! These tests do not require a live X11 display.  They exercise the pure
//! algorithmic logic in isolation.

#[cfg(unix)]
use ix_display::DamageRect;
#[cfg(unix)]
use ix_display_linux::x11::coalesce;

#[test]
#[cfg(unix)]
fn overlapping_rects_merge_into_bounding_box() {
    let input = vec![
        DamageRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        },
        DamageRect {
            x: 50,
            y: 50,
            w: 100,
            h: 100,
        },
    ];
    let out = coalesce(input);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0],
        DamageRect {
            x: 0,
            y: 0,
            w: 150,
            h: 150
        }
    );
}

#[test]
#[cfg(unix)]
fn disjoint_rects_kept_separate() {
    let input = vec![
        DamageRect {
            x: 0,
            y: 0,
            w: 50,
            h: 50,
        },
        DamageRect {
            x: 200,
            y: 200,
            w: 50,
            h: 50,
        },
    ];
    let out = coalesce(input.clone());
    assert_eq!(out.len(), 2);
    // Order may differ; sort by x to compare deterministically.
    let mut sorted = out;
    sorted.sort_by_key(|r| r.x);
    assert_eq!(sorted[0], input[0]);
    assert_eq!(sorted[1], input[1]);
}

#[test]
#[cfg(unix)]
fn empty_input_returns_empty() {
    let out = coalesce(vec![]);
    assert!(out.is_empty());
}

#[test]
#[cfg(unix)]
fn single_rect_unchanged() {
    let r = DamageRect {
        x: 10,
        y: 20,
        w: 30,
        h: 40,
    };
    let out = coalesce(vec![r]);
    assert_eq!(out, vec![r]);
}

#[test]
#[cfg(unix)]
fn touching_rects_merge() {
    // Rects that touch (share an edge) also overlap according to
    // DamageRect::overlaps (strict >, so adjacent rects do NOT merge).
    // Verify the documented behaviour: adjacent-but-not-overlapping is kept.
    let a = DamageRect {
        x: 0,
        y: 0,
        w: 50,
        h: 50,
    };
    let b = DamageRect {
        x: 50,
        y: 0,
        w: 50,
        h: 50,
    }; // right edge of a == left edge of b
    let out = coalesce(vec![a, b]);
    // overlaps() uses strict <, so 0 < 100 && 50 < 50 is false — they do NOT overlap.
    // Therefore they are kept separate.
    assert_eq!(out.len(), 2);
}

#[test]
#[cfg(unix)]
fn three_way_chain_merges_all() {
    // A overlaps B, B overlaps C, but A may not directly overlap C.
    // After one pass A+B → AB; AB overlaps C → ABC.
    let a = DamageRect {
        x: 0,
        y: 0,
        w: 60,
        h: 60,
    };
    let b = DamageRect {
        x: 50,
        y: 0,
        w: 60,
        h: 60,
    }; // overlaps a
    let c = DamageRect {
        x: 100,
        y: 0,
        w: 60,
        h: 60,
    }; // overlaps b (and ab)
    let out = coalesce(vec![a, b, c]);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0],
        DamageRect {
            x: 0,
            y: 0,
            w: 160,
            h: 60
        }
    );
}
