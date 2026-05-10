//! Smoke test that the USB listener spawn loop wires up cleanly without
//! an actual USB device. With no iPad plugged in, the listener should run
//! idle and return cleanly when the cancel token fires.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread")]
async fn usb_listener_idle_no_device() {
    if std::env::var("IX_USB_SKIP").is_ok() {
        eprintln!("IX_USB_SKIP set; skipping");
        return;
    }
    let state = Arc::new(RwLock::new(iextendd::DaemonState::new()));
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let handle = tokio::spawn(async move { iextendd::usb_listener::run(state, token).await });

    // Let the listener spin up (or hit the lib-missing path).
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel.cancel();

    let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
    match result {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(e))) => panic!("usb_listener::run returned Err: {e}"),
        Ok(Err(e)) => panic!("usb_listener task panicked: {e}"),
        Err(_) => panic!("usb_listener::run did not exit within 2s of cancel"),
    }
}
