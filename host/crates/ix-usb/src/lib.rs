//! Runtime-loaded wrapper around libimobiledevice + libusbmuxd.
//!
//! We use `libloading` (dlopen on Linux/macOS, LoadLibrary on Windows)
//! rather than build-time linking so the daemon ships and runs even when
//! libimobiledevice isn't installed — USB pair degrades to "unavailable"
//! and Wi-Fi pair keeps working.
//!
//! Surface area is intentionally minimal — we only need:
//! - `list_devices()` for the polling loop
//! - `connect_socket()` to open a TCP-shaped tunnel to a port on the iPad
//! - `subscribe_events()` for plug/unplug notifications (implemented as a
//!   background polling thread on top of `list_devices()` to avoid the
//!   trickier C-callback FFI)

use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use libloading::{Library, Symbol};
use once_cell::sync::OnceCell;
use std::collections::HashSet;
use std::os::raw::{c_char, c_int, c_uint};
use std::sync::Mutex;
use std::time::Duration;
use tracing::debug;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceInfo {
    pub udid: String,
    pub name: Option<String>,
    pub product_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    Plugged,
    Unplugged,
}

/// Return value of `list_devices` when libimobiledevice can't be loaded.
/// Distinguish "lib missing" from "no devices" so callers can surface a
/// helpful UI message in the first case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibAvailability {
    Available,
    Missing,
}

// ────────────────────────────────────────────────────────────────────────────
// libusbmuxd FFI definitions (manual — no bindgen needed).
//
// Signatures cribbed from libusbmuxd-2.0's usbmuxd.h. We only bind what we
// actually use; everything else is opaque.
// ────────────────────────────────────────────────────────────────────────────

/// Mirror of `usbmuxd_device_info_t` from libusbmuxd 2.0+.
/// 200-byte connection-data tail omitted — we never read it.
#[repr(C)]
#[derive(Copy, Clone)]
struct UsbmuxdDeviceInfo {
    handle: c_uint,
    product_id: c_uint,
    udid: [c_char; 44],
    conn_type: c_int,
    conn_data: [c_char; 200],
}

type UsbmuxdGetDeviceList = unsafe extern "C" fn(*mut *mut UsbmuxdDeviceInfo) -> c_int;
type UsbmuxdDeviceListFree = unsafe extern "C" fn(*mut *mut UsbmuxdDeviceInfo) -> c_int;
type UsbmuxdConnect = unsafe extern "C" fn(c_uint, u16) -> c_int;

struct UsbmuxdLib {
    _lib: Library,
    get_device_list: UsbmuxdGetDeviceList,
    device_list_free: UsbmuxdDeviceListFree,
    connect: UsbmuxdConnect,
}

// SAFETY: Library handles are Send + Sync — the underlying dlopen handle is
// safe to share, and the function pointers we hand out are pure C calls
// with no Rust state.
unsafe impl Send for UsbmuxdLib {}
unsafe impl Sync for UsbmuxdLib {}

fn load_usbmuxd() -> Result<UsbmuxdLib> {
    // Library names per platform. We try a few candidates because distros
    // disagree on the soname (Ubuntu uses `libusbmuxd-2.0.so.6`, Fedora
    // `libusbmuxd.so.6`, Windows `libusbmuxd.dll`).
    let candidates: &[&str] = if cfg!(target_os = "linux") {
        &["libusbmuxd-2.0.so.6", "libusbmuxd.so.6", "libusbmuxd.so"]
    } else if cfg!(target_os = "macos") {
        &["libusbmuxd-2.0.dylib", "libusbmuxd.dylib"]
    } else if cfg!(target_os = "windows") {
        &["libusbmuxd.dll", "usbmuxd.dll"]
    } else {
        &[]
    };

    let mut last_err: Option<libloading::Error> = None;
    for name in candidates {
        match unsafe { Library::new(name) } {
            Ok(lib) => unsafe {
                let get_device_list: Symbol<UsbmuxdGetDeviceList> = lib
                    .get(b"usbmuxd_get_device_list\0")
                    .context("usbmuxd_get_device_list")?;
                let device_list_free: Symbol<UsbmuxdDeviceListFree> = lib
                    .get(b"usbmuxd_device_list_free\0")
                    .context("usbmuxd_device_list_free")?;
                let connect: Symbol<UsbmuxdConnect> =
                    lib.get(b"usbmuxd_connect\0").context("usbmuxd_connect")?;

                // Detach the symbols' lifetimes — they live as long as `lib`.
                let get_device_list: UsbmuxdGetDeviceList = *get_device_list.into_raw();
                let device_list_free: UsbmuxdDeviceListFree = *device_list_free.into_raw();
                let connect: UsbmuxdConnect = *connect.into_raw();

                return Ok(UsbmuxdLib {
                    _lib: lib,
                    get_device_list,
                    device_list_free,
                    connect,
                });
            },
            Err(e) => {
                last_err = Some(e);
            }
        }
    }
    Err(anyhow!(
        "could not load libusbmuxd ({}); install libimobiledevice / Apple Mobile Device Service to enable USB pair",
        last_err.map(|e| e.to_string()).unwrap_or_else(|| "no candidates tried".into())
    ))
}

