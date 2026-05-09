//! Runtime encoder availability probe.
//!
//! [`Probe::detect`] runs once at daemon startup (~2 ms). It checks for the
//! presence of vendor SDK shared libraries and kernel device nodes, then
//! returns a [`ProbeOutcome`] that the session layer queries for per-peer
//! candidate lists.
//!
//! M4-iPad gating: even if the host has NVENC AV1 hardware, `NvencAv1` is
//! withheld from any peer whose `PeerCaps.av1_decode` is false or whose
//! `peer_kind` is not [`PeerKind::IpadProM4`]. This gate lives in
//! [`ProbeOutcome::candidates_for`], not in `detect()` — the probe is
//! host-only and doesn't know about any peer.

use crate::{EncoderKind, PeerCaps, PeerKind};
use tracing::debug;

/// The result of a hardware-availability probe.
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    /// Ordered list of encoders available on this host, from
    /// highest to lowest priority per [`EncoderKind::priority`].
    available: Vec<EncoderKind>,
}

impl ProbeOutcome {
    /// Iterate over available encoder kinds in probe order.
    pub fn iter(&self) -> std::slice::Iter<'_, EncoderKind> {
        self.available.iter()
    }

    /// `true` if every available encoder is a software encoder.
    /// The daemon surfaces a "battery drain" warning to the tray when this is true.
    pub fn software_fallback_only(&self) -> bool {
        !self.available.is_empty() && self.available.iter().all(|k| k.is_software())
    }

    /// Filter the host-side list against the peer's decode capabilities.
    ///
    /// - AV1 (`NvencAv1`) is only offered when `peer.av1_decode` is true **and**
    ///   `peer.peer_kind` is [`PeerKind::IpadProM4`].
    /// - HEVC encoders are offered whenever `peer.hevc_decode` is true.
    /// - The software fallback is always offered (it falls back to H.264 which
    ///   every iPad can decode).
    ///
    /// The returned list is sorted by [`EncoderKind::priority`] (ascending).
    pub fn candidates_for(&self, peer: &PeerCaps) -> Vec<EncoderKind> {
        let mut out: Vec<EncoderKind> = self
            .available
            .iter()
            .copied()
            .filter(|k| match k {
                EncoderKind::NvencAv1 => {
                    peer.av1_decode && matches!(peer.peer_kind, PeerKind::IpadProM4)
                }
                _ => peer.hevc_decode || k.is_software(),
            })
            .collect();
        out.sort_by_key(|k| k.priority());
        out
    }
}

/// Encoder-availability probe.
pub struct Probe;

impl Probe {
    /// Test seam: bypass OS detection and inject a known candidate list.
    ///
    /// The returned [`ProbeOutcome`] uses the slice as-is (order preserved).
    pub fn synthetic_for_test(candidates: &[EncoderKind]) -> ProbeOutcome {
        ProbeOutcome {
            available: candidates.to_vec(),
        }
    }

    /// Run the real probe against the current host.
    ///
    /// This is cheap (~2 ms): it only stats shared-library paths and
    /// `/dev/dri/renderD*` nodes; it does not open an encode session.
    // Each push is feature-gated; clippy's `vec_init_then_push` doesn't
    // distinguish that case from a static `vec![…]`-initialisable list,
    // so allow it here at the function level.
    #[allow(clippy::vec_init_then_push)]
    pub fn detect() -> ProbeOutcome {
        let mut available: Vec<EncoderKind> = Vec::new();

        #[cfg(feature = "nvenc")]
        {
            if Self::nvenc_present() {
                available.push(EncoderKind::NvencHevc);
                if Self::nvenc_av1_capable() {
                    available.push(EncoderKind::NvencAv1);
                }
            }
        }

        #[cfg(feature = "qsv")]
        {
            if Self::qsv_present() {
                available.push(EncoderKind::QsvHevc);
            }
        }

        #[cfg(feature = "amf")]
        {
            if Self::amf_present() {
                available.push(EncoderKind::AmfHevc);
            }
        }

        #[cfg(feature = "vaapi")]
        {
            if Self::vaapi_present() {
                available.push(EncoderKind::VaapiHevc);
            }
        }

        // Software fallback: always available when compiled in.
        #[cfg(feature = "sw-only")]
        available.push(EncoderKind::X264SoftwareUlllSw);

        debug!(
            available = ?available,
            "encoder probe complete"
        );

        ProbeOutcome { available }
    }

