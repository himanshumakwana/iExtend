//! mDNS-SD service advertise + browse for the iExtend pairing protocol.
//!
//! Service type: `_iextend._tcp.local.`
//!
//! TXT record fields (all values base64-url-no-pad encoded; everything else is
//! UTF-8 text):
//!
//! | key  | meaning                                         |
//! |------|-------------------------------------------------|
//! | `pv` | protocol version (decimal string, currently "1") |
//! | `hk` | host Ed25519 pubkey thumbprint (32 bytes raw → base64-url) |
//! | `dn` | display name (UTF-8, ≤ 64 bytes)                 |
//! | `pi` | optional pair-id once paired (UUID-v4 hex, 36 chars; absent during PIN window) |
//!
//! The pairing TCP listener (Plan 7 Task 8) lives on a separately-allocated
//! port; the SRV record exposes the chosen port to browsers. mDNS-SD is fine
//! with this — the service A/SRV pair encodes everything we need.

#![deny(missing_docs)]

mod advertise;
mod browse;

pub use advertise::{Advertiser, AdvertiseError};
pub use browse::{Browser, BrowseError, DiscoveredPeer};

/// Service type used by both halves of pairing.
pub const SERVICE_TYPE: &str = "_iextend._tcp.local.";
/// Currently the only supported protocol version.
pub const PROTOCOL_VERSION: u32 = 1;

/// A peer's mDNS-advertised handshake metadata (host side: what we send;
/// browser side: what we receive). The thumbprint is the *full* host pubkey
/// since the size is small (32 bytes); decoding is delegated to the keystore.
#[derive(Debug, Clone)]
pub struct PeerAdvertisement {
    /// Base64-url-no-pad encoding of the host's Ed25519 pubkey (32 bytes).
    pub host_pubkey_thumbprint: String,
    /// User-visible name shown in the iPad's "Discover" list.
    pub display_name: String,
    /// Stable pair-id once a successful pairing has been completed; absent
    /// during the PIN window.
    pub pair_id: Option<String>,
}
