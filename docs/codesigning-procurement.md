# Code Signing Procurement Guide

This document covers the administrative steps to acquire and renew code-signing
credentials for all three iExtend release surfaces: Windows (EV Authenticode),
Linux (MOK), and iPad (Apple Developer Program).

---

## 1. Windows — EV Authenticode Certificate

### Why EV, not OV?

An **Extended Validation (EV)** code-signing certificate is required for
shipping kernel drivers and is strongly preferred for MSIX packages.
OV (Organization Validation) certs trigger Microsoft SmartScreen warnings for
many months after first use.  EV certs bypass SmartScreen immediately.

### Vendor comparison

| Vendor | Annual cost (EV) | Token included | Notes |
|---|---|---|---|
| **DigiCert** | ~$474/yr | SafeNet eToken 5110 | Recommended; fastest issuance (~3 days) |
| **Sectigo** (formerly Comodo) | ~$299/yr | SafeNet or Thales | Budget option; 5–7 day vetting |
| **GlobalSign** | ~$399/yr | SafeNet eToken | Enterprise-friendly; GSMA liaison |
| **Entrust** | ~$500/yr | SafeNet | HSM integration options |

**Recommendation:** DigiCert for first purchase (fast, well-documented CI
integration).  Renew with the same vendor for continuity.

### Procurement steps

1. **Order:** Visit https://www.digicert.com/signing/code-signing-certificates/
   and select "Extended Validation Code Signing".
   - Use a corporate credit card (faster approval than personal).
   - Provide organization legal name, address, phone, and DUNS number if you have one.

2. **Identity vetting:** DigiCert will call the phone number on record to
   verify the organization.  Allow 3–5 business days.

3. **Token delivery:** The certificate is provisioned onto a SafeNet eToken 5110
   USB hardware security module (FIPS 140-2 Level 2).  The private key is
   non-exportable by design.  Allow another 2–3 business days for shipping.

4. **Install token drivers:**
   ```powershell
   # Download SafeNet Authentication Client from DigiCert's portal, then:
   # Install SAC, plug in token, verify cert appears:
   certutil -store -user My
   # Expected: Subject = iExtend, Issuer = DigiCert EV Code Signing CA G2
   ```

5. **Record the thumbprint:**
   ```powershell
   Get-ChildItem -Path Cert:\CurrentUser\My |
     Where-Object { $_.Subject -match "iExtend" } |
     Select-Object Thumbprint, Subject, NotAfter
   ```
   Store the thumbprint in:
   - GitHub Actions secret: `IEXTEND_EV_THUMBPRINT`
   - 1Password vault entry: `iExtend / Windows / EV Cert Thumbprint`
   - This document (last 8 characters only): `...________`

6. **Test signing:**
   ```powershell
   $env:IEXTEND_EV_THUMBPRINT = "<thumbprint>"
   .\host\installer\windows\build.ps1 -NoSign:$false -Version "0.0.1.0"
   # Confirm: no signtool errors, SmartScreen does NOT warn on install
   ```

### Renewal

- EV certs expire after 1 year (DigiCert) or 2 years (some vendors as of 2024).
- Set a calendar reminder 60 days before expiry (`NotAfter` field above).
- Renewal reuses the same token; DigiCert provisions the renewed cert onto it.
- After renewal, update `IEXTEND_EV_THUMBPRINT` in GitHub secrets.
- Counter-signatures (RFC 3161 timestamps) in already-shipped packages remain
  valid after the cert expires.

### Cloud HSM alternative (DigiCert KeyLocker)

Hardware tokens cannot be used on GitHub-hosted runners.  After v1, evaluate:

- **DigiCert KeyLocker**: ~$500/yr additional; private key lives in DigiCert's
  FIPS 140-2 Level 3 HSM; compatible with `azuresigntool` or DigiCert's own
  CI plugin.
- This removes the self-hosted runner requirement and enables signing on any
  `windows-latest` runner.

---

## 2. Linux — DKMS Module Signing Key (MOK)

Linux does not use a commercial CA for kernel module signing.  Instead, a
self-managed **Machine Owner Key (MOK)** is generated locally and enrolled
into each end-user's firmware.

### Key generation

The key is generated once during the `iextend-evdi-dkms` package installation
(via `host/packaging/linux/iextend-evdi-dkms/sign-key.sh` from Plan 4):

```sh
# Generates /var/lib/dkms/iextend-evdi/mok/MOK.{pem,der,priv}
openssl req -new -x509 -newkey rsa:4096 -keyout MOK.priv \
  -out MOK.pem -days 36500 -subj "/CN=iExtend DKMS MOK" -nodes
openssl x509 -in MOK.pem -out MOK.der -outform DER
```

