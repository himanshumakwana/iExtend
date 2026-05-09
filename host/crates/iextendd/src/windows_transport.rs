//! Windows named-pipe accept loop for the daemon's localhost gRPC server.
//!
//! Tonic's `serve_with_incoming` requires the IO type to implement
//! `tonic::transport::server::Connected` (in addition to the usual
//! `AsyncRead + AsyncWrite + Unpin + Send + 'static`). On Linux we get this
//! for free via `tokio::net::UnixStream`; on Windows we wrap each connected
//! `NamedPipeServer` in [`NamedPipeConn`] which provides the impl.
//!
//! The [`incoming_pipes`] helper returns an `impl Stream<Item = io::Result<…>>`
//! with a *concrete* error type, which is essential — without that type
//! ascription, `try_stream!`'s opaque `AsyncStream` leaves `IE` open and
//! `serve_with_incoming<I, IO, IE>` can't satisfy `IE: Into<tonic::Error>`.

#![cfg(windows)]

use std::pin::Pin;
use std::task::{Context, Poll};

use async_stream::try_stream;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio_stream::Stream;
use tonic::transport::server::Connected;

/// Newtype wrapper around `NamedPipeServer` providing the trait bundle
/// `serve_with_incoming` requires.
///
/// `NamedPipeServer` is already `Unpin + Send`, so the wrapper auto-derives
/// both. `AsyncRead`/`AsyncWrite` delegate via `get_mut()` projection.
pub struct NamedPipeConn(NamedPipeServer);

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
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_shutdown(cx)
    }
}

/// Build the stream of inbound named-pipe connections.
///
/// The first instance is bound up-front so the pipe exists before
/// `serve_with_incoming` starts polling — clients (`iextend-tray`) get
/// `ERROR_FILE_NOT_FOUND` otherwise. After each connect, we eagerly
/// pre-create the next instance with `first_pipe_instance(false)` so
/// additional clients can connect concurrently.
///
/// The explicit `impl Stream<Item = std::io::Result<NamedPipeConn>>` return
/// type is what fixes the upstream `serve_with_incoming` type-inference
/// failure (E0283 on `IE: Into<tonic::Error>`).
pub fn incoming_pipes(pipe_name: String) -> impl Stream<Item = std::io::Result<NamedPipeConn>> {
    try_stream! {
        let mut server: NamedPipeServer = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&pipe_name)?;
        loop {
            server.connect().await?;
            let connected = server;
            // first_pipe_instance must be `false` for subsequent instances
            // of the same pipe name — only the very first allows `true`.
            server = ServerOptions::new()
                .first_pipe_instance(false)
                .create(&pipe_name)?;
            yield NamedPipeConn(connected);
        }
    }
}
