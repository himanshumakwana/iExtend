mod grpc_server;
mod transport;

use ix_transport::LocalEndpoint;
use std::time::Instant;
use tonic::transport::Server;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let started = Instant::now();
    let endpoint = LocalEndpoint::default_for_user();
    info!(version = env!("CARGO_PKG_VERSION"), endpoint = %endpoint.0, "iextendd starting");

    let svc = grpc_server::DaemonImpl {
        started_at: started,
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
        // Windows named-pipe loop: accept one client at a time, spawn server task per conn.
        // (Implementer follows tonic's named-pipe example; ~30 lines.)
        anyhow::bail!("Windows named-pipe accept loop is left as the implementer's exercise; pattern in tonic/examples/src/uds");
    }

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
