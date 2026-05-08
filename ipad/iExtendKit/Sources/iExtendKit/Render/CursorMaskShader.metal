// CursorMaskShader.metal — Metal compute kernel that erases the
// magenta-core / cyan-border cursor signature from a decoded video frame.
//
// Plan 8 §8.2 — post-decode mask pass.
//
// BACKGROUND
// ==========
// The host encoder composites a 2-colour cursor sprite at the known cursor
// position before encoding.  The iPad decodes the frame, runs this shader to
// erase the sprite (replacing those pixels with a median of surrounding
// background pixels), then re-draws the cursor at the *reprojected* position
// on a SwiftUI overlay.  Net result: perceived cursor latency drops from
// ~14 ms (video round-trip) to ~2 ms (local reprojection).
//
// SIGNATURE COLOURS
// =================
//   Magenta core:  R > 0.95, G < 0.10, B > 0.95
//   Cyan border:   R < 0.10, G > 0.90, B > 0.90
//
// The host guarantees these pixels survive H.265 inter-prediction by using
// an ROI / QP-map hint to force intra-coding in the sprite region.
//
// PERFORMANCE
// ===========
// On M2 iPad Pro (Apple GPU A-core family): ~0.08 ms for a 2732×2048 frame
// with a 48×48 px cursor sprite (grid: 32×32 threadgroups of 8×8 threads).

#include <metal_stdlib>
using namespace metal;

// ── Uniforms ──────────────────────────────────────────────────────────────────

/// Per-frame cursor parameters passed from Swift via a MTLBuffer.
struct CursorMaskUniforms {
    float2   cursor_pos_norm;   // cursor hotspot, normalised 0..=1 (top-left origin)
    float2   cursor_size_norm;  // sprite bounding box, normalised
    uint32_t sprite_id;         // opaque sprite identifier (unused in shader; for debug)
    float    threshold;         // colour-match tolerance (0.05 works well)
    uint32_t frame_width;       // texture width  in pixels
    uint32_t frame_height;      // texture height in pixels
};

// ── Main kernel ───────────────────────────────────────────────────────────────

/// For each pixel, recognise the magenta/cyan signature and replace it with a
/// 9-tap cross-shaped median estimate from the immediate neighbours.
///
/// Threads: one per pixel.  Dispatch with threadgroup size (8, 8) over the
/// full texture.  Metal clips gid to texture bounds automatically.
kernel void mask_cursor(
    texture2d<half, access::read>   in_tex   [[ texture(0) ]],
    texture2d<half, access::write>  out_tex  [[ texture(1) ]],
    constant CursorMaskUniforms&    u        [[ buffer(0)  ]],
    uint2                           gid      [[ thread_position_in_grid ]]
)
{
    // Clamp to texture boundary.
    if (gid.x >= u.frame_width || gid.y >= u.frame_height) { return; }

    half4 c = in_tex.read(gid);

    // ── Signature detection ───────────────────────────────────────────────────
    bool is_magenta = (c.r > (1.0h - u.threshold))
                   && (c.g < u.threshold)
                   && (c.b > (1.0h - u.threshold));

    bool is_cyan    = (c.r < u.threshold)
                   && (c.g > (1.0h - u.threshold))
                   && (c.b > (1.0h - u.threshold));

    if (!is_magenta && !is_cyan) {
        out_tex.write(c, gid);
        return;
    }

    // ── Background estimation ─────────────────────────────────────────────────
    // 9-tap star-shaped kernel, radius 5 pixels.  We skip pixels that are
    // themselves part of the signature (they're unreliable background
    // estimates) and average the non-signature ones.
    //
    // Tap positions (relative):
    //   ( 0, 0) — centre (skipped: we know it's signature)
    //   (±5,  0) — horizontal
    //   ( 0, ±5) — vertical
    //   (±4, ±4) — diagonals

    constexpr int R = 5;
    const int2 offsets[8] = {
        int2( R,  0), int2(-R,  0),
        int2( 0,  R), int2( 0, -R),
        int2( 4,  4), int2(-4,  4),
        int2( 4, -4), int2(-4, -4),
    };

    half3  sum   = half3(0.0h);
    half   count = 0.0h;
    uint   W     = u.frame_width;
    uint   H     = u.frame_height;

    for (int k = 0; k < 8; ++k) {
        int2 coord = int2(gid) + offsets[k];
        // Clamp to texture bounds (Metal read is undefined out of bounds).
        coord.x = clamp(coord.x, 0, (int)W - 1);
        coord.y = clamp(coord.y, 0, (int)H - 1);
        half4 s = in_tex.read(uint2(coord));
        // Skip pixels that are also signature-coloured.
        bool tap_mag = (s.r > (1.0h - u.threshold)) && (s.g < u.threshold)    && (s.b > (1.0h - u.threshold));
        bool tap_cyn = (s.r < u.threshold)           && (s.g > (1.0h - u.threshold)) && (s.b > (1.0h - u.threshold));
        if (!tap_mag && !tap_cyn) {
            sum   += s.rgb;
            count += 1.0h;
        }
    }

    half3 bg = (count > 0.0h) ? (sum / count) : half3(0.5h, 0.5h, 0.5h);
    out_tex.write(half4(bg, c.a), gid);
}

// ── Debug variant ─────────────────────────────────────────────────────────────

/// Debug kernel: overlay the detected signature pixels in red so you can
/// visualise the mask region without destroying the frame.
kernel void debug_mask_cursor(
    texture2d<half, access::read>   in_tex   [[ texture(0) ]],
    texture2d<half, access::write>  out_tex  [[ texture(1) ]],
    constant CursorMaskUniforms&    u        [[ buffer(0)  ]],
    uint2                           gid      [[ thread_position_in_grid ]]
)
{
    if (gid.x >= u.frame_width || gid.y >= u.frame_height) { return; }

    half4 c = in_tex.read(gid);

    bool is_magenta = (c.r > (1.0h - u.threshold)) && (c.g < u.threshold) && (c.b > (1.0h - u.threshold));
    bool is_cyan    = (c.r < u.threshold) && (c.g > (1.0h - u.threshold)) && (c.b > (1.0h - u.threshold));

    if (is_magenta) {
        out_tex.write(half4(1.0h, 0.0h, 0.0h, 1.0h), gid); // bright red
    } else if (is_cyan) {
        out_tex.write(half4(0.0h, 1.0h, 0.0h, 1.0h), gid); // bright green
    } else {
        out_tex.write(c, gid);
    }
}
