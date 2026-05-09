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
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
        use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
        use tonic::transport::server::Connected;

        // Newtype wrapper around NamedPipeServer that implements
        // tonic's Connected trait — required by serve_with_incoming.
        // NamedPipeServer is already Unpin (HANDLE-based) so the wrapper
        // can delegate AsyncRead/AsyncWrite via get_mut()-projection.
        struct NamedPipeConn(NamedPipeServer);

        impl Connected for NamedPipeConn {
            type ConnectInfo = ();
            fn connect_info(&self) -> Self::ConnectInfo {}
        }

        impl AsyncRead for NamedPipeConn {
            fn poll_read(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &mut ReadBuf<'_>,
            ) -> Poll<std::io::Result<()>> {
                Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
            }
        }

        impl AsyncWrite for NamedPipeConn {
            fn poll_write(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<std::io::Result<usize>> {
                Pin::new(&mut self.get_mut().0).poll_write(cx, buf)
            }
            fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
                Pin::new(&mut self.get_mut().0).poll_flush(cx)
            }
            fn poll_shutdown(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
            ) -> Poll<std::io::Result<()>> {
                Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
            }
        }

        let pipe_name = endpoint.0.clone();

        // Yield each connected NamedPipeServer wrapped in NamedPipeConn so
        // tonic's serve_with_incoming sees a Connected-implementing IO type.
        let incoming = try_stream! {
            let mut server: NamedPipeServer = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&pipe_name)?;
            loop {
                server.connect().await?;
                let connected = server;
                server = ServerOptions::new()
                    .first_pipe_instance(false)
                    .create(&pipe_name)?;
                yield NamedPipeConn(connected);
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
