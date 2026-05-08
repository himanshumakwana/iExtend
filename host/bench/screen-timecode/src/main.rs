//! Screen-timecode reference signal for iExtend bench camera rig.
//!
//! # Purpose
//!
//! Paints a 7-segment decimal timecode (microseconds since process start,
//! `mod 10_000_000` so it fits in 7 digits) at the top-left corner of the
//! primary display. The repaint is locked to vsync via the platform's
//! frame-statistics API so there is no tearing artefact visible to the camera.
//!
//! A 32-bit binary stripe is also painted along the very top pixel row of the
//! window as a redundant fallback channel: 32 equal-width blocks, white = 1 /
//! black = 0, encoding the low 32 bits of the microsecond timestamp. The
//! `measure_p2p_latency.py` script tries OCR on the digit strip first; if that
//! fails (glare, motion blur) it falls back to the binary stripe.
//!
//! # Measurement principle
//!
//! The bench camera (e.g. iPhone 15 Pro at 240 fps) captures both the host
//! monitor and the iPad screen in the same frame. The host monitor shows the
//! timecode directly. The iPad shows the same timecode but delayed by the full
//! iExtend pipeline latency (capture → encode → transport → decode → render).
//! Frame-by-frame subtraction of the two timecodes = true photon-to-photon
//! latency.
//!
//! # Vsync synchronisation
//!
//! On Windows: `IDXGIOutput::WaitForVBlank` is called in a background thread.
//! When the vblank interrupt fires, it signals the main loop to request a
//! redraw. This pins the repaint to the display's physical scanout cycle.
//!
//! On Linux: `DRM_IOCTL_WAIT_VBLANK` on `/dev/dri/card0` provides the same
//! signal. On composited desktops (Wayland) vsync is provided by the
//! compositor's `frame` event; `winit` / `softbuffer` handle the plumbing.
//!
//! # Building
//!
//! ```bash
//! # from bench/camera-rig/screen-timecode/
//! cargo build --release
//! ./target/release/screen-timecode --digit-height 80
//! ```
//!
//! See `README.md` in this directory for the full operating procedure.

use anyhow::Result;
use clap::Parser;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[cfg(target_os = "linux")]
mod vsync_linux;
#[cfg(windows)]
mod vsync_windows;

// ── digit LUT ─────────────────────────────────────────────────────────────────

/// 7-segment encoding for digits 0-9.
/// Bit positions: 6=top(a), 5=top-right(b), 4=bottom-right(c),
///                3=bottom(d), 2=bottom-left(e), 1=top-left(f), 0=middle(g)
#[rustfmt::skip]
const SEG: [u8; 10] = [
    0b111_1110,  // 0
    0b011_0000,  // 1
    0b110_1101,  // 2
    0b111_1001,  // 3
    0b011_0011,  // 4
    0b101_1011,  // 5
    0b101_1111,  // 6
    0b111_0000,  // 7
    0b111_1111,  // 8
    0b111_1011,  // 9
];

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug, Clone)]
#[command(
    version,
    about = "Paint a vsync-locked timecode for the bench camera rig"
)]
struct Args {
    /// Digit height in pixels (80+ recommended for 240 fps readability)
    #[arg(long, default_value_t = 80)]
    digit_height: u32,

    /// Margin from the top-left corner, pixels
    #[arg(long, default_value_t = 16)]
    margin: u32,

    /// Segment bar thickness in pixels (default: digit_height / 10)
    #[arg(long)]
    seg_thickness: Option<u32>,

    /// Paint the binary stripe at y=0 (disable only for debugging)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    binary_stripe: bool,
}

// ── application state ─────────────────────────────────────────────────────────

struct App {
    args: Args,
    start: Instant,
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    context: Option<softbuffer::Context<Arc<Window>>>,
    // Set by the vsync thread to signal the main loop to repaint.
    vsync_flag: Arc<AtomicBool>,
}

