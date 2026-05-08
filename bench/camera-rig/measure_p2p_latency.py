"""Measure photon-to-photon latency from a 240 fps camera capture.

Input:
    A video file (MP4/MOV) captured by a 240 fps camera showing both the host
    monitor and the iPad screen side-by-side.  Each screen is painting the
    timecode produced by bench/camera-rig/screen-timecode/.

Output:
    A CSV with one row per frame:
        frame_idx, t_camera_us, host_us, ipad_us, latency_us, method

    Where:
        frame_idx   — 0-based frame index in the capture
        t_camera_us — wall-clock microseconds of this frame based on FPS
        host_us     — timecode read from the host monitor ROI (us since proc start)
        ipad_us     — timecode read from the iPad screen ROI
        latency_us  — ipad_us - host_us (positive = iPad lags, as expected)
        method      — "digits", "stripe", or "none" (how the value was read)

Usage:
    python measure_p2p_latency.py CAPTURE.mp4 --out results.csv
    python measure_p2p_latency.py CAPTURE.mp4 --out results.csv \\
        --host-roi 20,20,160,80 --ipad-roi 980,20,160,80 --method both

    Default ROIs assume a 1280×720 side-by-side frame at 240 fps; tune with
    --host-roi / --ipad-roi after inspecting a sample frame:
        ffmpeg -i CAPTURE.mp4 -vframes 1 -q:v 1 first_frame.jpg
        eog first_frame.jpg

Calibration:
    Run with --method stripe to get a baseline.  If the stripe values are stable
    (low std-dev when iExtend is NOT running) then your ROI placement is good.
    If you see random values, widen the ROI or adjust the stripe_h parameter in
    read_timecode_stripe().

Percentiles are printed to stdout at the end of a successful run:
    p50=14000 us  p95=22000 us  p99=29500 us  n=7198

See bench/camera-rig/README.md for the full physical setup procedure.
"""

from __future__ import annotations

import csv
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

import click
import cv2
import numpy as np

try:
    import pytesseract  # type: ignore
    _TESSERACT_OK = True
except ImportError:
    _TESSERACT_OK = False


# ── Region of interest ────────────────────────────────────────────────────────

@dataclass
class Roi:
    x: int
    y: int
    w: int
    h: int

    @classmethod
    def parse(cls, s: str) -> "Roi":
        """Parse 'x,y,w,h' string."""
        parts = [int(v.strip()) for v in s.split(",")]
        if len(parts) != 4:
            raise ValueError(f"ROI must be 'x,y,w,h', got: {s!r}")
        return cls(*parts)

    def crop(self, frame: np.ndarray) -> np.ndarray:
        return frame[self.y : self.y + self.h, self.x : self.x + self.w]


# ── Timecode readers ──────────────────────────────────────────────────────────

def read_timecode_digits(frame: np.ndarray, roi: Roi) -> Optional[int]:
    """OCR the 7-segment digit strip using Tesseract.

    Returns the integer value or None if OCR fails or Tesseract is not installed.
    """
    if not _TESSERACT_OK:
        return None

    crop = roi.crop(frame)
    gray = cv2.cvtColor(crop, cv2.COLOR_BGR2GRAY)
    # Binarise: the digits are black on white, so THRESH_BINARY_INV highlights
    # the segment bars; we pass the threshold image to Tesseract in its natural
    # orientation (black digits on white).
    _, bw = cv2.threshold(gray, 128, 255, cv2.THRESH_BINARY)
    # Scale up 2× so Tesseract has an easier time at 80 px digit height.
    scaled = cv2.resize(bw, (bw.shape[1] * 2, bw.shape[0] * 2),
                        interpolation=cv2.INTER_NEAREST)
    config = "--psm 7 --oem 1 -c tessedit_char_whitelist=0123456789"
    try:
        txt = pytesseract.image_to_string(scaled, config=config)
    except Exception:  # noqa: BLE001
        return None

    digits = "".join(c for c in txt if c.isdigit())
    return int(digits) if digits else None


