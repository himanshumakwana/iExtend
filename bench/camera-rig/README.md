# iExtend bench camera rig

Physical 240 fps camera setup for measuring true photon-to-photon latency.
This is the only measurement path that captures what the user actually sees and
feels — synthetic CI catches code regressions; the camera rig proves the
latency budget against the real display stack.

## Camera options

In increasing order of cost and measurement fidelity:

| Camera | FPS | Resolution | Cost | Notes |
|---|---|---|---|---|
| iPhone 15 Pro / 16 Pro | 240 | 1080p slo-mo | ~$0 (if you have one) | Easiest to set up; rolling shutter |
| GoPro Hero 12 | 240 | 1080p | ~$400 | Rolling shutter; decent in daylight |
| Sony RX0 II | 960 | 720p | ~$600 | Near-global shutter; excellent |
| Raspberry Pi HQ + IMX296 GS | 240 | 1.6 MP | ~$200 | Full global shutter; DIY friendly |
| Phantom Veo 410 | 500–2500 | 1280×800 | ~$50 000 | Sub-ms fidelity; only if budget allows |

**Recommendation:** iPhone 15 Pro at 240 fps covers the iExtend latency range
(10–30 ms) with roughly ±2 ms per-frame uncertainty — sufficient for
pre-release sign-off. Use a global-shutter camera for fine-grained sub-5 ms
work.

## Jig requirements

1. **Tripod or fixed mount** holding the camera level with both screens, 40–60 cm
   from the nearest screen surface.
2. **Side-by-side layout:** host monitor on the left, iPad on the right, with
   the timecode corner of each facing the camera seam. See "Why side-by-side"
   below.
3. **Anti-glare:** matte black backdrop behind the screens, diffused side
   lighting (no direct overhead). Anti-reflective coating on the iPad screen
   helps significantly.
4. **Both timecodes in the same frame** with at least 160×80 px per timecode
   region at the camera's output resolution.

## Why side-by-side (not over/under)

At 240 fps with a rolling-shutter camera, the electronic shutter scans from
top to bottom over the course of one frame period (~4.2 ms). If the host
monitor is above the iPad (or vice versa), the two timecodes are captured at
different instants within the same frame — this introduces a systematic bias of
up to ±4 ms, depending on which screen is higher.

Side-by-side placement ensures both ROIs are at the same vertical scanline
position, so the rolling-shutter offset cancels out perfectly.

## Capture protocol

1. **Prepare the host:** run `screen-timecode --digit-height 100` from
   `bench/camera-rig/screen-timecode/`.
2. **Prepare iExtend:** start `iextendd`, connect the iPad, verify the iPad's
   display is mirroring/extending. The iPad should show the same timecode
   (delayed by the pipeline latency) in its top-left corner.
3. **Align the camera:** confirm both timecodes are sharp and in frame. Take a
   test still (`--max-frames 1`) and verify ROI placement.
4. **Record at 240 fps for 30 s** — that is ~7,200 frames.  Name the file with
   the date and setup:
   ```
   captures/2026-05-08-wifi6e-rc1.mp4
   ```
5. **Run the analysis:**
   ```bash
   python measure_p2p_latency.py captures/2026-05-08-wifi6e-rc1.mp4 \
       --out results/2026-05-08-wifi6e-rc1.csv \
       --host-roi 20,20,160,80 \
       --ipad-roi 980,20,160,80 \
       --method both
   ```
6. **Inspect the output:**
   ```bash
   # Quick percentile summary (printed at end of analysis run):
   #   p50=14200 us  p95=21800 us  p99=28900 us  n=7198
   
   # For a histogram:
   python3 -c "
   import pandas as pd, matplotlib.pyplot as plt
   df = pd.read_csv('results/2026-05-08-wifi6e-rc1.csv').dropna(subset=['latency_us'])
   df.latency_us.hist(bins=100)
   plt.xlabel('latency (us)'); plt.ylabel('frames')
   plt.title('iExtend photon-to-photon Wi-Fi 6E')
   plt.savefig('results/2026-05-08-wifi6e-rc1-hist.png')
   "
   ```

## Pass/fail bar

| Budget | Wi-Fi 6E | USB-C |
|---|---|---|
| p50 | ≤ 14 ms | ≤ 10 ms |
| p99 | ≤ 30 ms | ≤ 18 ms |

If either threshold is exceeded, triage before releasing.

## Calibration

To verify ROI placement before a real capture:

1. Run `screen-timecode` on the host **without** iExtend running.
2. Capture 1 second at 240 fps.
3. Run with `--method stripe --max-frames 240`.
4. Check `latency_us` in the output CSV: should be `None` for the iPad ROI
   (no timecode there yet) and a valid value for the host ROI.
5. If host_us is `None` for most frames, the ROI is misaligned — adjust
   `--host-roi`.

## File structure

```
bench/camera-rig/
├── README.md               — this file
├── requirements.txt        — pip dependencies
├── measure_p2p_latency.py  — main analysis tool
├── screen-timecode/        — Rust binary that paints the timecode
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── vsync_linux.rs
│       └── vsync_windows.rs
├── captures/               — (gitignored) raw camera captures
└── results/                — (gitignored) analysis CSVs and histograms
```

## Dependencies

```bash
pip install -r requirements.txt

# Tesseract OCR binary (needed for --method digits or both):
sudo apt-get install tesseract-ocr     # Ubuntu/Debian
brew install tesseract                  # macOS
```
