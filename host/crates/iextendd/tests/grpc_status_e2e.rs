#![cfg(unix)]

use std::time::Instant;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::{Endpoint, Server, Uri};
use tower::service_fn;

mod _proto {
    tonic::include_proto!("iextend.v1");
}
use _proto::{daemon_client::DaemonClient, daemon_server::DaemonServer};

#[tokio::test]
async fn status_rpc_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("test.sock");

    // server task
    let sock_for_server = sock.clone();
    let started = Instant::now();
    let server = tokio::spawn(async move {
        let listener = tokio::net::UnixListener::bind(&sock_for_server).unwrap();
        let stream = UnixListenerStream::new(listener);
        // Inline a minimal impl here since grpc_server is a private module of the binary crate
        struct Impl {
            started: Instant,
        }
        #[tonic::async_trait]
        impl _proto::daemon_server::Daemon for Impl {
            async fn status(
                &self,
                _r: tonic::Request<_proto::StatusRequest>,
            ) -> Result<tonic::Response<_proto::StatusReply>, tonic::Status> {
                Ok(tonic::Response::new(_proto::StatusReply {
                    version: "test".into(),
                    uptime_s: self.started.elapsed().as_secs(),
                    session: 0,
                    peers: Vec::new(),
                    paired_count: 0,
                    endpoint: String::new(),
                    pairing_state: 0,
                    usb_devices: Vec::new(),
                }))
            }
            async fn start_session(
                &self,
                _r: tonic::Request<_proto::StartSessionRequest>,
            ) -> Result<tonic::Response<_proto::StartSessionReply>, tonic::Status> {
                Ok(tonic::Response::new(_proto::StartSessionReply {
                    started: false,
                    detail: String::new(),
                }))
            }
            async fn stop_session(
                &self,
                _r: tonic::Request<_proto::StopSessionRequest>,
            ) -> Result<tonic::Response<_proto::StopSessionReply>, tonic::Status> {
                Ok(tonic::Response::new(_proto::StopSessionReply {
                    stopped: false,
                }))
            }
            async fn begin_pairing(
                &self,
                _r: tonic::Request<_proto::BeginPairingRequest>,
            ) -> Result<tonic::Response<_proto::PairingStatus>, tonic::Status> {
                Ok(tonic::Response::new(_proto::PairingStatus::default()))
            }
            async fn get_pairing_status(
                &self,
                _r: tonic::Request<_proto::GetPairingStatusRequest>,
            ) -> Result<tonic::Response<_proto::PairingStatus>, tonic::Status> {
                Ok(tonic::Response::new(_proto::PairingStatus::default()))
            }
            async fn cancel_pairing(
                &self,
                _r: tonic::Request<_proto::CancelPairingRequest>,
            ) -> Result<tonic::Response<_proto::CancelPairingReply>, tonic::Status> {
                Ok(tonic::Response::new(_proto::CancelPairingReply::default()))
            }
            async fn list_paired_devices(
                &self,
                _r: tonic::Request<_proto::ListPairedDevicesRequest>,
            ) -> Result<tonic::Response<_proto::ListPairedDevicesReply>, tonic::Status>
            {
                Ok(tonic::Response::new(
                    _proto::ListPairedDevicesReply::default(),
                ))
            }
            async fn forget_device(
                &self,
                _r: tonic::Request<_proto::ForgetDeviceRequest>,
            ) -> Result<tonic::Response<_proto::ForgetDeviceReply>, tonic::Status> {
                Ok(tonic::Response::new(_proto::ForgetDeviceReply::default()))
            }
            async fn get_settings(
                &self,
                _r: tonic::Request<_proto::GetSettingsRequest>,
            ) -> Result<tonic::Response<_proto::Settings>, tonic::Status> {
                Ok(tonic::Response::new(_proto::Settings::default()))
            }
            async fn set_settings(
                &self,
                _r: tonic::Request<_proto::SetSettingsRequest>,
            ) -> Result<tonic::Response<_proto::Settings>, tonic::Status> {
                Ok(tonic::Response::new(_proto::Settings::default()))
            }
        }
        Server::builder()
            .add_service(DaemonServer::new(Impl { started }))
            .serve_with_incoming(stream)
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // client connection
    let path = sock.clone();
    let channel = Endpoint::try_from("http://[::]:50051")
        .unwrap()
        .connect_with_connector(service_fn(move |_: Uri| {
            let p = path.clone();
            async move {
                Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(
                    tokio::net::UnixStream::connect(p).await?,
                ))
            }
        }))
        .await
        .unwrap();
    let mut client = DaemonClient::new(channel);
    let reply = client
        .status(_proto::StatusRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(reply.version, "test");
    server.abort();
}
