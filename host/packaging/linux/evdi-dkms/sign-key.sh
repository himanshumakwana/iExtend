#!/bin/sh
# sign-evdi-module.sh — sign the evdi.ko for SecureBoot.
#
# Usage: sign-evdi-module.sh <kernel-version> <path-to-evdi.ko>
#
# Called from dkms.conf POST_BUILD when MOK.{priv,der} are present.
# Installed to /usr/lib/iextend/sign-evdi-module.sh by the package.

set -e

KVER=${1:?kernel version required}
MODULE=${2:?module path required}
KEY=/var/lib/iextend/MOK.priv
CERT=/var/lib/iextend/MOK.der

if [ ! -f "$KEY" ] || [ ! -f "$CERT" ]; then
    echo "sign-evdi-module.sh: signing key or cert not found — skipping." >&2
    exit 0
fi

# Prefer the kernel's own signing tool if available.
SIGN_FILE=/usr/lib/linux-kbuild-${KVER}/scripts/sign-file
if [ ! -x "$SIGN_FILE" ]; then
    SIGN_FILE=$(find /usr/lib/linux-kbuild-*/scripts/ -name sign-file 2>/dev/null | head -1)
fi

if [ -z "$SIGN_FILE" ] || [ ! -x "$SIGN_FILE" ]; then
    # Fall back to sbsign if available.
    if command -v sbsign >/dev/null 2>&1; then
        sbsign --key "$KEY" --cert "$CERT" --output "$MODULE" "$MODULE"
        echo "sign-evdi-module.sh: signed $MODULE with sbsign"
    else
        echo "sign-evdi-module.sh: no signing tool found — module unsigned." >&2
        echo "  Install linux-headers-${KVER} or sbsigntool and retry." >&2
    fi
    exit 0
fi

"$SIGN_FILE" sha256 "$KEY" "$CERT" "$MODULE"
echo "sign-evdi-module.sh: signed $MODULE (kernel=$KVER)"
