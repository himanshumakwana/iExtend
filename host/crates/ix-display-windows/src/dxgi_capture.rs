//! DXGI Desktop Duplication capture path (Plan A — Mirror mode).
//!
//! This is the simple, no-driver path: we ask Windows for a duplication of
//! the current primary display via `IDXGIOutputDuplication`, copy each
//! frame's BGRA pixels off the GPU, convert to YUV420P, and push to a
//! tokio mpsc channel for the encoder to consume.
//!
//! Why a separate module from `frame_pump` / `inverted_call`: that scaffold
//! targets the IddCx kernel-driver path (Plan B — Extend mode), which
//! requires a signed driver and delivers frames the user has explicitly
//! routed to a virtual monitor. DXGI Duplication runs without any driver
//! and captures the existing primary display — perfect for Mirror mode and
//! the first-pixel milestone.
//!
//! # Threading model
//!
//! `capture_loop` is intended to run on its own dedicated `std::thread`
//! (spawned by `iextendd::session`). It acquires/releases frames on that
//! thread; the only cross-thread surface is the `mpsc::Sender<CapturedFrame>`
//! the encoder side receives on.
//!
//! # Performance
//!
//! BGRA → YUV420P conversion is done in a tight scalar loop. For a 1080p
//! frame (~8 MB BGRA → 3 MB YUV) on a modern CPU, this is ~3-5 ms — plenty
//! fast for 30 fps. If we hit a frame budget on smaller cores we can swap
//! to libyuv-rs without changing the public surface.

#![cfg(windows)]

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{info, warn};

use windows::core::Interface;
use windows::Win32::Foundation::{HANDLE, HMODULE};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1,
    D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BIND_FLAG,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ, D3D11_RESOURCE_MISC_FLAG,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput, IDXGIOutput1,
    IDXGIOutputDuplication, IDXGIResource, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
    DXGI_ERROR_UNSUPPORTED, DXGI_OUTDUPL_FRAME_INFO,
};

/// One BGRA frame, converted to YUV420P, with capture timestamp.
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
    #[error("DXGI/D3D11 init failed: {0:?}")]
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

/// Acquire `IDXGIOutputDuplication`, walking every `(adapter, output)` pair
/// until one combination accepts `DuplicateOutput`.
///
/// Desktop Duplication requires the D3D11 device to live on the same adapter
/// as the output. On hybrid-graphics laptops (e.g. AMD iGPU + NVIDIA dGPU)
/// `D3D11CreateDevice(NULL, HARDWARE, …)` lands on the discrete GPU while
/// the display is owned by the integrated adapter — the mismatch surfaces
/// as `DXGI_ERROR_UNSUPPORTED` from `DuplicateOutput`. To avoid that we pin
/// the D3D11 device to each candidate adapter explicitly.
///
/// Returns the D3D11 device + context (held so they outlive the duplication
/// interface) and the duplication object itself.
fn init_duplication() -> Result<
    (
        ID3D11Device,
        ID3D11DeviceContext,
        IDXGIOutputDuplication,
        u32,
        u32,
    ),
    CaptureError,
