use anyhow::Result;
use ix_transport::LocalEndpoint;

pub mod proto {
    tonic::include_proto!("iextend.v1");
}
use proto::daemon_client::DaemonClient;
use proto::StatusRequest;

#[cfg(unix)]
pub async fn fetch_status(endpoint: &LocalEndpoint) -> Result<String> {
    use tokio::net::UnixStream;
    use tonic::transport::{Endpoint, Uri};
    use tower::service_fn;

    let path = endpoint.0.clone();
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_: Uri| {
            let p = path.clone();
            async move {
                Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(UnixStream::connect(p).await?))
            }
        }))
        .await?;

    let mut client = DaemonClient::new(channel);
    let reply = client.status(StatusRequest {}).await?.into_inner();
    Ok(format!("v{} · uptime {}s", reply.version, reply.uptime_s))
}

#[cfg(windows)]
pub async fn fetch_status(endpoint: &LocalEndpoint) -> Result<String> {
    use tokio::net::windows::named_pipe::ClientOptions;
    use tonic::transport::{Endpoint, Uri};
    use tower::service_fn;

    // Mirror of the cfg(unix) path — same tonic Endpoint + service_fn dance,
    // but using a Windows named-pipe client instead of a UnixStream.
    // The "http://[::]:50051" URI is a tonic API requirement (Endpoint needs
    // *some* URI); the connector function below ignores it and dials the
    // pipe directly.
    let pipe_name = endpoint.0.clone();
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_: Uri| {
            let n = pipe_name.clone();
            async move {
                Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(
                    ClientOptions::new().open(&n)?,
                ))
            }
        }))
        .await?;

    let mut client = DaemonClient::new(channel);
    let reply = client.status(StatusRequest {}).await?.into_inner();
    Ok(format!("v{} · uptime {}s", reply.version, reply.uptime_s))
}
