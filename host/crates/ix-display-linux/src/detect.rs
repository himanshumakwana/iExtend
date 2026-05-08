//! Compositor and display-session detection.
//!
//! The sole responsibility of this module is to inspect the process environment
//! and decide which capture backend (Wayland or X11) to use.  The logic is
//! intentionally kept thin so that unit tests can exercise it via the
//! `EnvProbe` trait without touching real environment variables.

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

/// Which display-session backend was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// A Wayland compositor is running.  We will use the
    /// `xdg-desktop-portal` ScreenCast portal + PipeWire DMA-BUF path.
    Wayland,
    /// An X11 server is running (or XWayland).  We will use the
    /// MIT-SHM + XDamage path.
    X11,
    /// No graphical session was detected.
    None,
}

// ---------------------------------------------------------------------------
// EnvProbe — dependency-injection seam for tests
// ---------------------------------------------------------------------------

/// Abstracts over environment-variable look-up so unit tests can pass a fake
/// environment without forking the process or mutating `std::env`.
pub trait EnvProbe {
    fn var(&self, key: &str) -> Option<String>;
}

/// Production implementation that reads from the real process environment.
pub struct StdEnv;

impl EnvProbe for StdEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

// ---------------------------------------------------------------------------
// detect_backend
// ---------------------------------------------------------------------------

/// Inspect `env` and return the most appropriate [`Backend`].
///
/// Priority: Wayland > X11 > None.
///
/// `WAYLAND_DISPLAY` being set is the canonical signal that a Wayland
/// compositor is running.  It is set even when XWayland is active, so we
/// prefer the native Wayland path in that case (better zero-copy
/// opportunities, no RANDR quirks).
pub fn detect_backend<E: EnvProbe>(env: &E) -> Backend {
    if env.var("WAYLAND_DISPLAY").is_some() {
        return Backend::Wayland;
    }
    if env.var("DISPLAY").is_some() {
        return Backend::X11;
    }
    Backend::None
}
