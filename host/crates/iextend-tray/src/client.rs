use anyhow::Result;
use ix_transport::LocalEndpoint;

pub mod proto {
    tonic::include_proto!("iextend.v1");
}
use proto::daemon_client::DaemonClient;
use proto::{
    BeginPairingRequest, CancelPairingReply, CancelPairingRequest, ForgetDeviceRequest,
    GetPairingStatusRequest, GetSettingsRequest, ListPairedDevicesReply, PairingStatus,
    SetSettingsRequest, Settings, StartSessionReply, StartSessionRequest, StatusReply,
    StatusRequest, StopSessionReply, StopSessionRequest,
};
use tonic::transport::Channel;

// ─── Platform-specific channel constructor ────────────────────────────────────

#[cfg(unix)]
async fn make_channel(endpoint: &LocalEndpoint) -> Result<Channel> {
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
    Ok(channel)
}

#[cfg(windows)]
async fn make_channel(endpoint: &LocalEndpoint) -> Result<Channel> {
    use tokio::net::windows::named_pipe::ClientOptions;
    use tonic::transport::{Endpoint, Uri};
    use tower::service_fn;

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
    Ok(channel)
}

// ─── Legacy single-call helper (kept for compat) ──────────────────────────────

#[allow(dead_code)]
pub async fn fetch_status(endpoint: &LocalEndpoint) -> Result<String> {
    let reply = status(endpoint).await?;
    Ok(format!("v{} · uptime {}s", reply.version, reply.uptime_s))
}

// ─── Individual RPC wrappers ──────────────────────────────────────────────────

pub async fn status(endpoint: &LocalEndpoint) -> Result<StatusReply> {
    let channel = make_channel(endpoint).await?;
    let mut client = DaemonClient::new(channel);
    Ok(client.status(StatusRequest {}).await?.into_inner())
}

pub async fn begin_pairing(endpoint: &LocalEndpoint) -> Result<PairingStatus> {
    let channel = make_channel(endpoint).await?;
    let mut client = DaemonClient::new(channel);
    Ok(client
        .begin_pairing(BeginPairingRequest {})
        .await?
        .into_inner())
}

pub async fn get_pairing_status(endpoint: &LocalEndpoint) -> Result<PairingStatus> {
    let channel = make_channel(endpoint).await?;
    let mut client = DaemonClient::new(channel);
    Ok(client
        .get_pairing_status(GetPairingStatusRequest {})
        .await?
        .into_inner())
}

pub async fn cancel_pairing(endpoint: &LocalEndpoint) -> Result<CancelPairingReply> {
    let channel = make_channel(endpoint).await?;
    let mut client = DaemonClient::new(channel);
    Ok(client
        .cancel_pairing(CancelPairingRequest {})
        .await?
        .into_inner())
}

pub async fn list_paired_devices(endpoint: &LocalEndpoint) -> Result<ListPairedDevicesReply> {
    let channel = make_channel(endpoint).await?;
    let mut client = DaemonClient::new(channel);
    Ok(client
        .list_paired_devices(proto::ListPairedDevicesRequest {})
        .await?
        .into_inner())
}

pub async fn forget_device(endpoint: &LocalEndpoint, pair_id: String) -> Result<bool> {
    let channel = make_channel(endpoint).await?;
    let mut client = DaemonClient::new(channel);
    let reply = client
        .forget_device(ForgetDeviceRequest { pair_id })
        .await?
        .into_inner();
    Ok(reply.forgotten)
}

pub async fn get_settings(endpoint: &LocalEndpoint) -> Result<Settings> {
    let channel = make_channel(endpoint).await?;
    let mut client = DaemonClient::new(channel);
    Ok(client
        .get_settings(GetSettingsRequest {})
        .await?
        .into_inner())
}

pub async fn set_settings(endpoint: &LocalEndpoint, settings: Settings) -> Result<Settings> {
    let channel = make_channel(endpoint).await?;
    let mut client = DaemonClient::new(channel);
    Ok(client
        .set_settings(SetSettingsRequest {
            settings: Some(settings),
        })
        .await?
        .into_inner())
}

pub async fn start_session(endpoint: &LocalEndpoint) -> Result<StartSessionReply> {
    let channel = make_channel(endpoint).await?;
    let mut client = DaemonClient::new(channel);
    Ok(client
        .start_session(StartSessionRequest {
            peer_id: String::new(),
        })
        .await?
        .into_inner())
}

pub async fn stop_session(endpoint: &LocalEndpoint) -> Result<StopSessionReply> {
    let channel = make_channel(endpoint).await?;
    let mut client = DaemonClient::new(channel);
    Ok(client
        .stop_session(StopSessionRequest {})
        .await?
        .into_inner())
}