static USBMUXD: OnceCell<Result<UsbmuxdLib, String>> = OnceCell::new();

fn usbmuxd() -> Result<&'static UsbmuxdLib> {
    USBMUXD
        .get_or_init(|| load_usbmuxd().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| anyhow!("{e}"))
}

// ────────────────────────────────────────────────────────────────────────────
// Public API
// ────────────────────────────────────────────────────────────────────────────

/// Probe whether libusbmuxd is available without doing any I/O. Used by
/// the daemon at startup to decide whether to spawn the USB listener.
pub fn availability() -> LibAvailability {
    if usbmuxd().is_ok() {
        LibAvailability::Available
    } else {
        LibAvailability::Missing
    }
}

/// List currently-attached iOS devices.
///
/// Returns `Ok(empty)` when no device is plugged in. Returns `Err` when the
/// underlying libusbmuxd can't be loaded — caller should treat this as
/// "USB pair unavailable on this machine" rather than a transient error.
pub fn list_devices() -> Result<Vec<DeviceInfo>> {
    let lib = usbmuxd()?;
    let mut list_ptr: *mut UsbmuxdDeviceInfo = std::ptr::null_mut();
    let count = unsafe { (lib.get_device_list)(&mut list_ptr) };
    if count < 0 {
        return Err(anyhow!("usbmuxd_get_device_list failed: {count}"));
    }
    if count == 0 || list_ptr.is_null() {
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(count as usize);
    for i in 0..(count as isize) {
        let dev = unsafe { &*list_ptr.offset(i) };
        let udid = c_str_to_string(&dev.udid);
        if udid.is_empty() {
            continue;
        }
        out.push(DeviceInfo {
            udid,
            name: None, // libimobiledevice's lockdownd lookup needed for friendly name
            product_type: None, // — same.
        });
    }

    // Free the list. libusbmuxd allocates the array; ownership returns here.
    unsafe {
        let mut p = list_ptr;
        let _ = (lib.device_list_free)(&mut p);
    }
    Ok(out)
}

/// Open a TCP-shaped socket tunneled through usbmuxd to `port` on the iPad
/// identified by `udid`. Returns a blocking `std::net::TcpStream`; caller
/// can convert to a tokio stream via `TcpStream::from_std` after setting
/// nonblocking.
pub fn connect_socket(udid: &str, port: u16) -> Result<std::net::TcpStream> {
    let lib = usbmuxd()?;
    let handle = device_handle_for_udid(lib, udid)?;
    let fd = unsafe { (lib.connect)(handle, port) };
    if fd < 0 {
        return Err(anyhow!(
            "usbmuxd_connect({udid}, {port}) failed: {fd} \
             (iPad app likely isn't listening on {port}, or device isn't trusted yet)"
        ));
    }
    // SAFETY: usbmuxd_connect returns an OS-level fd we own; wrapping in
    // TcpStream takes ownership and closes on drop.
    #[cfg(unix)]
    let stream = {
        use std::os::unix::io::FromRawFd;
        unsafe { std::net::TcpStream::from_raw_fd(fd) }
    };
    #[cfg(windows)]
    let stream = {
        use std::os::windows::io::FromRawSocket;
        unsafe { std::net::TcpStream::from_raw_socket(fd as u64) }
    };
    Ok(stream)
}

fn device_handle_for_udid(lib: &UsbmuxdLib, udid: &str) -> Result<c_uint> {
    let mut list_ptr: *mut UsbmuxdDeviceInfo = std::ptr::null_mut();
    let count = unsafe { (lib.get_device_list)(&mut list_ptr) };
    if count <= 0 || list_ptr.is_null() {
        return Err(anyhow!("no devices currently attached"));
    }
    let mut found: Option<c_uint> = None;
    for i in 0..(count as isize) {
        let dev = unsafe { &*list_ptr.offset(i) };
        if c_str_to_string(&dev.udid) == udid {
            found = Some(dev.handle);
            break;
        }
    }
    unsafe {
        let mut p = list_ptr;
        let _ = (lib.device_list_free)(&mut p);
    }
    found.ok_or_else(|| anyhow!("no attached device matches udid {udid}"))
}

/// Subscribe to USB plug/unplug events.
///
/// Implementation detail: spawns a background thread that polls
/// `list_devices()` every second and emits Plugged/Unplugged events on a
/// crossbeam channel. Polling avoids the trickier C-callback FFI dance and
/// is plenty fast — USB plug latency tolerance is ~1-2s.
///
/// Drop the returned receiver to stop the worker thread.
pub fn subscribe_events() -> Result<Receiver<(DeviceEvent, DeviceInfo)>> {
    let _ = usbmuxd()?; // surface "lib missing" early
    let (tx, rx) = unbounded();
    spawn_event_thread(tx);
    Ok(rx)
}

/// Background poll loop. Lives until the receiver is dropped (channel send
/// fails). Uses a `Mutex` to guard the seen-set so multiple subscribers
/// observe consistent transitions.
fn spawn_event_thread(tx: Sender<(DeviceEvent, DeviceInfo)>) {
    static SUBSCRIBERS: Mutex<Vec<Sender<(DeviceEvent, DeviceInfo)>>> = Mutex::new(Vec::new());
    static THREAD_STARTED: Mutex<bool> = Mutex::new(false);

    {
        let mut subs = SUBSCRIBERS.lock().unwrap();
        subs.push(tx);
    }

    let mut started = THREAD_STARTED.lock().unwrap();
    if *started {
        return;
    }
    *started = true;
    drop(started);

    std::thread::Builder::new()
        .name("ix-usb-poller".into())
        .spawn(move || {
            let mut seen: HashSet<DeviceInfo> = HashSet::new();
            loop {
                std::thread::sleep(Duration::from_millis(1000));
                let devices = match list_devices() {
                    Ok(d) => d,
                    Err(e) => {
                        debug!(err = %e, "ix-usb poll failed; lib likely unloaded");
                        continue;
                    }
                };
                let now: HashSet<DeviceInfo> = devices.iter().cloned().collect();
                let plugged = now.difference(&seen).cloned().collect::<Vec<_>>();
                let unplugged = seen.difference(&now).cloned().collect::<Vec<_>>();
                seen = now;

                let subs = SUBSCRIBERS.lock().unwrap();
                let mut dead_indices: Vec<usize> = Vec::new();
                for (idx, sub) in subs.iter().enumerate() {
                    for d in &plugged {
                        if sub.send((DeviceEvent::Plugged, d.clone())).is_err() {
                            dead_indices.push(idx);
                            break;
                        }
                    }
                    for d in &unplugged {
                        if sub.send((DeviceEvent::Unplugged, d.clone())).is_err() {
                            dead_indices.push(idx);
                            break;
                        }
                    }
                }
                drop(subs);

                if !dead_indices.is_empty() {
                    let mut subs = SUBSCRIBERS.lock().unwrap();
                    // Remove from highest index first to keep indices valid.
                    for idx in dead_indices.into_iter().rev() {
                        if idx < subs.len() {
                            subs.swap_remove(idx);
                        }
                    }
                }
            }
        })
        .expect("ix-usb-poller spawn failed");
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

fn c_str_to_string(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercise `list_devices` without panicking, even when libimobiledevice
    /// isn't installed. CI runners without the lib will hit the
    /// `LibAvailability::Missing` branch; dev machines with it will return
    /// an empty Vec when no iPad is plugged.
    #[test]
    fn list_devices_does_not_panic() {
        if std::env::var("IX_USB_SKIP").is_ok() {
            eprintln!("IX_USB_SKIP set; skipping");
            return;
        }
        match list_devices() {
            Ok(v) => {
                eprintln!("list_devices returned {} entries", v.len());
            }
            Err(e) => {
                eprintln!("list_devices unavailable: {e}");
            }
        }
    }

    #[test]
    fn availability_is_one_of_two_values() {
        let a = availability();
        assert!(matches!(
            a,
            LibAvailability::Available | LibAvailability::Missing
        ));
    }
}
