#!/usr/bin/env bash
# generate-app-icon.sh
# Renders scripts/app-icon-source.svg → ipad/iExtend/Assets.xcassets/AppIcon.appiconset/icon-1024.png
# at 1024×1024. Uses rsvg-convert (librsvg2-bin). Run-once when the icon source changes.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SRC="$SCRIPT_DIR/app-icon-source.svg"
OUT_DIR="$REPO_ROOT/ipad/iExtend/Assets.xcassets/AppIcon.appiconset"
OUT="$OUT_DIR/icon-1024.png"

if ! command -v rsvg-convert >/dev/null 2>&1; then
    echo "error: rsvg-convert not found." >&2
    echo "install with: sudo apt install librsvg2-bin   (Linux)" >&2
    echo "         or: brew install librsvg              (macOS)" >&2
    exit 1
fi

if [ ! -f "$SRC" ]; then
    echo "error: source SVG missing at $SRC" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"
rsvg-convert -w 1024 -h 1024 "$SRC" -o "$OUT"

echo "wrote $OUT"
ls -la "$OUT"
