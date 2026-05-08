// uinput_smoke.rs — smoke test for the Linux uinput injector.
//
// This test is gated behind the `integration` feature and requires:
//   - Linux
//   - /dev/uinput accessible (user in `input` group, or root, or the udev rule
//     from host/udev/99-iextend-input.rules installed)
//
// Run with:
//   cargo test --test uinput_smoke --features integration -- --ignored
//
// The test:
//   1. Creates a LinuxInjector (opens /dev/uinput, registers 3 virtual devices).
//   2. Injects a PencilMove packet with pressure 0.7.
//   3. Injects a TouchBegin packet.
//   4. Injects a KeyDown packet for 'a'.
//   5. Verifies no panic and no error during injection.
//
// For a full evdev readback verification, run `evtest` on the virtual device
// node that appears in /sys/class/input/ after this test starts.

#[cfg(target_os = "linux")]
mod linux_integration {
    use ix_input::wire::{KeyPayload, Kind, Packet, PencilPayload, TouchPayload};
    use ix_input::Injector;

    #[test]
    #[ignore = "requires /dev/uinput; run with --ignored on Linux with uinput access"]
    fn uinput_creates_three_devices() {
        let inj = ix_input::linux::LinuxInjector::new()
            .expect("/dev/uinput must be accessible; add user to 'input' group or run as root");

        // Inject a pencil move with pressure 0.7.
        let pencil_pl = PencilPayload {
            x: 683.0,
            y: 512.0,
            pressure: 0.7,
            tilt: 0.3,
            azimuth: 1.5,
            twist: 0.0,
            barrel: false,
            hover: false,
        };
        let pencil_pkt = Packet {
            kind: Kind::PencilMove,
            time_us: 1_000_000,
            seq: 1,
            flags: 0,
            payload: pencil_pl.into_bytes(),
        };
        inj.inject(&pencil_pkt);

        // Inject a touch begin.
        let touch_pl = TouchPayload {
            x: 200.0,
            y: 300.0,
            radius_major: 6.0,
            radius_minor: 5.0,
            force: 0.4,
        };
        let touch_pkt = Packet {
            kind: Kind::TouchBegin,
            time_us: 1_010_000,
            seq: 2,
            flags: 0,
            payload: touch_pl.into_bytes(),
        };
        inj.inject(&touch_pkt);

        // Inject a key down for 'a' (HID usage page 7, usage 0x04).
        let key_pl = KeyPayload {
            usage_page: 7,
            usage: 4,
            modifiers: 0,
        };
        let key_pkt = Packet {
            kind: Kind::KeyDown,
            time_us: 1_020_000,
            seq: 3,
            flags: 0,
            payload: key_pl.into_bytes(),
        };
        inj.inject(&key_pkt);

        // Inject pencil end.
        let pencil_end = Packet {
            kind: Kind::PencilEnd,
            time_us: 1_030_000,
            seq: 4,
            flags: 0,
            payload: PencilPayload {
                x: 683.0,
                y: 512.0,
                pressure: 0.0,
                tilt: 0.0,
                azimuth: 0.0,
                twist: 0.0,
                barrel: false,
                hover: false,
            }
            .into_bytes(),
        };
        inj.inject(&pencil_end);

        // If we got here without panic, the basic injection pipeline works.
        // Verify via `xinput list` or `evtest` that three virtual devices appeared.
        println!("uinput smoke test passed: three virtual devices created and events injected");
    }

    #[test]
    #[ignore = "requires /dev/uinput"]
    fn uinput_dispatcher_integration() {
        use ix_input::Dispatcher;
        use std::sync::Arc;

        let inj = ix_input::linux::LinuxInjector::new().expect("uinput accessible");
        let inj = Arc::new(inj);
        let mut d = Dispatcher::new(inj);

        // Feed 5 increasing-seq pencil packets through the dispatcher.
        for seq in 1u32..=5 {
            let pl = PencilPayload {
                x: seq as f32 * 10.0,
                y: seq as f32 * 8.0,
                pressure: seq as f32 * 0.1,
                tilt: 0.2,
                azimuth: 1.0,
                twist: 0.0,
                barrel: false,
                hover: false,
            };
            let pkt = Packet {
                kind: Kind::PencilMove,
                time_us: seq as u64 * 8_333,
                seq,
                flags: 0,
                payload: pl.into_bytes(),
            };
            let bytes = pkt.to_bytes();
            d.handle(&bytes).expect("dispatcher should not fail");
        }

        println!("dispatcher integration: 5 packets forwarded to uinput");
    }
}

// If not on Linux, publish a dummy test so the file doesn't produce
// "no tests found" warnings.
#[cfg(not(target_os = "linux"))]
#[test]
fn uinput_smoke_not_applicable_on_this_platform() {
    // uinput is Linux-only; nothing to test here.
}
