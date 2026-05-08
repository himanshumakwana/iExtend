# WHQL Submission Runbook — iExtend Driver Package

This directory contains the driver INF files, HLK test configuration, and
submission tooling for obtaining WHQL certification for the iExtend kernel
drivers (`iextend-iddcx` and `iextend-vhf-stylus`).

> **Attention:** WHQL is the longest pole in the release schedule.
> Budget **1 week per submission round**; Microsoft signing turnaround is
> 3–7 business days *after* passing all HLK tests.  Plan the first RC tag
> accordingly — target WHQL submission at least 2 weeks before the public
> release date.

---

## Pre-WHQL notice (v0.x betas)

Pre-WHQL drivers load only on machines that have test-signing enabled:

```powershell
bcdedit /set testsigning on
# (reboot required)
```

**Do not distribute outside closed beta until WHQL approval lands.**

---

## Prerequisites

| Component | Version | Notes |
|---|---|---|
| Windows Hardware Lab Kit (HLK) | 22H2 (matching target OS) | Download from Microsoft |
| HLK Studio | ships with HLK | |
| HLK Controller | Windows Server 2022 recommended | |
| HLK Client | Windows 11 22H2 | Test target machine |
| EV Authenticode cert | any valid EV | Required to submit to Dashboard |
| Microsoft Partner Center account | — | Hardware Dashboard access |

---

## Directory layout

```
whql/
├── iextend-iddcx.inf        — IddCx (virtual display) driver INF
├── iextend-vhf-stylus.inf   — VHF stylus (HID pen) driver INF
├── hlk-config.xml           — HLK Studio test selection
├── submit.ps1               — Hardware Dashboard submission helper
└── README.md                — this file
```

---

## Step-by-step WHQL process

### 1. Set up the HLK lab

1. Install the HLK Controller on a Windows Server 2022 machine (domain-joined
   or in a workgroup — workgroup is fine for a small team).
2. Install the HLK Client on a Windows 11 22H2 test machine.  This is the
   machine under test (MUT).
3. On the HLK Studio machine (can be the same as the controller), open HLK Studio
   and verify both machines appear in the **Machine Pool**.

### 2. Install the test drivers on the MUT

```powershell
# On the MUT, with test-signing enabled:
pnputil /add-driver iextend-iddcx.inf /install
pnputil /add-driver iextend-vhf-stylus.inf /install
```

Verify in Device Manager that both devices appear under:
- Display adapters → `iExtend Indirect Display`
- Human Interface Devices → `iExtend Stylus`

### 3. Run HLK tests

1. In HLK Studio > **Project** > **New Project** > name `iExtend-0.1.0`.
2. **Selection** tab: select the MUT machine, then select both iExtend devices.
3. **Tests** tab: confirm the test list matches `hlk-config.xml` (you can import
   the config via File > Open > hlk-config.xml for pre-selected categories).
4. Click **Run Selected** and wait.  Full DevFund + Display.Indirect + Digitizer
   suite takes approximately 4–8 hours on a typical test machine.
5. On completion, **all tests must be Pass**.  Investigate any failures before
   proceeding.

### 4. Package and submit

1. In HLK Studio > **Package** tab > **Create Package**.
2. Sign the package with the EV cert:
   ```powershell
   signtool sign /sha1 $env:IEXTEND_EV_THUMBPRINT `
     /tr http://timestamp.digicert.com /td sha256 /fd sha256 iExtend-0.1.0.hlkx
   ```
3. Run `submit.ps1` to validate the package, then upload manually:
   - URL: https://partner.microsoft.com/dashboard/hardware
   - Path: Driver > New submission > Upload `.hlkx`
   - Submission name: `iExtend 0.1.0 — IddCx + VHF Stylus`
4. Record the submission ID in this file under **Submission log**.

### 5. Post-signing steps

After Microsoft returns the signed `.cab` (3–7 business days):

1. Download and extract the `.cab`:
   ```powershell
   expand -F:* iExtend-signed.cab whql\signed\
   ```
2. The signed `.cat` files land in `whql/signed/`.
3. Re-run `build.ps1` — it will pick up the signed cats and include them in
   the MSIX payload automatically (edit the staging step to copy from
   `whql/signed/` if needed).
4. Smoke-install the new MSIX on a clean Windows 11 22H2 VM (no test signing).

---

## Submission log

| Date | Build | Submission ID | Result | Signed .cat committed |
|------|-------|---------------|--------|-----------------------|
| (pending) | 0.1.0 | — | — | No |

---

## Certificate renewal

The EV Authenticode cert expires yearly.  Renewal steps:
1. Order renewal from the same vendor (DigiCert / Sectigo / GlobalSign).
2. Update `IEXTEND_EV_THUMBPRINT` in GitHub Actions secrets.
3. Re-sign any packages that will be distributed after the expiry date
   (counter-signatures with RFC 3161 timestamp mean old packages remain valid;
   only future-signed packages need the new cert).

---

## References

- [HLK documentation](https://docs.microsoft.com/en-us/windows-hardware/test/hlk/)
- [Hardware Dashboard](https://partner.microsoft.com/dashboard/hardware)
- [IddCx driver requirements](https://docs.microsoft.com/en-us/windows-hardware/drivers/display/iddcx/)
- [VHF driver requirements](https://docs.microsoft.com/en-us/windows-hardware/drivers/hid/virtual-hid-framework--vhf-)
