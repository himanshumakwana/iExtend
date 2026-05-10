//! gRPC service implementation for the daemon.
//!
//! [`DaemonImpl`] holds the long-lived state shared across RPC handlers
//! (uptime, paired-device store, session state, pairing state). The
//! handlers themselves are mostly thin facades over [`crate::session`],
//! [`crate::keystore::PinStore`], and [`crate::pair_listener`].

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

pub mod proto {
    tonic::include_proto!("iextend.v1");
}

use proto::{
    daemon_server::Daemon, BeginPairingRequest, CancelPairingReply, CancelPairingRequest,
    ForgetDeviceReply, ForgetDeviceRequest, GetPairingStatusRequest, GetSettingsRequest,
    ListPairedDevicesReply, ListPairedDevicesRequest, PairedDevice, PairingState, PairingStatus,
    PeerInfo, SessionState, SetSettingsRequest, Settings, StartSessionReply, StartSessionRequest,
    StatusReply, StatusRequest, StopSessionReply, StopSessionRequest, UsbDeviceInfo,
};

/// Shared mutable daemon state.
pub struct DaemonImpl {
    pub started_at: Instant,
    pub endpoint: String,
    pub state: Arc<RwLock<DaemonState>>,
}

/// Centralized daemon state — protected by a single RwLock so handlers can
/// read consistent snapshots and the pair listener / session loop can mutate
/// fields without racing.
pub struct DaemonState {
    pub session: SessionState,
    pub peers: Vec<PeerInfo>,
    pub pairing: PairingStatus,
    pub settings: Settings,
    /// iPads currently attached over USB (libimobiledevice / usbmuxd).
    /// Updated by `usb_listener`. Empty when no USB device is plugged in or
    /// when libimobiledevice isn't installed.
    pub usb_devices: Vec<ix_usb::DeviceInfo>,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            session: SessionState::Idle,
            peers: Vec::new(),
            pairing: PairingStatus {
                state: PairingState::Idle as i32,
                pin: String::new(),
                seconds_left: 0,
                port: 0,
                last_paired: None,
                error: String::new(),
            },
            settings: Settings {
                auto_connect_on_launch: false,
                preferred_codec: "hevc".into(),
                max_bitrate_kbps: 80_000,
                hdr_enabled: false,
                pair_port: crate::pair_listener::DEFAULT_PAIR_PORT as u32,
            },
            usb_devices: Vec::new(),
        }
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}

#[tonic::async_trait]
impl Daemon for DaemonImpl {
    async fn status(&self, _r: Request<StatusRequest>) -> Result<Response<StatusReply>, Status> {
        let state = self.state.read().await;
        let paired_count = paired_count_via_pin_store();
        let usb_devices: Vec<UsbDeviceInfo> = state
            .usb_devices
            .iter()
            .map(|d| UsbDeviceInfo {
                udid: d.udid.clone(),
                display_name: d.name.clone().unwrap_or_default(),
                product_type: d.product_type.clone().unwrap_or_default(),
            })
            .collect();
        Ok(Response::new(StatusReply {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_s: self.started_at.elapsed().as_secs(),
            session: state.session as i32,
            peers: state.peers.clone(),
            paired_count,
            endpoint: self.endpoint.clone(),
            pairing_state: state.pairing.state,
            usb_devices,
        }))
    }

    async fn start_session(
        &self,
        _r: Request<StartSessionRequest>,
    ) -> Result<Response<StartSessionReply>, Status> {
        let mut state = self.state.write().await;
        if matches!(state.session, SessionState::Live | SessionState::Connecting) {
            return Ok(Response::new(StartSessionReply {
                started: false,
                detail: "session already running".into(),
            }));
        }
        // Plan 5 will wire this to the real capture/encode/transport pipeline;
        // for now we just transition state so the tray UI can reflect it.
        state.session = SessionState::Live;
        Ok(Response::new(StartSessionReply {
            started: true,
            detail: "session state → Live (transport not yet wired — Plan 5)".into(),
        }))
    }

    async fn stop_session(
        &self,
        _r: Request<StopSessionRequest>,
    ) -> Result<Response<StopSessionReply>, Status> {
        let mut state = self.state.write().await;
        let was_running = !matches!(state.session, SessionState::Idle);
        state.session = SessionState::Idle;
        state.peers.clear();
        Ok(Response::new(StopSessionReply {
            stopped: was_running,
        }))
    }

    // ── Pairing ──────────────────────────────────────────────────────────
    async fn begin_pairing(
        &self,
        _r: Request<BeginPairingRequest>,
    ) -> Result<Response<PairingStatus>, Status> {
        let pairing = crate::pair_listener::begin(self.state.clone())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(pairing))
    }

    async fn get_pairing_status(
        &self,
        _r: Request<GetPairingStatusRequest>,
    ) -> Result<Response<PairingStatus>, Status> {
        let mut state = self.state.write().await;
        // Tick the seconds_left countdown if WAITING.
        crate::pair_listener::tick(&mut state);
        Ok(Response::new(state.pairing.clone()))
    }

    async fn cancel_pairing(
        &self,
        _r: Request<CancelPairingRequest>,
    ) -> Result<Response<CancelPairingReply>, Status> {
        let cancelled = crate::pair_listener::cancel(self.state.clone()).await;
        Ok(Response::new(CancelPairingReply { cancelled }))
    }

    async fn list_paired_devices(
        &self,
        _r: Request<ListPairedDevicesRequest>,
    ) -> Result<Response<ListPairedDevicesReply>, Status> {
        let devices =
            crate::pair_listener::list_paired().map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListPairedDevicesReply { devices }))
    }

    async fn forget_device(
        &self,
        r: Request<ForgetDeviceRequest>,
    ) -> Result<Response<ForgetDeviceReply>, Status> {
        let pair_id = r.into_inner().pair_id;
        let forgotten =
            crate::pair_listener::forget(&pair_id).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ForgetDeviceReply { forgotten }))
    }

    // ── Settings ─────────────────────────────────────────────────────────
    async fn get_settings(
        &self,
        _r: Request<GetSettingsRequest>,
    ) -> Result<Response<Settings>, Status> {
        let state = self.state.read().await;
        Ok(Response::new(state.settings.clone()))
    }

    async fn set_settings(
        &self,
        r: Request<SetSettingsRequest>,
    ) -> Result<Response<Settings>, Status> {
        let new = r
            .into_inner()
            .settings
            .ok_or_else(|| Status::invalid_argument("settings missing"))?;
        let mut state = self.state.write().await;
        state.settings = new;
        Ok(Response::new(state.settings.clone()))
    }
}

/// Best-effort count of paired devices on disk. Errors return 0 — the tray
/// shows the full list via ListPairedDevices anyway.
fn paired_count_via_pin_store() -> u32 {
    crate::keystore::PinStore::open_default()
        .and_then(|s| s.list())
        .map(|v| v.len() as u32)
        .unwrap_or(0)
}

/// Helper for the pair listener / session loop to map our PairedDevice rows
/// (stored in the sqlite PinStore) to the proto wire type.
pub fn paired_device_to_proto(d: &crate::keystore::PinnedIpad) -> PairedDevice {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    PairedDevice {
        pair_id: d.pair_id.clone(),
        display_name: d.name.clone(),
        pubkey_b64: B64.encode(d.pubkey),
        paired_at_unix: d.paired_at,
    }
}
