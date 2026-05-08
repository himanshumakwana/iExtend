# iExtend hardware-CI cluster

Four boxes representing the four major host configurations iExtend ships for,
each tethered USB-C to a dedicated iPad Pro M2. Used as GitHub Actions
self-hosted runners that run nightly hardware-CI jobs and per-release-branch
smoke tests.

## Bill of materials (BOM)

### Box 1 — Win11 + NVIDIA RTX 4060 (NVENC AV1 / HEVC path)

| Component | Model | Est. cost |
|---|---|---|
| CPU | AMD Ryzen 7 7700 (8c, AM5) | ~$280 |
| Motherboard | ASRock B650M PG Lightning | ~$140 |
| RAM | 32 GB DDR5-6000 (2×16 GB) | ~$110 |
| GPU | ASUS Dual GeForce RTX 4060 8 GB | ~$300 |
| NVMe | WD Black SN770 1 TB | ~$80 |
| PSU | Corsair RM750x | ~$120 |
| Case | Fractal Design Pop Mini Air | ~$80 |
| OS | Windows 11 Pro license | ~$200 |
| **Subtotal** | | **~$1,310** |

### Box 2 — Win11 + Intel iGPU (Quick Sync path)

| Component | Model | Est. cost |
|---|---|---|
| CPU | Intel Core Ultra 5 245K (Meteor Lake; iGPU = Xe-LPG, QSV) | ~$340 |
| Motherboard | ASUS PRIME B860M-A | ~$160 |
| RAM | 32 GB DDR5-5600 | ~$100 |
| NVMe | 1 TB | ~$80 |
| PSU + case | | ~$180 |
| OS | Windows 11 Pro license | ~$200 |
| **Subtotal** | | **~$1,060** |

### Box 3 — Ubuntu 24.04 + AMD RX 7600 (VAAPI HEVC path)

| Component | Model | Est. cost |
|---|---|---|
| CPU | AMD Ryzen 5 7600 | ~$200 |
| Motherboard | B650M | ~$130 |
| RAM | 32 GB DDR5 | ~$100 |
| GPU | ASUS Dual RX 7600 8 GB | ~$280 |
| NVMe | 1 TB | ~$80 |
| PSU + case | | ~$180 |
| **Subtotal** | | **~$970** |

### Box 4 — Ubuntu 24.04 + Intel iGPU (VAAPI + QSV on Linux)

Same hardware as Box 2, no Windows license: **~$860**

### iPads

4× iPad Pro M2 11" 128 GB (refurb or new): **~$3,500**

### Networking and ancillaries

| Item | Est. cost |
|---|---|
| Wi-Fi 6E AP (TP-Link Archer AXE75 or equivalent) | ~$200 |
| 8-port gigabit switch | ~$50 |
| 4× USB-C 3.2 cables, 1 m, certified 5 Gbps | ~$60 |
| 4× 32" 4K monitor (refurb) | ~$700 |
| KVM (optional) | ~$300 |
| UPS (1500 VA, line-interactive) | ~$250 |

### Total hardware cost

**~$8,800** (excluding labor, shipping, and office space)

## Network topology

```
                   ┌──────────────────────────────┐
                   │        Wi-Fi 6E AP            │
                   │   (TP-Link Archer AXE75)      │
                   │   SSID: iextend-bench-6GHz    │
                   └─────────────┬────────────────┘
                                 │ uplink (gigabit)
                   ┌─────────────┴────────────────┐
                   │      8-port gigabit switch    │
                   └──┬────┬────┬────┬────────────┘
                      │    │    │    │
                   Box1  Box2  Box3  Box4
                 (Win RTX) (Win iGPU) (UbAMD) (UbIntel)
                      │    │    │    │
                     iPad  iPad  iPad  iPad   (USB-C tethered)

Static DHCP reservations:
  Box 1 (win11-rtx4060): 10.0.10.11
  Box 2 (win11-igpu):    10.0.10.12
  Box 3 (ubuntu-rx7600): 10.0.10.13
  Box 4 (ubuntu-igpu):   10.0.10.14
```

## Procurement steps

1. **Order all four boxes** as DIY kits or pre-built. Allow 2–4 weeks for
   refurbished iPads.
2. **Assemble / unbox.** Run a clean OS install from fresh media.
3. **Network setup:**
   - Connect all boxes to the gigabit switch and associate them with the
     Wi-Fi 6E AP (for the Wi-Fi 6E latency path).
   - Configure static DHCP reservations in your router so IPs stay stable.
4. **Per-box OS bootstrap (before Ansible):**
   - **Win11 boxes:** install Win11 Pro, create local admin account
     `iextend-runner`, enable OpenSSH Server (Settings → Optional Features),
     enable PowerShell remoting from the operator workstation:
     ```powershell
     Enable-PSRemoting -Force
     Set-Service sshd -StartupType Automatic
     Start-Service sshd
     ```
   - **Ubuntu boxes:** install Ubuntu 24.04 LTS Server, create user
     `iextend-runner`, install `openssh-server`, grant NOPASSWD sudo:
     ```bash
     echo 'iextend-runner ALL=(ALL) NOPASSWD:ALL' | sudo tee /etc/sudoers.d/iextend-runner
     ```
5. **Run Ansible:**
   ```bash
   cd bench/cluster
   cp inventory.yml inventory.local.yml   # edit IPs + runner tokens
   ansible-playbook -i inventory.local.yml playbook.yml --check  # dry run
   ansible-playbook -i inventory.local.yml playbook.yml          # apply
   ```
6. **iPad pairing (manual — requires physical access):** see
   `roles/ipad-tether/tasks/main.yml` for the step-by-step checklist. Each
   iPad must be paired with its host via the SPAKE2 PIN flow before hardware
   CI can run.
7. **Verify:** after pairing, re-run with `--tags verify`:
   ```bash
   ansible-playbook -i inventory.local.yml playbook.yml --tags verify
   ```

## Recurring operating costs

| | Annual |
|---|---|
| Power (4 boxes × 100 W avg × 24/7 × $0.15/kWh) | ~$525 |
| iPad battery service (~1 replacement/yr) | ~$400 |
| Workspace / cooling / network | varies |
| **Total** | **~$1,000+** |

## When the budget doesn't allow hardware CI

Tier-1 synthetic CI (free, per PR) catches ~70% of regressions. For releases,
run the camera rig (Tasks 7–8) and the soak (Task 9) manually on a single dev
workstation. Document the hardware-CI gap in the release notes.

The four playbook files are kept in tree regardless so they're ready when the
budget permits.

## Files in this directory

```
bench/cluster/
├── README.md               — this file (BOM + topology + setup guide)
├── inventory.yml           — Ansible inventory (copy to inventory.local.yml, gitignore)
├── playbook.yml            — master playbook; applies all three roles in order
└── roles/
    ├── base/               — timezone, NTP, hostname, auto-update policy
    ├── iextendd-runner/    — install iextendd package + GitHub Actions runner
    └── ipad-tether/        — pairing checklist + reachability verification
```