def read_timecode_stripe(
    frame: np.ndarray,
    roi: Roi,
    stripe_h: int = 4,
    n_bits: int = 32,
) -> Optional[int]:
    """Read the binary stripe from the top `stripe_h` rows of `roi`.

    The binary stripe is painted along the very top edge of the timecode
    window: `n_bits` equal-width blocks, white=1 / black=0, MSB-first,
    encoding the low 32 bits of the microsecond timestamp.

    Returns the integer value (always succeeds if the ROI is non-empty).
    """
    crop = roi.crop(frame)
    if crop.size == 0:
        return None
    # Use only the top stripe_h rows.
    stripe = crop[:stripe_h, :]
    if stripe.size == 0:
        return None
    gray = cv2.cvtColor(stripe, cv2.COLOR_BGR2GRAY)

    w = gray.shape[1]
    bit_w = max(w // n_bits, 1)
    val = 0
    for bit in range(n_bits):
        x0 = bit * bit_w
        x1 = min((bit + 1) * bit_w, w)
        luma = float(np.mean(gray[:, x0:x1]))
        val = (val << 1) | (1 if luma > 128 else 0)
    return val


# ── Main CLI ──────────────────────────────────────────────────────────────────

@click.command()
@click.argument(
    "video",
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
)
@click.option(
    "--out", "out_csv",
    type=click.Path(dir_okay=False, path_type=Path),
    required=True,
    help="Output CSV path.",
)
@click.option(
    "--host-roi", "host_roi_str",
    default="20,20,160,80",
    show_default=True,
    help="ROI for the host monitor timecode: x,y,w,h (pixels in the capture frame).",
)
@click.option(
    "--ipad-roi", "ipad_roi_str",
    default="980,20,160,80",
    show_default=True,
    help="ROI for the iPad screen timecode: x,y,w,h.",
)
@click.option(
    "--method",
    type=click.Choice(["digits", "stripe", "both"]),
    default="both",
    show_default=True,
    help=(
        "digits: Tesseract OCR only.  "
        "stripe: binary stripe only (no Tesseract needed).  "
        "both: try digits first, fall back to stripe."
    ),
)
@click.option(
    "--max-frames", "max_frames",
    type=int,
    default=0,
    help="Stop after this many frames (0 = process all).",
)
@click.option(
    "--stripe-h",
    type=int,
    default=4,
    show_default=True,
    help="Height in pixels of the binary stripe to sample.",
)
def cli(
    video: Path,
    out_csv: Path,
    host_roi_str: str,
    ipad_roi_str: str,
    method: str,
    max_frames: int,
    stripe_h: int,
) -> None:
    """Measure photon-to-photon latency from a side-by-side 240 fps camera capture."""

    h_roi = Roi.parse(host_roi_str)
    i_roi = Roi.parse(ipad_roi_str)

    cap = cv2.VideoCapture(str(video))
    if not cap.isOpened():
        click.echo(f"error: could not open {video}", err=True)
        sys.exit(1)

    fps = cap.get(cv2.CAP_PROP_FPS) or 240.0
    total = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
    click.echo(f"Video: {video.name}  FPS={fps:.1f}  frames={total}")

    if method in ("digits", "both") and not _TESSERACT_OK:
        click.echo(
            "warning: pytesseract not installed — digit OCR disabled; using stripe only.",
            err=True,
        )

    out_csv.parent.mkdir(parents=True, exist_ok=True)

    fieldnames = ["frame_idx", "t_camera_us", "host_us", "ipad_us", "latency_us", "method"]
    rows_written = 0

    with out_csv.open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()

        idx = 0
        while True:
            ok, frame = cap.read()
            if not ok:
                break
            if max_frames > 0 and idx >= max_frames:
                break

            t_camera_us = int(idx / fps * 1_000_000)
            h_us: Optional[int] = None
            i_us: Optional[int] = None
            used_method = "none"

            # ── try digits first (if method in digits|both) ────────────────
            if method in ("digits", "both"):
                h_us = read_timecode_digits(frame, h_roi)
                i_us = read_timecode_digits(frame, i_roi)
                if h_us is not None or i_us is not None:
                    used_method = "digits"

            # ── fall back to stripe (if method in stripe|both) ─────────────
            if (h_us is None or i_us is None) and method in ("stripe", "both"):
                if h_us is None:
                    h_us = read_timecode_stripe(frame, h_roi, stripe_h=stripe_h)
                if i_us is None:
                    i_us = read_timecode_stripe(frame, i_roi, stripe_h=stripe_h)
                used_method = "stripe"

            lat_us: Optional[int] = None
            if h_us is not None and i_us is not None:
                lat_us = i_us - h_us

            writer.writerow({
                "frame_idx": idx,
                "t_camera_us": t_camera_us,
                "host_us": h_us,
                "ipad_us": i_us,
                "latency_us": lat_us,
                "method": used_method,
            })
            rows_written += 1
            idx += 1

            if idx % 240 == 0:
                click.echo(f"  processed {idx} frames ({idx / fps:.1f} s) …", err=True)

    cap.release()
    click.echo(f"Wrote {rows_written} rows to {out_csv}")

    # ── print percentile summary ───────────────────────────────────────────
    try:
        import pandas as pd  # type: ignore
        df = pd.read_csv(out_csv)
        valid = df.dropna(subset=["latency_us"])
        if not valid.empty:
            lats = valid["latency_us"].astype(float)
            click.echo(
                f"  p50={lats.quantile(0.50):.0f} us  "
                f"p95={lats.quantile(0.95):.0f} us  "
                f"p99={lats.quantile(0.99):.0f} us  "
                f"n={len(lats)}"
            )
        else:
            click.echo("  warning: no valid latency rows (all host_us or ipad_us are None)")
    except ImportError:
        click.echo("  (install pandas for percentile summary)")


if __name__ == "__main__":
    cli()
