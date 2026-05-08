//! Windows vsync via `IDXGIOutput::WaitForVBlank`.
//!
//! We do not render through DXGI; we just need a high-precision callback that
//! fires once per display refresh cycle so the softbuffer repaint is presented
//! on the correct scanout boundary — eliminating tearing from the camera's
//! perspective.
//!
//! # Implementation
//!
//! 1. `CreateDXGIFactory1` → enumerate adapters → first output (`IDXGIOutput`).
//! 2. Spawn a background thread that calls `WaitForVBlank` in a tight loop.
//! 3. On each return from `WaitForVBlank`, set the shared `AtomicBool` to
//!    `true`.  The `winit` main loop's `about_to_wait` handler polls this flag
//!    and calls `window.request_redraw()`.
//!
//! The thread is intentionally kept simple — no COM apartment considerations
//! beyond the `CoInitialize` on the thread entry.  On displays above 120 Hz
//! the thread may busy-spin for a short time between `WaitForVBlank` calls if
//! Windows coalesces them; this is harmless for a bench-tool binary.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput, DXGI_ERROR_NOT_FOUND,
};

/// Install the vblank callback thread.
///
/// The thread runs for the lifetime of the process.  Returns immediately
/// after the thread is spawned.  If DXGI is not available (unlikely on Win11
/// but possible in headless CI), returns `Ok(())` with no thread — the main
/// loop will use `ControlFlow::Poll` and repaint at CPU speed.
pub fn install_present_callback(flag: Arc<AtomicBool>) -> Result<()> {
    // Get the first available DXGI output (the primary monitor).
    let output = get_primary_output()?;

    std::thread::Builder::new()
        .name("vsync-vblank".into())
        .spawn(move || {
            loop {
                // SAFETY: WaitForVBlank is safe to call from any thread and
                // does not require a live swap chain — it blocks on the
                // hardware scanout counter until the next vblank interrupt.
                let result = unsafe { output.WaitForVBlank() };
                if result.is_err() {
                    // Output disconnected or display driver reset — sleep and
                    // let the main loop continue at Poll speed.
                    std::thread::sleep(std::time::Duration::from_millis(8));
                    continue;
                }
                flag.store(true, Ordering::Relaxed);
            }
        })
        .context("failed to spawn vsync-vblank thread")?;

    Ok(())
}

/// Return the `IDXGIOutput` for the first adapter's first output (primary).
fn get_primary_output() -> Result<IDXGIOutput> {
    // SAFETY: CreateDXGIFactory1 is safe when called once during startup.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
        .context("CreateDXGIFactory1 failed — DXGI not available")?;

    let adapter = unsafe { factory.EnumAdapters1(0) }
        .context("no DXGI adapters found")?;

    let output = unsafe { adapter.EnumOutputs(0) };
    match output {
        Ok(o) => Ok(o),
        Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => {
            anyhow::bail!("adapter has no outputs — headless or docked-display-only?")
        }
        Err(e) => Err(e).context("EnumOutputs(0) failed"),
    }
}
