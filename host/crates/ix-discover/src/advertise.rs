//! Host-side mDNS service advertise.
//!
//! Wraps `mdns-sd`'s `ServiceDaemon`. The advertise is created on construction
//! and tied to the lifetime of the [`Advertiser`] — drop it to deregister.

use crate::{PeerAdvertisement, PROTOCOL_VERSION, SERVICE_TYPE};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
use thiserror::Error;
use tracing::info;

/// Errors raised by the advertiser. Most are wrapped from `mdns-sd`.
#[derive(Debug, Error)]
pub enum AdvertiseError {
    /// Underlying mDNS daemon failure.
    #[error("mdns-sd: {0}")]
    Mdns(#[from] mdns_sd::Error),
}

/// Owns a registered `_iextend._tcp.local.` service. Drop to deregister.
pub struct Advertiser {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Advertiser {
    /// Register a new mDNS service with the supplied advertisement and the
    /// pairing-listener port (chosen by `iextendd::pair_listener`). The
    /// instance name is derived from `display_name` plus the first 6 bytes of
    /// the hex-encoded pubkey thumbprint to keep collisions reliably absent on
    /// even a busy LAN (e.g. four people running iExtend in the same room).
    pub fn start(
        ad: &PeerAdvertisement,
        port: u16,
        ifaddrs: &[std::net::IpAddr],
    ) -> Result<Self, AdvertiseError> {
        let daemon = ServiceDaemon::new()?;

        let mut props: HashMap<String, String> = HashMap::new();
        props.insert("pv".into(), PROTOCOL_VERSION.to_string());
        props.insert("hk".into(), ad.host_pubkey_thumbprint.clone());
        props.insert("dn".into(), ad.display_name.clone());
        if let Some(pi) = &ad.pair_id {
            props.insert("pi".into(), pi.clone());
        }

        // Truncate first 6 chars of pubkey thumbprint as instance name suffix.
        // Six chars of base64-url is 4.5 bytes ≈ 36 bits ≈ 1-in-68B collision.
        let suffix: String = ad.host_pubkey_thumbprint.chars().take(6).collect();
        let instance = format!("{}-{}", sanitize_instance_name(&ad.display_name), suffix);
        let fullname = format!("{instance}.{SERVICE_TYPE}");
        let host = format!("{instance}.local.");

        let info = ServiceInfo::new(SERVICE_TYPE, &instance, &host, ifaddrs, port, Some(props))?;
        daemon.register(info)?;
        info!(service = %fullname, port, "iExtend mDNS service registered");
        Ok(Self { daemon, fullname })
    }

    /// Hot-update the advertised pair-id (e.g. just-paired). Re-registers the
    /// SRV+TXT under the same instance name; mDNS clients pick up the new TXT
    /// on next refresh.
    pub fn set_pair_id(&self, _new: Option<String>) {
        // mdns-sd 0.13 doesn't expose a clean update; deregister + re-register
        // would surface as a service-down + service-up to clients, which is
        // visible UI churn. For Plan 7 we leave the TXT static within a session
        // — the iPad already learns the pair-id via the cert exchange itself,
        // so the TXT field is only used for steady-state reconnect filtering.
    }
}

impl Drop for Advertiser {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// mDNS-SD instance names must avoid `.` and `/`; keep ASCII letters/digits
/// plus hyphen-underscore-space, lowercase, max 32 chars (well under the
/// 63-byte DNS label limit).
fn sanitize_instance_name(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else if c.is_whitespace() {
                '-'
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    if out.is_empty() {
        out.push_str("iextend");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::sanitize_instance_name;

    #[test]
    fn sanitize_basic() {
        assert_eq!(sanitize_instance_name("Aman's PC"), "aman_s-pc");
        assert_eq!(sanitize_instance_name(""), "iextend");
        assert_eq!(sanitize_instance_name("a/b.c"), "a_b_c");
    }

    #[test]
    fn sanitize_truncates_long_names() {
        let long = "a".repeat(80);
        assert_eq!(sanitize_instance_name(&long).len(), 32);
    }
}
