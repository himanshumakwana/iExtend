# Contributing to the iExtend host workspace

## Prereqs

- Rust 1.85+ (workspace MSRV; `rust-toolchain.toml` pins 1.90.0 — `rustup` will fetch it on first build)
- protoc **not** required — `tonic-build` uses the bundled `protoc-bin-vendored` binaries
- Linux: `sudo apt install build-essential libssl-dev pkg-config`
- Windows: Visual Studio 2022 Build Tools with the C++ desktop workload

## Build

```bash
cd host
cargo build --workspace
```

## Run the daemon + tray locally

```bash
# terminal 1
cargo run -p iextendd

# terminal 2
cargo run -p iextend-tray
```

Click "Ping iextendd" in the tray window. You should see the daemon's version and uptime.

## What's actually implemented vs. stubbed

After Plan 2 (this plan): only the gRPC `Status` RPC and the localhost transport. Everything else (capture, encode, WebRTC, pairing, input forwarding, drivers) arrives in Plans 3–10. See `docs/superpowers/plans/`.

## Style

- `cargo fmt` before commits.
- `clippy -D warnings` is the merge gate.
- Library crates (`ix-*`) export traits + minimal types only; impls go in the binaries or in subsystem-owning crates introduced by later plans.
