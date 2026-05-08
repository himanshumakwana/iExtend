# screen-timecode

Paints a vsync-locked decimal timecode on the primary display. Used by the
iExtend bench camera rig to measure true photon-to-photon latency.

## Quick start

```bash
# Build (from this directory):
cargo build --release

# Run (auto-picks digit height 80; full-screen decorations disabled):
./target/release/screen-timecode

# Custom digit size:
./target/release/screen-timecode --digit-height 120 --margin 20
```

## Digit shape — 7-segment LUT

Each digit occupies `digit_height/2` × `digit_height` pixels and is drawn
with `digit_height/10` px thick bars (minimum 1 px).

Segment labels follow the standard `a–g` convention:

```
 _        ← a (top)
|_|       ← f (top-left), g (middle), b (top-right)
|_|       ← e (bottom-left), d (bottom), c (bottom-right)
```

Encoding (bit position → segment):

| Bit | Segment | Position    |
|-----|---------|-------------|
| 6   | a       | top         |
| 5   | b       | top-right   |
| 4   | c       | bottom-right|
| 3   | d       | bottom      |
| 2   | e       | bottom-left |
| 1   | f       | top-left    |
| 0   | g       | middle      |

LUT (digits 0–9):

| Digit | Bits       | Segments active  |
|-------|------------|-----------------|
| 0     | 0111_1110  | a b c d e f     |
| 1     | 0011_0000  | b c             |
| 2     | 0110_1101  | a b d e g       |
| 3     | 0111_1001  | a b c d g       |
| 4     | 0011_0011  | b c f g         |
| 5     | 0101_1011  | a c d f g       |
| 6     | 0101_1111  | a c d e f g     |
| 7     | 0111_0000  | a b c           |
| 8     | 0111_1111  | a b c d e f g   |
| 9     | 0111_1011  | a b c d f g     |

## Binary stripe fallback

Along the top pixel row of the window, 32 equal-width blocks encode the low
32 bits of the microsecond timestamp:

- White block = bit 1
- Black block = bit 0
- Bits are ordered MSB-first (block 0 = bit 31)

This stripe survives frames where the 7-segment OCR fails due to motion blur
or glare because it has a much larger per-bit area and does not require
character recognition — just luma threshold per block.

`measure_p2p_latency.py` reads the stripe via `read_timecode_stripe()` as a
fallback when `pytesseract` returns an empty string.

## Reading the timecode from a 240 fps frame (Python)

```python
import cv2
import numpy as np

def read_stripe(frame: np.ndarray, y: int = 0, stripe_h: int = 4) -> int:
    """Read the 32-bit binary stripe at y from an RGB/BGR frame."""
    h, w = frame.shape[:2]
    block_w = w // 32
    val = 0
    for bit in range(32):
        x = bit * block_w + block_w // 2  # sample middle of block
        luma = frame[y : y + stripe_h, x, :].mean()
        val = (val << 1) | (1 if luma > 128 else 0)
    return val

# Full pipeline:
# cap = cv2.VideoCapture("captures/2026-05-08-wifi6e.mp4")
# ok, frame = cap.read()
# host_us = read_stripe(frame[0:8, 0:frame.shape[1]])   # host monitor ROI
# ipad_us = read_stripe(frame[0:8, frame.shape[1]//2:]) # iPad ROI
# latency_us = ipad_us - host_us
```

See `../measure_p2p_latency.py` for the full CLI tool.

## Vsync synchronisation

### Linux — DRM vblank

A background thread calls `DRM_IOCTL_WAIT_VBLANK` (relative, sequence=1) on
`/dev/dri/card0` (override with `$DRM_DEVICE`). On each return it sets an
`AtomicBool` that the `winit` main loop polls to trigger a repaint.

On Wayland, the compositor's `frame` callback gates the actual scanout; the DRM
thread may fire slightly earlier. This introduces at most one scanout-line of
jitter — negligible at 240 fps camera speed.

Permission requirements: your user must be in the `video` group or `card0` must
be readable without root.

```bash
sudo usermod -aG video "$USER"   # then log out and back in
```

### Windows — DXGI WaitForVBlank

A background thread calls `IDXGIOutput::WaitForVBlank()` in a loop on the
primary display's DXGI output. This blocks until the physical vblank interrupt,
independent of whether we are rendering through DXGI or softbuffer.

### macOS

Not explicitly supported — `winit` will use the `CVDisplayLink` via the
platform backend and `softbuffer` will pace the repaints via `CADisplayLink`.
The timecode will be vsync-locked but the accuracy may differ slightly.

## Why this is a separate crate from `host/`

The `host/` workspace's `iextendd` build matrix must be clean on CI runners
that lack a display server (headless Ubuntu). Adding `winit` + `softbuffer` +
`drm` + `windows-rs` to that workspace would:

1. Pull platform display APIs into the `iextendd` dependency tree.
2. Require headless CI to have DRM permissions or a virtual display.
3. Slow down the main workspace build on every PR.

By keeping `screen-timecode` as a standalone crate, it is built only when the
bench operator needs it — not in CI.

The `host/crates/iextendd/tests/support/timecode_source.rs` module re-implements
the same digit-painting logic as pure in-memory RGBA operations (no display)
for use in the synthetic latency CI test.

## Build requirements

| Platform | Requirements |
|---|---|
| Linux | `libdrm-dev`, `libwayland-dev` or X11 headers; Rust 1.88+ |
| Windows | Windows SDK 10.0.22000+; Rust 1.88+ |
| macOS | Xcode Command Line Tools; Rust 1.88+ |
