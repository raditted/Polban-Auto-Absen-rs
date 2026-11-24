%global debug_package %{nil}
%global _build_id_links none

Name:           polban-presensi
Version:        %{app_version}
Release:        1
Summary:        Aplikasi desktop pemantau dan pencatat presensi akademik Polban
License:        MIT
URL:            https://github.com/raditted/Polban-Auto-Absen-rs
BuildArch:      x86_64

AutoReqProv:    no
Requires:       gtk3
Requires:       dbus-libs
Requires:       libxkbcommon
Requires:       libxcb
Requires:       libglvnd-glx
Requires:       libglvnd-egl
Recommends:     libayatana-appindicator-gtk3
Recommends:     pipewire-utils

%description
Aplikasi desktop native untuk login otomatis dan mencatat presensi
akademik di akademik.polban.ac.id/ajar/absen secara lokal dengan
dukungan system tray, notifikasi desktop, dan penyimpanan kredensial
melalui Secret Service sistem operasi.

%install
rm -rf %{buildroot}
install -Dm0755 "%{bin_path}" "%{buildroot}%{_bindir}/polban-absen-gui"
install -Dm0644 "%{desktop_path}" "%{buildroot}%{_datadir}/applications/polban-presensi.desktop"
install -Dm0644 "%{icon_path}" "%{buildroot}%{_datadir}/icons/hicolor/256x256/apps/polban-presensi.png"
install -Dm0644 "%{license_path}" "%{buildroot}%{_datadir}/licenses/polban-presensi/LICENSE"

%post
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q "%{_datadir}/applications" || :
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f "%{_datadir}/icons/hicolor" || :
fi

%postun
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q "%{_datadir}/applications" || :
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f "%{_datadir}/icons/hicolor" || :
fi

%files
%{_bindir}/polban-absen-gui
%{_datadir}/applications/polban-presensi.desktop
%{_datadir}/icons/hicolor/256x256/apps/polban-presensi.png
%license %{_datadir}/licenses/polban-presensi/LICENSE
