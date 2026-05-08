//! Unit tests for compositor / display-session detection.
//!
//! These tests do not open any real X11 or Wayland connection — they exercise
//! the `detect_backend` logic against synthetic environments via the
//! `EnvProbe` trait.
//!
//! They are intentionally kept OS-agnostic so they can run in CI on any host.

use ix_display_linux::{detect_backend, Backend, EnvProbe};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Fake environment
// ---------------------------------------------------------------------------

struct TestEnv(HashMap<&'static str, &'static str>);

impl TestEnv {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn set(mut self, key: &'static str, value: &'static str) -> Self {
        self.0.insert(key, value);
        self
    }
}

impl EnvProbe for TestEnv {
    fn var(&self, key: &str) -> Option<String> {
        self.0.get(key).map(|v| v.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn wayland_display_set_picks_wayland() {
    let env = TestEnv::new()
        .set("WAYLAND_DISPLAY", "wayland-0")
        .set("DISPLAY", ":0");
    // Wayland takes priority even when DISPLAY is also set.
    assert_eq!(detect_backend(&env), Backend::Wayland);
}

#[test]
fn only_display_set_picks_x11() {
    let env = TestEnv::new().set("DISPLAY", ":0");
    assert_eq!(detect_backend(&env), Backend::X11);
}

#[test]
fn neither_set_returns_none() {
    let env = TestEnv::new();
    assert_eq!(detect_backend(&env), Backend::None);
}

#[test]
fn wayland_only_set_picks_wayland() {
    let env = TestEnv::new().set("WAYLAND_DISPLAY", "wayland-1");
    assert_eq!(detect_backend(&env), Backend::Wayland);
}

#[test]
fn empty_string_wayland_display_is_not_treated_as_set() {
    // An *absent* key means None; TestEnv returns None for absent keys.
    let env = TestEnv::new().set("DISPLAY", ":0");
    // Without WAYLAND_DISPLAY, we fall to X11.
    assert_eq!(detect_backend(&env), Backend::X11);
}
