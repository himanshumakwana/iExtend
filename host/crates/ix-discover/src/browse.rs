//! Host-side mDNS service browser. Mainly used by integration tests and the
//! `pair_listener` self-check ("can the iPad actually see me?"). The iPad's
//! NWBrowser equivalent lives in Swift — see `ipad/.../Connection/Browser.swift`.

use crate::{PeerAdvertisement, SERVICE_TYPE};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Errors raised by the browser.
#[derive(Debug, Error)]
pub enum BrowseError {
    /// Underlying mdns-sd failure.
    #[error("mdns-sd: {0}")]
    Mdns(#[from] mdns_sd::Error),
}

/// One discovered peer with the parsed TXT fields and the SRV target.
#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    /// Fully-qualified service name (`<instance>._iextend._tcp.local.`).
    pub fullname: String,
    /// Resolved address(es). Takes the first A/AAAA the resolver reports.
    pub addrs: Vec<std::net::IpAddr>,
    /// SRV port — the iExtend pairing TCP listener.
    pub port: u16,
    /// Parsed advertisement metadata.
    pub ad: PeerAdvertisement,
}

/// A handle to an active browse subscription. Drop to stop browsing.
pub struct Browser {
    daemon: ServiceDaemon,
    rx: mpsc::Receiver<DiscoveredPeer>,
    /// Worker task handle. Aborted on drop.
    _worker: tokio::task::JoinHandle<()>,
}

impl Browser {
    /// Begin browsing for `_iextend._tcp` services on the local link. Discovered
    /// peers arrive on the returned receiver; resolution events for the same
    /// peer may fire multiple times (e.g. address-update) — callers should
    /// dedupe by `fullname`.
    pub fn start() -> Result<Self, BrowseError> {
        let daemon = ServiceDaemon::new()?;
        let receiver = daemon.browse(SERVICE_TYPE)?;
        let (tx, rx) = mpsc::channel(16);

        let worker = tokio::spawn(async move {
            // mdns-sd's receiver is sync; we adapt with spawn_blocking.
            loop {
                let evt = match tokio::task::spawn_blocking({
                    let r = receiver.clone();
                    move || r.recv_timeout(Duration::from_secs(60))
                })
                .await
                {
                    Ok(Ok(evt)) => evt,
                    Ok(Err(_)) => continue, // timeout — keep going
                    Err(e) => {
                        warn!(?e, "mdns browse worker panicked");
                        return;
                    }
                };
                if let ServiceEvent::ServiceResolved(info) = evt {
                    let props = info.get_properties();
                    let ad = PeerAdvertisement {
                        host_pubkey_thumbprint: props
                            .get_property_val_str("hk")
                            .unwrap_or("")
                            .to_string(),
                        display_name: props.get_property_val_str("dn").unwrap_or("").to_string(),
                        pair_id: props
                            .get_property_val_str("pi")
                            .map(|s| s.to_string())
                            .filter(|s| !s.is_empty()),
                    };
                    let peer = DiscoveredPeer {
                        fullname: info.get_fullname().to_string(),
                        addrs: info.get_addresses().iter().copied().collect(),
                        port: info.get_port(),
                        ad,
                    };
                    debug!(?peer, "discovered iExtend peer");
                    if tx.send(peer).await.is_err() {
                        return; // consumer dropped
                    }
                }
            }
        });

        Ok(Self {
            daemon,
            rx,
            _worker: worker,
        })
    }

    /// Receive the next resolved peer. Returns `None` once the worker has
    /// shut down.
    pub async fn next(&mut self) -> Option<DiscoveredPeer> {
        self.rx.recv().await
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}
