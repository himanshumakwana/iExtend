mod cursor_protocol;
mod grpc_server;
mod keystore;
mod pair_listener;
mod session;
mod signaling;
mod transport;
mod usb_listener;
#[cfg(windows)]
mod windows_transport;

use ix_transport::LocalEndpoint;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let started = Instant::now();
    let endpoint = LocalEndpoint::default_for_user();
    info!(version = env!("CARGO_PKG_VERSION"), endpoint = %endpoint.0, "iextendd starting");

    let state = std::sync::Arc::new(tokio::sync::RwLock::new(grpc_server::DaemonState::new()));

    // Cancellation token shared with side tasks (usb_listener so far).
    // The main shutdown path fires this after the gRPC server exits, so
    // background tasks unwind cleanly.
    let cancel = CancellationToken::new();

    // USB pair listener — runs in parallel with the gRPC server. Idles
    // gracefully when libimobiledevice isn't installed; never blocks the
    // Wi-Fi pair path.
    let usb_state = state.clone();
    let usb_cancel = cancel.clone();
    tokio::spawn(async move {
        if let Err(e) = usb_listener::run(usb_state, usb_cancel).await {
            error!(err = %e, "usb_listener exited with error");
        }
    });

    // WebRTC signaling listener — accepts the iPad's SDP/ICE bidi stream
    // after pair completes. Idles when port 7783 is unavailable so the rest
    // of the daemon still runs.
    let sig_state = state.clone();
    let sig_cancel = cancel.clone();
    tokio::spawn(async move {
        if let Err(e) = signaling::run(sig_state, sig_cancel).await {
            error!(err = %e, "signaling exited with error");
        }
    });

    let svc = grpc_server::DaemonImpl {
        started_at: started,
        endpoint: endpoint.0.clone(),
        state,
    };
    let svc = grpc_server::proto::daemon_server::DaemonServer::new(svc);

    #[cfg(unix)]
    {
        use tokio_stream::wrappers::UnixListenerStream;
        let listener = transport::LocalServer::bind(endpoint.clone())?;
        let stream = UnixListenerStream::new(listener);
        tokio::select! {
            res = Server::builder().add_service(svc).serve_with_incoming(stream) => res?,
            _ = wait_for_shutdown_signal() => info!("shutdown signal received"),
        }
        let _ = std::fs::remove_file(endpoint.as_path());
    }

    #[cfg(windows)]
    {
        let incoming = windows_transport::incoming_pipes(endpoint.0.clone());
        tokio::pin!(incoming);
        tokio::select! {
            res = Server::builder().add_service(svc).serve_with_incoming(incoming) => res?,
            _ = wait_for_shutdown_signal() => info!("Ctrl-C received, shutting down"),
        }
    }

    // Tell side tasks to unwind. usb_listener honours this within ~500ms.
    cancel.cancel();

    info!(uptime_s = started.elapsed().as_secs(), "iextendd stopped");
    Ok(())
}

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).json().init();
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM");
    let mut int = signal(SignalKind::interrupt()).expect("install SIGINT");
    tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
}

#[cfg(windows)]
async fn wait_for_shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