impl App {
    fn new(args: Args, vsync_flag: Arc<AtomicBool>) -> Self {
        Self {
            args,
            start: Instant::now(),
            window: None,
            surface: None,
            context: None,
            vsync_flag,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("iExtend timecode")
            .with_decorations(false)
            .with_transparent(false);

        let window = Arc::new(el.create_window(attrs).expect("create window"));

        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface =
            softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");

        self.context = Some(context);
        self.surface = Some(surface);
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),

            WindowEvent::RedrawRequested => {
                let Some(window) = self.window.as_ref() else {
                    return;
                };
                let Some(surface) = self.surface.as_mut() else {
                    return;
                };

                let size = window.inner_size();
                let w = size.width;
                let h = size.height;
                if w == 0 || h == 0 {
                    return;
                }

                surface
                    .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
                    .expect("resize surface");

                let mut buf = surface.buffer_mut().expect("buffer_mut");
                paint_timecode(
                    &mut buf,
                    w,
                    h,
                    self.start.elapsed().as_micros() as u64,
                    &self.args,
                );
                buf.present().expect("present");
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        // Poll the vsync flag; when the vsync thread fires, request a repaint.
        if self.vsync_flag.swap(false, Ordering::Relaxed) {
            if let Some(w) = self.window.as_ref() {
                w.request_redraw();
            }
        }
    }
}

// ── painting ──────────────────────────────────────────────────────────────────

/// Fill the softbuffer with a white background, then paint the timecode.
/// `buf` is a slice of u32 pixels in 0x00RRGGBB format (softbuffer default).
fn paint_timecode(
    buf: &mut softbuffer::Buffer<'_, Arc<Window>, Arc<Window>>,
    w: u32,
    h: u32,
    microseconds: u64,
    args: &Args,
) {
    let w = w as usize;
    let h = h as usize;

    // Clear to white.
    for px in buf.iter_mut() {
        *px = 0x00FF_FFFF;
    }

    // ── binary stripe at y=0 ───────────────────────────────────────────────
    if args.binary_stripe {
        let block_w = (w / 32).max(1);
        let val32 = (microseconds & 0xFFFF_FFFF) as u32;
        for bit in 0..32usize {
            let colour: u32 = if (val32 >> (31 - bit)) & 1 == 1 {
                0x00FF_FFFF // white
            } else {
                0x0000_0000 // black
            };
            let x_start = bit * block_w;
            let x_end = ((bit + 1) * block_w).min(w);
            for x in x_start..x_end {
                // Stripe height: 4 pixels for visibility.
                for y in 0..4usize {
                    let idx = y * w + x;
                    if idx < buf.len() {
                        buf[idx] = colour;
                    }
                }
            }
        }
    }

    // ── 7-segment digits ──────────────────────────────────────────────────
    let dh = args.digit_height as usize;
    let dw = dh / 2;
    let seg_t = args.seg_thickness.unwrap_or(args.digit_height / 10).max(1) as usize;
    let margin_x = args.margin as usize;
    let margin_y = args.margin as usize + 8; // 8 px gap below binary stripe

    // Render up to 7 decimal digits of `microseconds mod 10_000_000`.
    let display_val = microseconds % 10_000_000;
    let digit_str = format!("{display_val:07}");
    let digits: Vec<u8> = digit_str.bytes().map(|b| b - b'0').collect();

    for (i, &d) in digits.iter().enumerate() {
        let ox = margin_x + i * (dw + 4);
        let oy = margin_y;
        draw_digit(buf, w, h, d, ox, oy, dw, dh, seg_t);
    }
}

/// Draw a single 7-segment digit at pixel offset (ox, oy).
fn draw_digit(
    buf: &mut [u32],
    stride: usize,
    h: usize,
    digit: u8,
    ox: usize,
    oy: usize,
    dw: usize,
    dh: usize,
    seg_t: usize,
) {
    let segs = SEG[digit.min(9) as usize];
    let half = dh / 2;
    let black: u32 = 0x0000_0000;

    // Paint a horizontal bar of `len` pixels at (x, y) with height `seg_t`.
    macro_rules! h_bar {
        ($x:expr, $y:expr, $len:expr) => {{
            let (bx, by, blen) = ($x, $y, $len);
            for dx in 0..$len {
                for dy in 0..seg_t {
                    let px = (by + dy) * stride + (bx + dx);
                    if (by + dy) < h && (bx + dx) < stride && px < buf.len() {
                        buf[px] = black;
                    }
                }
            }
            let _ = blen; // suppress unused warning
        }};
    }

    // Paint a vertical bar of height `ht` at (x, y) with width `seg_t`.
    macro_rules! v_bar {
        ($x:expr, $y:expr, $ht:expr) => {{
            let (bx, by, bht) = ($x, $y, $ht);
            for dy in 0..bht {
                for dx in 0..seg_t {
                    let px = (by + dy) * stride + (bx + dx);
                    if (by + dy) < h && (bx + dx) < stride && px < buf.len() {
                        buf[px] = black;
                    }
                }
            }
        }};
    }

    if segs & (1 << 6) != 0 {
        h_bar!(ox, oy, dw);
    } // top
    if segs & (1 << 5) != 0 {
        v_bar!(ox + dw - seg_t, oy, half);
    } // top-right
    if segs & (1 << 4) != 0 {
        v_bar!(ox + dw - seg_t, oy + half, half);
    } // bot-right
    if segs & (1 << 3) != 0 {
        h_bar!(ox, oy + dh - seg_t, dw);
    } // bottom
    if segs & (1 << 2) != 0 {
        v_bar!(ox, oy + half, half);
    } // bot-left
    if segs & (1 << 1) != 0 {
        v_bar!(ox, oy, half);
    } // top-left
    if segs & (1 << 0) != 0 {
        h_bar!(ox, oy + half - seg_t / 2, dw);
    } // middle
}

// ── entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Args::parse();

    // Shared flag: vsync thread sets true, main loop clears it after redraw.
    let vsync_flag = Arc::new(AtomicBool::new(false));

    // Install the platform vsync handler (no-op on systems without DRM/DXGI).
    #[cfg(windows)]
    vsync_windows::install_present_callback(vsync_flag.clone())?;

    #[cfg(target_os = "linux")]
    vsync_linux::install_drm_vblank_handler(vsync_flag.clone())?;

    let event_loop = EventLoop::new()?;
    // Poll mode: the main loop runs `about_to_wait` after every event batch,
    // where we check the vsync flag.
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(args, vsync_flag);
    event_loop.run_app(&mut app)?;

    Ok(())
}
