# USB Pair Design

**Status:** Approved 2026-05-10. Coexists with existing Wi-Fi pair flow; does not replace.

**Goal:** Pair iPad to laptop daemon over a USB cable using the same `simple-pair-v0` wire protocol the Wi-Fi flow already uses. Tray auto-detects plugged-in iPads via Apple's usbmuxd; falls back to Wi-Fi when no cable is present.

## Why USB at all

- **Latency**: USB 3 round-trip is sub-ms vs ~5-30 ms over typical home Wi-Fi.
- **Reliability**: no IP discovery, no firewall config, no PIN re-typing per session.
- **Setup**: cable plugs in once. iOS "Trust This Computer" prompt is one-time per laptop and is handled by Apple's stack — we don't manage trust ourselves.

## Architecture

### Transport direction inverts

Wi-Fi: iPad is TCP client → laptop's listener at `0.0.0.0:7779`.
USB:  laptop is TCP client → iPad's listener at `127.0.0.1:7780`, tunneled through Apple's usbmuxd.

Wire protocol is **identical** in both directions. The iPad sends `PSimpleHello { pin, client_pubkey_b64, display_name }` first regardless of who opened the socket; the laptop daemon validates and responds with `PSimpleAck { pair_id, host_pubkey_b64 }`.

This means `iextendd::pair_listener::handle_simple_one` is reused as-is for both transports — only the socket source changes.

### Component map

```
┌─ Laptop ────────────────────────────────────────────────────────┐
│                                                                 │
│  iextendd                                                       │
│  ├─ pair_listener.rs       (Wi-Fi: binds 0.0.0.0:7779)          │
│  ├─ usb_listener.rs  ←NEW  (subscribes to USB events)           │
│  │     └─ on iPad plugin: ix-usb::connect_socket(udid, 7780)    │
│  │        → tokio::net::TcpStream → handle_simple_one(...)      │
│  └─ keystore (PinStore writes are unchanged)                    │
│                                                                 │
│  ix-usb (new crate)                                             │
│  └─ wraps libimobiledevice + libusbmuxd                         │
│                                                                 │
│  iextend-tray                                                   │
│  └─ Pair tab gains "USB connected: <iPad name>" chip when an    │
│     iPad is plugged in. PIN UI is unchanged (laptop shows PIN). │
└─────────────────────────────────────────────────────────────────┘
                                ↕  USB cable
┌─ iPad ──────────────────────────────────────────────────────────┐
│                                                                 │
│  USBPairingListener.swift  ←NEW                                 │
│  └─ NWListener on 127.0.0.1:7780, foreground-only               │
│     on accept: same PIN-entry UI as the Wi-Fi pair flow,        │
│     then sends PSimpleHello on the accepted socket.             │
└─────────────────────────────────────────────────────────────────┘
```

## Bundling

| Platform | What ships | Size cost |
|---|---|---|
| Linux .deb | `Depends: libimobiledevice6, usbmuxd` | 0 (system-installed) |
| Windows .exe | Bundled `imobiledevice.dll`, `libusbmuxd.dll`, `iproxy.exe` from libimobiledevice-win32 release | ~3 MB |
| macOS | Already in OS — Apple's stack provides usbmuxd | 0 |

Windows users still need **Apple Mobile Device Service** (installed alongside iTunes / Apple Devices app) for the actual USB stack. If absent, libimobiledevice's connect call returns an error → tray shows "Apple Mobile Device Service required for USB pair" → user falls back to Wi-Fi.

## Threat model

PIN gates pairing the same way over USB as over Wi-Fi.

- USB tunnel terminates at usbmuxd (a localhost daemon on the laptop). Any laptop process can theoretically connect to the iPad's port 7780 if it knows the UDID — PIN is therefore still required.
- iOS "Trust This Computer" prompt is the OS-level permission gate above this. If the iPad doesn't trust the laptop, `idevice_connect_socket` returns an error before any of our code runs.
- We do not need to store or manage trust certificates ourselves.

## Open implementation question (resolved at Task 1)

Which Rust binding to use for libimobiledevice. Candidates:
- `rusty_libimobiledevice` (community wrapper, last updated 2023)
- `idevice` (newer, partial coverage)
- Direct FFI via `bindgen` against the C headers

Pick whichever builds cleanly on Linux (Ubuntu 24.04) AND Windows (msys2 mingw-w64). Default order: try `rusty_libimobiledevice` first, fall back to direct FFI.

## Out of scope (explicit)

- mDNS auto-discovery for the Wi-Fi flow (still manual IP entry)
- Screen share (separate next step the user has asked for after USB pair lands)
- Wi-Fi pair listener teardown when USB is the active path (both keep running)
- Multi-iPad simultaneous USB pair (handle one at a time; queue behavior TBD when needed)
- libimobiledevice trust setup (Apple's stack handles it)

## Non-goals

- We do not aim to support iPads connected via Personal Hotspot's "Wired" mode (works as Wi-Fi from our perspective).
- We do not aim to detect iPads attached to other Macs/Windows machines on the LAN.

## Success criteria

1. Plug in an iPad → tray Pair tab shows "USB connected: <iPad name>" within 2 seconds.
2. Click "Begin pairing" → PIN appears, identical UI to Wi-Fi flow.
3. Type PIN on iPad → `PSimpleAck` returns within 500ms (USB latency budget).
4. Devices tab shows the iPad after the ack.
5. Unplug the iPad → "USB connected" chip disappears within 2 seconds.
6. Re-plug the same iPad → no re-pair required (pubkey already in PinStore).
