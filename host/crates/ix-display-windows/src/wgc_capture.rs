//! Screen capture via **Windows.Graphics.Capture** (WGC).
//!
//! Why this and not DXGI Desktop Duplication: WGC sits above the adapter
//! layer, so the OS handles cross-GPU pixel transfer for us. That makes it
//! work on hybrid-graphics laptops (NVIDIA Optimus + AMD/Intel iGPU) where
//! DXGI `DuplicateOutput` returns `DXGI_ERROR_UNSUPPORTED` because the
//! display is owned by an adapter the dGPU's D3D11 device can't touch.
//!
//! Requires Windows 10 1903 (May 2019 Update) or newer. The crate
//! `ix-display-windows` already requires Windows 10 by virtue of D3D11
//! and IddCx, so this is not a regression.
//!
//! # Threading model
//!
//! `Direct3D11CaptureFramePool::CreateFreeThreaded` delivers `FrameArrived`
//! events on a Windows thread-pool thread — no UI/message-pump needed.
//! Our `capture_loop` sets up the session, then idles on the dedicated
//! capture thread until the consumer drops the receiver; the per-frame
//! work happens entirely inside the FrameArrived closure.
//!
//! Per-frame mutable state (staging texture, scratch buffers, frame
//! counter) lives inside a `Mutex<HandlerState>` since the closure must
//! be `Fn + Send + Sync + 'static`.

#![cfg(windows)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::capture_types::{bgra_to_yuv420p, CaptureError, CapturedFrame};

use windows::core::{IInspectable, Interface};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{HMODULE, POINT};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{MonitorFromPoint, HMONITOR, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

/// Spawn the capture loop on a dedicated OS thread. Returns the channel
/// receiver the encoder consumes from. Capacity 4 is intentional — a deeper
/// queue means deeper latency when the encoder is slow.
pub fn spawn_capture_thread() -> mpsc::Receiver<CapturedFrame> {
    let (tx, rx) = mpsc::channel::<CapturedFrame>(4);
    std::thread::Builder::new()
        .name("ix-wgc-capture".into())
        .spawn(move || {
            if let Err(e) = capture_loop(tx) {
                warn!(err = ?e, "WGC capture loop exited");
            }
        })
        .expect("ix-wgc-capture spawn failed");
    rx
}

/// Holds the WGC resources for the lifetime of capture. Dropping closes
/// the session and pool, which unregisters the FrameArrived handler.
struct WgcSession {
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
    _device: IDirect3DDevice,
    _item: GraphicsCaptureItem,
}

impl Drop for WgcSession {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// Per-frame mutable state, behind a Mutex so the `Fn + Send + Sync`
/// FrameArrived closure can update it.
struct HandlerState {
    /// Staging texture sized to the most recent frame. Re-created if the
    /// frame dimensions change (display resolution change, monitor swap).
    staging: Option<ID3D11Texture2D>,
    bgra_buf: Vec<u8>,
    yuv_buf: Vec<u8>,
    last_w: u32,
    last_h: u32,
    frames_sent: u64,
    started: Instant,
}

fn capture_loop(tx: mpsc::Sender<CapturedFrame>) -> Result<(), CaptureError> {
    let _session = init_wgc(tx.clone())?;
    // Keep the WGC session alive while the consumer is reading. The actual
    // work happens inside the FrameArrived callback on a thread-pool thread.
    while !tx.is_closed() {
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

fn init_wgc(tx: mpsc::Sender<CapturedFrame>) -> Result<WgcSession, CaptureError> {
    unsafe {
        // 1. Create a D3D11 device on any hardware adapter. WGC handles
        //    cross-adapter copy internally, so the choice of adapter here
        //    doesn't have to match the display's owner — that's the whole
        //    point of using WGC over DXGI duplication.
        let feature_levels = [D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0];
        let mut d3d_device: Option<ID3D11Device> = None;
        let mut d3d_context: Option<ID3D11DeviceContext> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut d3d_device),
            None,
            Some(&mut d3d_context),
        )
        .map_err(CaptureError::Init)?;
        let d3d_device =
            d3d_device.ok_or_else(|| CaptureError::Init(windows::core::Error::from_win32()))?;
        let d3d_context =
            d3d_context.ok_or_else(|| CaptureError::Init(windows::core::Error::from_win32()))?;

        // 2. Wrap as IDirect3DDevice — the WinRT-facing handle WGC wants.
        let dxgi_device: IDXGIDevice = d3d_device.cast().map_err(CaptureError::Init)?;
        let inspectable: IInspectable =
            CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device).map_err(CaptureError::Init)?;
        let direct3d_device: IDirect3DDevice = inspectable.cast().map_err(CaptureError::Init)?;

        // 3. Identify the primary monitor and build a GraphicsCaptureItem.
        let hmonitor: HMONITOR = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        if hmonitor.0.is_null() {
            return Err(CaptureError::Init(windows::core::Error::from_win32()));
        }
        let interop: IGraphicsCaptureItemInterop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
                .map_err(CaptureError::Init)?;
        let item: GraphicsCaptureItem = interop
            .CreateForMonitor(hmonitor)
            .map_err(CaptureError::Init)?;
        let size = item.Size().map_err(CaptureError::Init)?;
        info!(
            width = size.Width,
            height = size.Height,
            "WGC capture acquired (Windows.Graphics.Capture)"
        );

        // 4. Build the frame pool. CreateFreeThreaded → callbacks fire on
        //    the Windows thread pool, no message-pump needed.
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &direct3d_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            size,
        )
        .map_err(CaptureError::Init)?;

        // 5. Per-frame state lives behind a Mutex so the Fn closure can
        //    mutate it without unsafe interior tricks.
        let state = Arc::new(Mutex::new(HandlerState {
            staging: None,
            bgra_buf: Vec::new(),
            yuv_buf: Vec::new(),
            last_w: 0,
            last_h: 0,
            frames_sent: 0,
            started: Instant::now(),
        }));

        // 6. Wire FrameArrived → convert + send.
        let handler_device = d3d_device.clone();
        let handler_context = d3d_context.clone();
        let handler_tx = tx.clone();
        let handler_state = state.clone();
        let handler = TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(
            move |pool, _args| {
                let pool = match pool.as_ref() {
                    Some(p) => p,
                    None => return Ok(()),
                };
                if handler_tx.is_closed() {
                    return Ok(());
                }
                if let Err(e) = handle_frame(
                    pool,
                    &handler_device,
                    &handler_context,
                    &handler_state,
                    &handler_tx,
                ) {
                    warn!(err = %e, "WGC frame handler error");
                }
                Ok(())
            },
        );
        pool.FrameArrived(&handler).map_err(CaptureError::Init)?;

        // 7. Create the session and start capture. Suppress the yellow
        //    "being captured" border when running on Windows 11 22H2+.
        //    Older Windows versions ignore the call.
        let session = pool
            .CreateCaptureSession(&item)
            .map_err(CaptureError::Init)?;
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture().map_err(CaptureError::Init)?;

        Ok(WgcSession {
            session,
            pool,
            _device: direct3d_device,
            _item: item,
        })
    }
}

