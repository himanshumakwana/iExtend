//! SecureBoot detection.
//!
//! On UEFI Linux, the kernel exposes the SecureBoot state as an EFI variable
//! at:
//!
//!   `/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c`
//!
//! The file is a binary blob: a 4-byte EFI attributes prefix followed by one
//! or more data bytes.  The **last** byte is the actual SecureBoot flag:
//! `1` = enabled, `0` = disabled.  (The spec says the flag is at offset 4,
//! i.e. `bytes[4]`, but on every kernel we have tested `bytes.last()` is the
//! same value and is robust to variable-length attributes fields.)
//!
//! We use the `SecureBootProbe` trait for dependency injection so unit tests
//! can supply fake byte streams without touching `/sys`.

// ---------------------------------------------------------------------------
// SecureBootProbe — dependency-injection seam
// ---------------------------------------------------------------------------

/// Reads the raw bytes of the SecureBoot EFI variable.
///
/// Returns `None` when:
/// - The system is not UEFI (no EFI variable filesystem mounted).
/// - The variable is absent (BIOS systems or VMs without UEFI firmware).
pub trait SecureBootProbe {
    fn read_secureboot_efivar(&self) -> Option<Vec<u8>>;
}

// ---------------------------------------------------------------------------
// StdSecureBootProbe
// ---------------------------------------------------------------------------

/// Production implementation: reads directly from `/sys/firmware/efi/efivars/`.
pub struct StdSecureBootProbe;

impl SecureBootProbe for StdSecureBootProbe {
    fn read_secureboot_efivar(&self) -> Option<Vec<u8>> {
        const PATH: &str = "/sys/firmware/efi/efivars/\
            SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
        std::fs::read(PATH).ok()
    }
}

// ---------------------------------------------------------------------------
// is_secureboot_enabled
// ---------------------------------------------------------------------------

/// Returns `true` iff SecureBoot is currently active on this system.
///
/// A missing EFI variable is treated as "disabled" (safe default for
/// BIOS-mode systems and VMs).
pub fn is_secureboot_enabled<P: SecureBootProbe>(probe: &P) -> bool {
    match probe.read_secureboot_efivar() {
        Some(bytes) => bytes.last().copied() == Some(1),
        None => false,
    }
}
