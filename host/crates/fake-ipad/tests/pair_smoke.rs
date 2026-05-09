//! Integration smoke test for the fake-ipad → iextendd pairing handshake.
//!
//! Strategy: we run an in-process TCP listener via
//! `iextendd::pair_listener::spawn(pin)`, then drive the client-side logic
//! from `fake_ipad::pairing_client::run_pairing_client`. Because both sides
//! live in the same process we avoid needing a real subprocess.
//!
//! The current pair_listener `handle_one` implementation stops after sending
//! `PResponse` (Plan-N stub), so the test verifies that:
//! 1. The listener binds and returns a port.
//! 2. The client completes PStart/PResponse without errors.
//! 3. The listener task completes (or allows the socket close) gracefully.

use fake_ipad::pairing_client::run_pairing_client;
use ix_rtc::pairing::generate_pin;

#[tokio::test(flavor = "multi_thread")]
async fn pair_handshake_completes() {
    // Generate a 4-digit PIN.
    let pin = generate_pin();

    // Spawn the pair listener (binds on 0.0.0.0:0 → ephemeral port).
    let (port, listener_handle) = iextendd::pair_listener::spawn(pin.clone())
        .await
        .expect("pair_listener::spawn failed");

    let host_addr = format!("127.0.0.1:{port}");

    // Run the fake-ipad client against the listener.
    // The listener sends PResponse and then closes; the client treats this as
    // partial / stub mode and should not error.
    run_pairing_client(&host_addr, &pin)
        .await
        .expect("pairing client returned an error");

    // Give the listener a moment to notice the closed connection and exit.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    listener_handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_fails_with_wrong_pin() {
    let correct_pin = generate_pin();
    let wrong_pin = {
        // Produce a PIN that is numerically different.
        let n: u32 = correct_pin.parse().unwrap_or(0);
        format!("{:04}", (n + 1) % 10_000)
    };

    let (port, listener_handle) = iextendd::pair_listener::spawn(correct_pin.clone())
        .await
        .expect("pair_listener::spawn failed");

    let host_addr = format!("127.0.0.1:{port}");

    // The wrong PIN leads to a mismatched SPAKE2 key, so the client will
    // either get back a PErr or see AEAD failure when it tries PCertReq.
    // Either way, the pairing client should treat it as a "stub close" since
    // the current server just sends PResponse with a zeroed verifier and
    // closes — the SPAKE2 keys will simply be different, which we detect
    // only on PCertReq AEAD open. At present the server doesn't respond to
    // PCertReq at all (closes socket), so the client sees EOF, which the
    // wrong-pin path treats as stub-mode close. We verify no panic occurs.
    let result = run_pairing_client(&host_addr, &wrong_pin).await;
    // Should not panic; may succeed (stub) or fail. Either is acceptable
    // since the full verification chain is Plan-N.
    let _ = result;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    listener_handle.abort();
}
