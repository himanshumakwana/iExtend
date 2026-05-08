use std::time::Instant;
use tonic::{Request, Response, Status};

pub mod proto { tonic::include_proto!("iextend.v1"); }

use proto::{
    daemon_server::Daemon,
    SessionState, Settings, StatusReply, StatusRequest,
    StartSessionReply, StartSessionRequest,
    StopSessionReply, StopSessionRequest, GetSettingsRequest,
};

pub struct DaemonImpl {
    pub started_at: Instant,
}

#[tonic::async_trait]
impl Daemon for DaemonImpl {
    async fn status(&self, _r: Request<StatusRequest>) -> Result<Response<StatusReply>, Status> {
        Ok(Response::new(StatusReply {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_s: self.started_at.elapsed().as_secs(),
            session: SessionState::Idle as i32,
        }))
    }
    async fn start_session(&self, _r: Request<StartSessionRequest>) -> Result<Response<StartSessionReply>, Status> {
        Ok(Response::new(StartSessionReply { started: false, detail: "not implemented (Plan 5)".into() }))
    }
    async fn stop_session(&self, _r: Request<StopSessionRequest>) -> Result<Response<StopSessionReply>, Status> {
        Ok(Response::new(StopSessionReply { stopped: false }))
    }
    async fn get_settings(&self, _r: Request<GetSettingsRequest>) -> Result<Response<Settings>, Status> {
        Ok(Response::new(Settings {
            auto_connect_on_launch: false,
            preferred_codec: "hevc".into(),
            max_bitrate_kbps: 80_000,
            hdr_enabled: false,
        }))
    }
}
