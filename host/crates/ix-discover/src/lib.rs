//! mDNS browse/advertise + pair token verification. Real impl in Plan 7.

#[derive(Debug, Clone)]
pub struct PeerAdvertisement {
    pub host_pubkey_thumbprint: String,
    pub display_name: String,
}