    // ── per-platform probes ──────────────────────────────────────────────────

    #[cfg(feature = "nvenc")]
    fn nvenc_present() -> bool {
        #[cfg(target_os = "linux")]
        {
            // libnvidia-encode.so must be present (nvidia proprietary driver).
            let candidates = [
                "/usr/lib/x86_64-linux-gnu/libnvidia-encode.so.1",
                "/usr/lib/libnvidia-encode.so.1",
            ];
            candidates.iter().any(|p| std::path::Path::new(p).exists())
        }
        #[cfg(target_os = "windows")]
        {
            // nvEncodeAPI64.dll ships with NVIDIA display driver.
            Self::dll_loadable("nvEncodeAPI64.dll")
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            false
        }
    }

    #[cfg(feature = "nvenc")]
    fn nvenc_av1_capable() -> bool {
        // Stub: in the real implementation we'd open a minimal NVENC session
        // and query NV_ENC_CAPS_SUPPORT_LOOKAHEAD_WITH_AQ / codec GUID list.
        // For now, assume AV1 is only on RTX 40-series (Ada) and newer.
        // The probe returns false here to stay safe; callers that actually have
        // the SDK can override by running the capability query.
        false
    }

    #[cfg(feature = "qsv")]
    fn qsv_present() -> bool {
        #[cfg(target_os = "linux")]
        {
            // libvpl.so.2 or libmfx.so
            let candidates = [
                "/usr/lib/x86_64-linux-gnu/libvpl.so.2",
                "/usr/lib/libvpl.so.2",
            ];
            candidates.iter().any(|p| std::path::Path::new(p).exists())
        }
        #[cfg(target_os = "windows")]
        {
            Self::dll_loadable("libvpl.dll") || Self::dll_loadable("libmfx64-gen.dll")
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            false
        }
    }

    #[cfg(feature = "amf")]
    fn amf_present() -> bool {
        #[cfg(target_os = "linux")]
        {
            // AMD on Linux uses VAAPI under the hood; AMF is Windows-primary.
            false
        }
        #[cfg(target_os = "windows")]
        {
            Self::dll_loadable("amfrt64.dll")
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            false
        }
    }

    #[cfg(feature = "vaapi")]
    fn vaapi_present() -> bool {
        #[cfg(target_os = "linux")]
        {
            // At least one render node and libva present.
            let has_drm = std::fs::read_dir("/dev/dri")
                .map(|mut d| {
                    d.any(|e| {
                        e.map(|e| e.file_name().to_string_lossy().starts_with("renderD"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            let has_libva = [
                "/usr/lib/x86_64-linux-gnu/libva.so.2",
                "/usr/lib/libva.so.2",
            ]
            .iter()
            .any(|p| std::path::Path::new(p).exists());
            has_drm && has_libva
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    #[cfg(target_os = "windows")]
    #[allow(dead_code)] // Wired up in Plan 5 follow-up when feature flags activate the hardware-encoder probes.
    fn dll_loadable(name: &str) -> bool {
        // Stub: real impl uses LoadLibraryExA(LOAD_LIBRARY_AS_DATAFILE).
        // Returns false here to avoid actual DLL loading in probe context.
        let _ = name;
        false
    }
}

/// Convenience free function — calls [`Probe::detect`].
pub fn probe_available_encoders() -> ProbeOutcome {
    Probe::detect()
}