The key is stored on the end-user's machine, not in the repository.  The
public part (`MOK.der`) is enrolled into their firmware's MOK list via
`mokutil --import`.

### No commercial cost

There is no vendor or CA involved.  The only "cost" is the one-time MOK
enrollment step that end users must complete on SecureBoot systems.

### Key backup

- The private key (`MOK.priv`) should **not** be shared or stored centrally.
  It exists only on the user's machine.
- If the key is lost, a new one is generated on reinstall; the old enrollment
  entry can be revoked with `mokutil --delete` (using the old DER) or simply
  left as a harmless orphan.

---

## 3. iPad — Apple Developer Program

### Enrollment

1. Visit https://developer.apple.com/programs/
2. Click "Enroll" and sign in with your Apple ID.
3. Cost: **USD $99/year** (billed annually; auto-renews).
4. Organization enrollment requires a D-U-N-S number (apply at
   https://www.dnb.com/duns-number/get-a-duns.html — free, takes 1–5 business days).
5. Apple vetting: individual accounts are approved in minutes; organization
   accounts take 2–14 business days.

### App ID

After enrollment, create the App ID:

1. Developer Portal → Identifiers → `+` → App IDs → App
2. Description: `iExtend`
3. Bundle ID (Explicit): `com.iextend.iExtend`
4. Capabilities to enable:
   - **Network Extensions**: not needed (we use Network.framework, no entitlement required)
   - No other non-standard capabilities required

### Distribution certificate

1. Certificates → `+` → iOS Distribution (App Store and Ad Hoc)
2. Generate a CSR on your Mac:
   ```sh
   openssl req -new -newkey rsa:2048 -nodes \
     -keyout iextend-distribution.key \
     -out iextend-distribution.csr \
     -subj "/CN=iExtend Distribution/O=iExtend"
   ```
3. Upload the CSR, download the resulting `.cer`.
4. Convert to `.p12` for fastlane match:
   ```sh
   # Double-click the .cer in Keychain Access, then export as .p12
   ```
5. Store the `.p12` in:
   - 1Password vault: `iExtend / Apple / Distribution Cert (p12)`
   - Passphrase in: `iExtend / Apple / Distribution Cert Passphrase`
   - **Never commit to git.**

### Provisioning profile

1. Profiles → `+` → App Store → select the App ID → select the distribution cert.
2. Name: `iExtend App Store`
3. Download and add to Xcode, or let fastlane match manage it (recommended).

### fastlane match setup (recommended)

`match` stores certs and profiles encrypted in a private git repo.  Setup:

```sh
cd ipad/installer/AppStoreConnect
bundle exec fastlane match init
# Choose: git
# Enter URL: git@github.com:iextend/iextend-certs.git  (private repo)
bundle exec fastlane match appstore
```

The encryption passphrase (`MATCH_PASSWORD`) is stored in GitHub Actions secrets.

### Renewal

- Distribution certificates expire after **1 year**.
- Provisioning profiles expire after **1 year** (or when the cert expires).
- fastlane match auto-detects expiry and can renew with `fastlane match appstore --force`.
- Apple Developer Program auto-renews at $99/yr; keep the credit card on file current.

### App Store Connect setup checklist

After enrollment, complete these in App Store Connect before first submission:

- [ ] Create the app record: Apps → `+` → New App
  - Platform: iOS
  - Name: iExtend
  - Bundle ID: `com.iextend.iExtend`
  - SKU: `iextend-001`
  - User Access: Full Access
- [ ] App Privacy: declare data types (see PrivacyPolicy.md — all None except
  optional Crash Data)
- [ ] Privacy Policy URL: `https://iextend.example/privacy`
- [ ] App Information: Category → Utilities
- [ ] Pricing: Free
- [ ] Age Rating: 4+

---

## 4. Summary of annual costs

| Item | Cost/yr | Required for |
|---|---|---|
| EV Authenticode cert (DigiCert) | ~$474 | Windows MSIX + WHQL |
| Apple Developer Program | $99 | iPad App Store + TestFlight |
| DigiCert KeyLocker (cloud HSM) | ~$500 | CI signing without self-hosted runner (optional v1+) |
| Linux MOK key | $0 | Linux DKMS signing (self-managed) |
| **Total (v1 minimum)** | **~$573/yr** | |
| **Total (with KeyLocker)** | **~$1073/yr** | |

---

## 5. Security practices

- Never commit private keys, .p12 files, or cert passphrases to any repository.
- Store all credentials in 1Password under the `iExtend / Signing` vault.
- Grant access to the `iExtend / Signing` vault only to engineers who run
  releases.
- Rotate the `MATCH_PASSWORD` annually.
- If a signing key is compromised, revoke it immediately via the vendor portal
  and generate a new one.  For the MOK key, it is machine-local and
  self-revokeable; for EV certs, contact DigiCert support.
