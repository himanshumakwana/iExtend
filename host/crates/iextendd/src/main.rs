mod cursor_protocol;
mod grpc_server;
mod keystore;
mod pair_listener;
mod session;
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
        use async_stream::try_stream;
        use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

        let pipe_name = endpoint.0.clone();

        // tonic wants Stream<Item = Result<IO, E>> where IO is AsyncRead+AsyncWrite.
        // On Unix, tokio_stream::UnixListenerStream provides that. On Windows we
        // yield each connected NamedPipeServer instance and pre-create the next
        // so the pipe is always listening for additional clients.
        let incoming = try_stream! {
            // Bind the first instance up-front so the pipe exists before
            // serve_with_incoming starts polling — clients (iextend-tray) get
            // ERROR_FILE_NOT_FOUND otherwise.
            let mut server: NamedPipeServer = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&pipe_name)?;
            loop {
                server.connect().await?;
                let connected = server;
                // first_pipe_instance must be false for subsequent instances
                // of the same pipe name.
                server = ServerOptions::new()
                    .first_pipe_instance(false)
                    .create(&pipe_name)?;
                yield connected;
            }
        };

        tokio::pin!(incoming);
        tokio::select! {
            res = Server::builder().add_service(svc).serve_with_incoming(incoming) => res?,
            _ = wait_for_shutdown_signal() => info!("Ctrl-C received, shutting down"),
        }
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
