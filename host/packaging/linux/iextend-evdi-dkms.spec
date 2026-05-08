Name:           iextend-evdi-dkms
Version:        1.14.7
Release:        1%{?dist}
Summary:        iExtend virtual display driver (evdi kernel module via DKMS)
License:        GPL-2.0-only
URL:            https://github.com/REPLACE_WITH_OWNER/iExtend
BuildArch:      noarch

# evdi upstream source tarball — pinned to 1.14.7 (the version we have tested).
# The tarball is mirrored in our release assets; the upstream URL is:
# https://github.com/DisplayLink/evdi/archive/refs/tags/v1.14.7.tar.gz
Source0:        evdi-1.14.7.tar.gz

# Packaging helpers and sign script.
Source1:        dkms.conf
Source2:        sign-evdi-module.sh

Requires:       dkms
Requires:       make
Requires:       gcc
Requires:       kernel-devel
# mokutil is optional (only needed on SecureBoot systems).
Recommends:     mokutil

# The iextend daemon loads libevdi via dlopen — no build-time dep.
# Keep this package GPL-only; the daemon is a separate Apache-2.0 RPM.

%description
Provides the evdi (Extensible Virtual Display Interface) kernel module as a
DKMS source package.  evdi is maintained by DisplayLink and licensed GPL-2.0.

This package is separate from the iextend daemon (Apache-2.0) to keep the
GPL license boundary clean: the daemon links libevdi only at runtime via
dlopen, so it does not inherit the GPL.

%prep
%setup -q -n evdi-%{version}

%install
install -d %{buildroot}/usr/src/iextend-evdi-%{version}
cp -r . %{buildroot}/usr/src/iextend-evdi-%{version}/
install -Dm644 %{SOURCE1} %{buildroot}/usr/src/iextend-evdi-%{version}/dkms.conf
install -Dm755 %{SOURCE2} %{buildroot}/usr/lib/iextend/sign-evdi-module.sh

%files
/usr/src/iextend-evdi-%{version}/
/usr/lib/iextend/sign-evdi-module.sh

%post
# Register with DKMS.
dkms add    -m iextend-evdi -v %{version} --force 2>/dev/null || true
dkms build  -m iextend-evdi -v %{version} -k "$(uname -r)" 2>&1 | tail -5 || true
dkms install -m iextend-evdi -v %{version} -k "$(uname -r)" --force 2>&1 | tail -5 || true
modprobe evdi initial_device_count=1 2>/dev/null || true

# SecureBoot guidance (identical logic to Debian postinst).
KEY_DIR=/var/lib/iextend
mkdir -p "$KEY_DIR" && chmod 700 "$KEY_DIR"
if [ ! -f "$KEY_DIR/MOK.priv" ] || [ ! -f "$KEY_DIR/MOK.der" ]; then
    openssl req -new -x509 -newkey rsa:2048 \
        -keyout "$KEY_DIR/MOK.priv" \
        -outform DER -out "$KEY_DIR/MOK.der" \
        -days 36500 -nodes \
        -subj "/CN=iExtend evdi module signing key/" \
        2>/dev/null && chmod 600 "$KEY_DIR/MOK.priv" || true
fi
if mokutil --sb-state 2>/dev/null | grep -q "SecureBoot enabled"; then
    echo ""
    echo "  SecureBoot is enabled.  Run this before rebooting:"
    echo "    sudo mokutil --import /var/lib/iextend/MOK.der"
    echo ""
fi

%preun
if [ "$1" = "0" ]; then
    modprobe -r evdi 2>/dev/null || true
    dkms remove -m iextend-evdi -v %{version} --all 2>/dev/null || true
fi

%changelog
* Thu May 08 2026 iExtend Contributors <dev@iextend.app> - 1.14.7-1
- Initial package of evdi 1.14.7 for iExtend