/// One frame: pull from the pool, copy through a staging texture, convert
/// BGRA→YUV420P, and try-send on the consumer channel.
fn handle_frame(
    pool: &Direct3D11CaptureFramePool,
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    state: &Mutex<HandlerState>,
    tx: &mpsc::Sender<CapturedFrame>,
) -> windows::core::Result<()> {
    let frame = pool.TryGetNextFrame()?;
    let surface = frame.Surface()?;
    let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
    // SAFETY: GetInterface is a COM QueryInterface — texture is valid as
    // long as `surface` (and hence `frame`) is alive.
    let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };

    let desc = unsafe {
        let mut d = std::mem::zeroed::<D3D11_TEXTURE2D_DESC>();
        texture.GetDesc(&mut d);
        d
    };
    let w = desc.Width;
    let h = desc.Height;

    let mut st = match state.lock() {
        Ok(s) => s,
        Err(poisoned) => poisoned.into_inner(),
    };

    if st.staging.is_none() || w != st.last_w || h != st.last_h {
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut tex: Option<ID3D11Texture2D> = None;
        unsafe {
            device.CreateTexture2D(&staging_desc, None, Some(&mut tex))?;
        }
        st.staging = Some(tex.ok_or_else(windows::core::Error::from_win32)?);
        let pitch = (w * 4) as usize;
        st.bgra_buf = vec![0u8; pitch * h as usize];
        st.yuv_buf = vec![0u8; (w * h * 3 / 2) as usize];
        st.last_w = w;
        st.last_h = h;
    }
    // Clone the COM pointer so we can release the immutable borrow of `st`
    // before we touch `st.bgra_buf` mutably below. `.clone()` on a windows
    // crate interface is a refcount bump, not a texture copy.
    let staging_tex = st
        .staging
        .as_ref()
        .expect("staging texture initialized above")
        .clone();

    let pitch = (w * 4) as usize;
    unsafe {
        context.CopyResource(&staging_tex, &texture);
        let mut mapped = std::mem::zeroed();
        context.Map(&staging_tex, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let src = mapped.pData as *const u8;
        let row_pitch = mapped.RowPitch as usize;
        let dst_ptr = st.bgra_buf.as_mut_ptr();
        for row in 0..h as usize {
            let src_row = src.add(row * row_pitch);
            let dst_row = dst_ptr.add(row * pitch);
            std::ptr::copy_nonoverlapping(src_row, dst_row, pitch);
        }
        context.Unmap(&staging_tex, 0);
    }

    // Borrow-split: convert into yuv_buf while reading bgra_buf.
    let st_mut = &mut *st;
    bgra_to_yuv420p(
        &st_mut.bgra_buf,
        pitch,
        w as usize,
        h as usize,
        &mut st_mut.yuv_buf,
    );

    let pts_us = st.started.elapsed().as_micros() as u64;
    let captured = CapturedFrame {
        data: Arc::new(st.yuv_buf.clone()),
        width: w,
        height: h,
        pts_us,
    };
    match tx.try_send(captured) {
        Ok(()) => {
            st.frames_sent += 1;
            if st.frames_sent.is_multiple_of(60) {
                let elapsed = st.started.elapsed().as_secs_f64();
                info!(
                    frames = st.frames_sent,
                    avg_fps = st.frames_sent as f64 / elapsed,
                    "WGC capture progress"
                );
            }
        }
        Err(mpsc::error::TrySendError::Full(_)) => {
            if st.frames_sent.is_multiple_of(30) {
                warn!("encoder backpressure — dropping frame");
            }
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            // Consumer gone; the outer capture_loop will notice on its next
            // poll and unwind. Nothing to do here.
        }
    }

    Ok(())
}
