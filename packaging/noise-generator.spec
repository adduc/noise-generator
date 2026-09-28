# Built by packaging/build-rpm.sh, which creates both source tarballs.
# The crates are vendored so the build runs offline, as it would in mock.

%global app_id us.jlong.NoiseGenerator

Name:           noise-generator
# Must match Cargo.toml and the newest release in the metainfo file.
Version:        0.1.0
Release:        1%{?dist}
Summary:        Play white, pink, brown and other colored noise

# The app is GPL-3.0-or-later; the rest covers the vendored crates, as
# listed by %%cargo_license_summary during the build. Recheck it there when
# dependencies change.
SourceLicense:  GPL-3.0-or-later
License:        GPL-3.0-or-later AND Apache-2.0 AND MIT AND (MIT OR Apache-2.0) AND Unicode-3.0 AND (Unlicense OR MIT) AND (BSD-2-Clause OR MIT OR Apache-2.0) AND (BSD-3-Clause OR MIT OR Apache-2.0) AND (Zlib OR Apache-2.0 OR MIT)
URL:            https://github.com/adduc/noise-generator
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.xz

ExclusiveArch:  %{rust_arches}

BuildRequires:  cargo-rpm-macros >= 26
BuildRequires:  make
BuildRequires:  pkgconfig(gtk4) >= 4.12
BuildRequires:  pkgconfig(alsa)
BuildRequires:  desktop-file-utils
BuildRequires:  appstream

Requires:       hicolor-icon-theme

%description
A small GTK 4 app that plays continuous background noise, shaped with a
10-band equalizer, with one-click presets for white, pink, brown, blue and
violet noise.

%prep
%autosetup -a1
%cargo_prep -v vendor

%build
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies
%cargo_vendor_manifest

%install
# %%cargo_prep links target/release to the rpm profile's output, so the
# Makefile finds the binary where it expects it.
%make_install PREFIX=%{_prefix}

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/%{app_id}.desktop
appstreamcli validate --no-net %{buildroot}%{_metainfodir}/%{app_id}.metainfo.xml
%cargo_test

%files
%license LICENSE LICENSE.dependencies cargo-vendor.txt
%doc README.md
%{_bindir}/%{name}
%{_datadir}/applications/%{app_id}.desktop
%{_metainfodir}/%{app_id}.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg
%{_datadir}/icons/hicolor/symbolic/apps/%{app_id}-symbolic.svg

%changelog
* Sun Sep 27 2026 John Long <a@88k.us> - 0.1.0-1
- First release