> {
    // Pinned to ≥ 10.0 because Desktop Duplication is unavailable on 9.x.
    let feature_levels = [
        D3D_FEATURE_LEVEL_11_1,
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0,
    ];

    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(CaptureError::Init)?;

        let mut last_err: Option<windows::core::Error> = None;
        let mut adapter_idx = 0u32;
        loop {
            let adapter1: IDXGIAdapter1 = match factory.EnumAdapters1(adapter_idx) {
                Ok(a) => a,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => {
                    last_err = Some(e);
                    break;
                }
            };
            adapter_idx += 1;

            let desc = adapter1.GetDesc1().map_err(CaptureError::Init)?;
            // Skip the Microsoft Basic Render Driver / WARP — desktop
            // duplication is not supported on software adapters.
            if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                continue;
            }

            let adapter_name: String = String::from_utf16_lossy(
                &desc
                    .Description
                    .iter()
                    .take_while(|&&c| c != 0)
                    .copied()
                    .collect::<Vec<u16>>(),
            );
            let adapter: IDXGIAdapter = adapter1.cast().map_err(CaptureError::Init)?;

            let mut output_idx = 0u32;
            loop {
                let output: IDXGIOutput = match adapter.EnumOutputs(output_idx) {
                    Ok(o) => o,
                    Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(e) => {
                        last_err = Some(e);
                        break;
                    }
                };
                output_idx += 1;

                let output1: IDXGIOutput1 = match output.cast() {
                    Ok(o) => o,
                    Err(e) => {
                        last_err = Some(e);
                        continue;
                    }
                };

                // Pin the device to *this* adapter. When pAdapter is non-null
                // the driver type MUST be UNKNOWN, else D3D11CreateDevice
                // returns E_INVALIDARG.
                let mut device: Option<ID3D11Device> = None;
                let mut context: Option<ID3D11DeviceContext> = None;
                if let Err(e) = D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_FLAG(0),
                    Some(&feature_levels),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                ) {
                    last_err = Some(e);
                    continue;
                }
                let (device, context) = match (device, context) {
                    (Some(d), Some(c)) => (d, c),
                    _ => continue,
                };

                match output1.DuplicateOutput(&device) {
                    Ok(dup) => {
                        let dup_desc = dup.GetDesc();
                        let width = dup_desc.ModeDesc.Width;
                        let height = dup_desc.ModeDesc.Height;
                        info!(
                            adapter = %adapter_name,
                            adapter_idx = adapter_idx - 1,
                            output_idx = output_idx - 1,
                            width,
                            height,
                            format = ?dup_desc.ModeDesc.Format,
                            "DXGI duplication acquired"
                        );
                        return Ok((device, context, dup, width, height));
                    }
                    Err(e) => {
                        warn!(
                            adapter = %adapter_name,
                            output_idx = output_idx - 1,
                            err = %e,
                            "DuplicateOutput rejected this adapter/output pair; trying next"
                        );
                        last_err = Some(e);
                        continue;
                    }
                }
            }
        }

        Err(CaptureError::Init(last_err.unwrap_or_else(|| {
            windows::core::Error::new(
                DXGI_ERROR_UNSUPPORTED,
                "no DXGI adapter/output pair supported Desktop Duplication",
            )
        })))
    }
}

/// Allocate a staging texture sized to the source. CPU_READ usage so we can
/// `Map` it after `CopyResource`s the duplicated frame in.
fn create_staging_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> Result<ID3D11Texture2D, CaptureError> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: D3D11_BIND_FLAG(0).0 as u32,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: D3D11_RESOURCE_MISC_FLAG(0).0 as u32,
    };
    let mut tex: Option<ID3D11Texture2D> = None;
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut tex))
            .map_err(CaptureError::Init)?;
    }
    tex.ok_or_else(|| CaptureError::Init(windows::core::Error::from_win32()))
}

