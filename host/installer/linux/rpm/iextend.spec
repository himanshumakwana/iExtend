Name:           iextend
Version:        0.1.0
Release:        1%{?dist}
Summary:        iPad as a wireless second screen
License:        Apache-2.0
URL:            https://iextend.example
Source0:        %{name}-%{version}.tar.gz

# Build dependencies
BuildRequires:  cargo >= 1.75
BuildRequires:  rust >= 1.75
BuildRequires:  pkgconfig(libpipewire-0.3)
BuildRequires:  pkgconfig(libdrm)
BuildRequires:  protobuf-compiler

# Runtime dependencies
Requires:       iextend-evdi-dkms >= 0.1
Requires:       systemd
Requires:       pipewire >= 0.3.40

%description
iExtend turns an iPad into a wireless second monitor over Wi-Fi or
USB-C, with Apple Pencil pressure and tilt forwarded as a Wacom-class
stylus.

The iextendd daemon manages the virtual display (via EVDI kernel module)
and the WebRTC video pipeline.  The iextend-tray system-tray application
handles pairing and display configuration.

Note: the EVDI kernel module (iextend-evdi-dkms) must be installed and
built before use.  On SecureBoot systems, MOK enrollment is required;
the package's %%post scriptlet will guide you through that one-time step.

%prep
%autosetup

%build
cd host && cargo build --release --workspace

%install
# Binaries
install -Dm755 host/target/release/iextendd \
    %{buildroot}%{_bindir}/iextendd
install -Dm755 host/target/release/iextend-tray \
    %{buildroot}%{_bindir}/iextend-tray

# systemd user unit
install -Dm644 host/installer/linux/debian/iextendd.service \
    %{buildroot}%{_userunitdir}/iextendd.service

# XDG autostart entry
install -Dm644 host/installer/linux/debian/iextend.desktop \
    %{buildroot}%{_sysconfdir}/xdg/autostart/iextend.desktop

# MOK enrollment helper
install -Dm755 host/installer/linux/dkms-mok-enrollment.sh \
    %{buildroot}%{_datadir}/iextend/dkms-mok-enrollment.sh

%post
%systemd_user_post iextendd.service

# Run MOK enrollment helper if evdi-dkms key is already present
MOK_HELPER=%{_datadir}/iextend/dkms-mok-enrollment.sh
if [ -x "$MOK_HELPER" ]; then
    "$MOK_HELPER" || true
fi

echo ""
echo "iExtend installed. Log out and back in to start the tray,"
echo "or: systemctl --user start iextendd.service"
echo ""

%preun
%systemd_user_preun iextendd.service

%files
%license LICENSE
%doc README.md
%{_bindir}/iextendd
%{_bindir}/iextend-tray
%{_userunitdir}/iextendd.service
%{_sysconfdir}/xdg/autostart/iextend.desktop
%{_datadir}/iextend/dkms-mok-enrollment.sh

%changelog
* Fri May 08 2026 iExtend <maintainers@iextend.example> - 0.1.0-1
- Initial release.
- EVDI kernel module managed via iextend-evdi-dkms companion package.
- SecureBoot MOK enrollment guided via post-install helper.
- Systemd user service (iextendd.service) enabled globally at install.
