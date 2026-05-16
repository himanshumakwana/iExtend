//! Shared types + helpers for the capture backends (`wgc_capture` is the
//! primary path; `dxgi_capture` is retained as legacy / fallback).

#![cfg(windows)]

use std::sync::Arc;

/// One captured frame, converted to YUV420P (I420), with capture timestamp.
///
/// `data` layout: full Y plane (width × height), then U plane
/// (width/2 × height/2), then V plane (width/2 × height/2). 4:2:0 sample
/// positioning matches what OpenH264's I420 input expects.
#[derive(Debug)]
pub struct CapturedFrame {
    pub data: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub pts_us: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("capture init failed: {0:?}")]
    Init(windows::core::Error),
    #[error("AcquireNextFrame timed out")]
    Timeout,
    #[error("AcquireNextFrame failed: {0:?}")]
    Acquire(windows::core::Error),
    #[error("Map failed: {0:?}")]
    Map(windows::core::Error),
    #[error("Channel closed (consumer dropped)")]
    ChannelClosed,
}

/// Convert a BGRA frame at `bgra` (stride = `pitch` bytes) to a planar
/// YUV420P (I420) frame in `yuv`. `yuv` must be sized `w*h*3/2`.
///
/// Uses BT.601 limited-range coefficients — matches what most H.264 software
/// encoders default to. Subsampling is 2x2 box filter (average four BGRAs
/// per chroma sample).
pub(crate) fn bgra_to_yuv420p(bgra: &[u8], pitch: usize, w: usize, h: usize, yuv: &mut [u8]) {
    debug_assert!(yuv.len() >= w * h * 3 / 2);
    let y_plane_size = w * h;
    let uv_w = w / 2;
    let uv_h = h / 2;

    // Y plane — BT.601: Y = 0.257*R + 0.504*G + 0.098*B + 16
    for row in 0..h {
        let src_row = &bgra[row * pitch..row * pitch + w * 4];
        let dst_row = &mut yuv[row * w..row * w + w];
        for col in 0..w {
            let b = src_row[col * 4] as i32;
            let g = src_row[col * 4 + 1] as i32;
            let r = src_row[col * 4 + 2] as i32;
            let y = (66 * r + 129 * g + 25 * b + 128) >> 8;
            dst_row[col] = (y + 16).clamp(0, 255) as u8;
        }
    }

    // U + V planes. 2x2 box filter — average 4 BGRA samples per UV cell.
    // U = -0.148*R - 0.291*G + 0.439*B + 128
    // V = 0.439*R - 0.368*G - 0.071*B + 128
    let (u_plane, v_plane) = yuv[y_plane_size..].split_at_mut(uv_w * uv_h);
    for row in 0..uv_h {
        for col in 0..uv_w {
            let r0 = row * 2;
            let c0 = col * 2;
            let mut r_sum = 0i32;
            let mut g_sum = 0i32;
            let mut b_sum = 0i32;
            for dy in 0..2 {
                let src_row = &bgra[(r0 + dy) * pitch..];
                for dx in 0..2 {
                    let off = (c0 + dx) * 4;
                    b_sum += src_row[off] as i32;
                    g_sum += src_row[off + 1] as i32;
                    r_sum += src_row[off + 2] as i32;
                }
            }
            let r = r_sum >> 2;
            let g = g_sum >> 2;
            let b = b_sum >> 2;
            let u = (-38 * r - 74 * g + 112 * b + 128) >> 8;
            let v = (112 * r - 94 * g - 18 * b + 128) >> 8;
            u_plane[row * uv_w + col] = (u + 128).clamp(0, 255) as u8;
            v_plane[row * uv_w + col] = (v + 128).clamp(0, 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip a synthetic white image through `bgra_to_yuv420p`.
    /// Verifies the conversion doesn't panic and produces sensible Y/U/V
    /// values for a known BGRA input. Pure CPU — no DXGI/D3D11 needed.
    #[test]
    fn bgra_to_yuv_known_inputs() {
        let pitch = 16;
        let mut bgra = vec![0u8; pitch * 4];
        for row in 0..4 {
            for col in 0..4 {
                bgra[row * pitch + col * 4] = 255; // B
                bgra[row * pitch + col * 4 + 1] = 255; // G
                bgra[row * pitch + col * 4 + 2] = 255; // R
                bgra[row * pitch + col * 4 + 3] = 255; // A
            }
        }
        let mut yuv = vec![0u8; 4 * 4 * 3 / 2];
        bgra_to_yuv420p(&bgra, pitch, 4, 4, &mut yuv);

        for y in &yuv[..16] {
            assert!(*y > 220 && *y < 245, "Y = {y} not near 235 for white");
        }
        for u in &yuv[16..20] {
            assert!(*u > 120 && *u < 135, "U = {u} not near 128 for white");
        }
        for v in &yuv[20..24] {
            assert!(*v > 120 && *v < 135, "V = {v} not near 128 for white");
        }
    }
}