/// Convert a BGRA frame at `bgra` (stride = `pitch` bytes) to a planar
/// YUV420P (I420) frame in `yuv`. `yuv` must be sized `w*h*3/2`.
///
/// Uses BT.601 limited-range coefficients — matches what most H.264 software
/// encoders default to. Subsampling is 2x2 box filter (average four BGRAs
/// per chroma sample).
fn bgra_to_yuv420p(bgra: &[u8], pitch: usize, w: usize, h: usize, yuv: &mut [u8]) {
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

/// Run the capture loop forever, sending each captured frame on `tx`.
///
/// Exits cleanly when `tx` is dropped (consumer gone) or on a fatal error
/// (returns `Err`). Recoverable errors (per-frame timeouts) are warned and
/// the loop continues.
pub fn capture_loop(tx: mpsc::Sender<CapturedFrame>) -> Result<(), CaptureError> {
    let (device, context, dup, width, height) = init_duplication()?;
    let staging = create_staging_texture(&device, width, height)?;

    let pitch = (width * 4) as usize;
    let yuv_size = (width * height * 3 / 2) as usize;
    let mut yuv_buf = vec![0u8; yuv_size];
    let mut bgra_buf = vec![0u8; pitch * height as usize];

    let started = Instant::now();
    let mut frames_sent = 0u64;

    loop {
        if tx.is_closed() {
            return Err(CaptureError::ChannelClosed);
        }

        let mut frame_info: DXGI_OUTDUPL_FRAME_INFO = unsafe { std::mem::zeroed() };
        let mut resource: Option<IDXGIResource> = None;

        let acquire_result = unsafe { dup.AcquireNextFrame(100, &mut frame_info, &mut resource) };

        match acquire_result {
            Ok(()) => {}
            Err(e) if e.code() == windows::Win32::Graphics::Dxgi::DXGI_ERROR_WAIT_TIMEOUT => {
                continue;
            }
            Err(e) => {
                warn!(err = %e, "AcquireNextFrame failed; will reinit");
                return Err(CaptureError::Acquire(e));
            }
        }

        // Got a frame — copy from the duplicated resource into our staging
        // texture so we can read it on the CPU.
        let resource = match resource {
            Some(r) => r,
            None => {
                unsafe {
                    let _ = dup.ReleaseFrame();
                }
                continue;
            }
        };

        let texture: ID3D11Texture2D = match resource.cast() {
            Ok(t) => t,
            Err(e) => {
                warn!(err = %e, "could not cast resource to ID3D11Texture2D");
                unsafe {
                    let _ = dup.ReleaseFrame();
                }
                continue;
            }
        };

        unsafe {
            context.CopyResource(&staging, &texture);
        }

        // Map the staging texture to read pixels.
        let mapped = unsafe {
            let mut m = std::mem::zeroed();
            match context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut m)) {
                Ok(()) => m,
                Err(e) => {
                    let _ = dup.ReleaseFrame();
                    return Err(CaptureError::Map(e));
                }
            }
        };

        let row_pitch = mapped.RowPitch as usize;
        unsafe {
            let src = mapped.pData as *const u8;
            for row in 0..height as usize {
                let src_row = src.add(row * row_pitch);
                let dst_row = bgra_buf.as_mut_ptr().add(row * pitch);
                std::ptr::copy_nonoverlapping(src_row, dst_row, pitch);
            }
            context.Unmap(&staging, 0);
            let _ = dup.ReleaseFrame();
        }

        // Convert BGRA → YUV420P. This is the bottleneck on small CPUs;
        // libyuv would be ~3x faster but requires another link dep.
        bgra_to_yuv420p(
            &bgra_buf,
            pitch,
            width as usize,
            height as usize,
            &mut yuv_buf,
        );

        let pts_us = started.elapsed().as_micros() as u64;
        let frame = CapturedFrame {
            data: Arc::new(yuv_buf.clone()),
            width,
            height,
            pts_us,
        };

        match tx.try_send(frame) {
            Ok(()) => {
                frames_sent += 1;
                if frames_sent.is_multiple_of(60) {
                    let elapsed = started.elapsed().as_secs_f64();
                    info!(
                        frames = frames_sent,
                        avg_fps = frames_sent as f64 / elapsed,
                        "DXGI capture progress"
                    );
                }
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                // Encoder behind — drop the frame rather than block. We log
                // sparsely (every 30 dropped frames) to avoid log spam.
                if frames_sent.is_multiple_of(30) {
                    warn!("encoder backpressure — dropping frame");
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                return Err(CaptureError::ChannelClosed);
            }
        }
    }
}

/// Spawn the capture loop on a dedicated OS thread. Returns the channel
/// receiver the encoder consumes from. Capacity 4 is intentional — a deeper
/// queue means deeper latency when the encoder is slow.
pub fn spawn_capture_thread() -> mpsc::Receiver<CapturedFrame> {
    let (tx, rx) = mpsc::channel::<CapturedFrame>(4);
    std::thread::Builder::new()
        .name("ix-dxgi-capture".into())
        .spawn(move || {
            if let Err(e) = capture_loop(tx) {
                warn!(err = ?e, "DXGI capture loop exited");
            }
        })
        .expect("ix-dxgi-capture spawn failed");
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip a synthetic gradient through bgra_to_yuv420p. Validates
    /// the conversion doesn't panic and produces sensible Y values for a
    /// known BGRA input. Doesn't need any DXGI/D3D11 — pure CPU function.
    #[test]
    fn bgra_to_yuv_known_inputs() {
        // 4x4 white image, BGRA stride = 16 bytes per row.
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

        // White → Y ≈ 235 (limited-range), U ≈ 128, V ≈ 128.
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
