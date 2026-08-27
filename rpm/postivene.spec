# Sailfish/OBS packaging for Postivene.
#
# Modeled on real-world prior art for Rust+Qt5/QML Sailfish apps -- notably
# Whisperfish's rpm/harbour-whisperfish.spec (see docs/SCOPE.md's §4
# reference to Whisperfish as an architectural/build-tooling template) --
# but simplified: Postivene has no C/C++ vendored dependencies of its own
# (no sqlcipher, no protobuf, etc.), just the Rust workspace under rust/,
# the qml/ tree, and a *separately obtained* deltachat-rpc-server binary
# (scripts/fetch-rpc-server.sh, see vendor/deltachat-rpc-server/).
#
# Two build modes:
#   sfdk build                  -- crates are fetched from crates.io by the
#                                  build engine, which has network access.
#   sfdk build -- --with vendor -- crates come from a pre-generated
#                                  vendor.tar.xz; required on OBS/Chum,
#                                  which build without network. Generate
#                                  the two extra sources with
#                                  scripts/vendor-crates.sh.
#
# NOT YET DONE, tracked in docs/MILESTONES.md:
#   - an actual `sfdk build` / OBS build of this spec (no Sailfish SDK has
#     been available in any environment this repo was developed in)
#   - Harbour-store compliance (sailjail permissions, aarch64/armv7hl
#     signing, etc.) if that channel is ever pursued instead of/alongside
#     Chum or OpenRepos

%bcond_with vendor

Name:       postivene
Summary:    Native SailfishOS client for Delta Chat
Version:    0.1.0
Release:    1
License:    MPL-2.0
Group:      Qt/Qt
URL:        https://github.com/muhnschein/postivene
Source0:    %{name}-%{version}.tar.gz
%if %{with vendor}
Source1:    vendor.tar.xz
Source2:    vendor.toml
%endif

Requires:   sailfishsilica-qt5 >= 0.10.9
Requires:   libsailfishapp-launcher
Requires:   sailfish-version >= 4.5.0

# Sailfish ships Rust 1.75.0 (sailfishos/rust); rust/Cargo.lock is kept in
# the v3 lockfile format because Cargo only learned to read v4 in 1.78.
# `rust-std-static` is the virtual provide of the native std library; the
# *cross* std for the target triple comes from
# rust-std-static-%{rusttarget}, which the SDK target normally already has.
# If a build fails with "can't find crate for `std`", install it into the
# tooling, the way Whisperfish's build docs describe:
#   sfdk tools exec <tooling> zypper in rust-std-static-%{rusttarget}
BuildRequires:  rust >= 1.75
BuildRequires:  rust-std-static >= 1.75
BuildRequires:  cargo >= 1.75
# qmetaobject-rs compiles C++ glue (the cpp/cpp_build crates) against Qt.
BuildRequires:  gcc-c++
BuildRequires:  git
BuildRequires:  desktop-file-utils
BuildRequires:  pkgconfig(Qt5Core)
BuildRequires:  pkgconfig(Qt5Qml)
BuildRequires:  pkgconfig(Qt5Quick)

%ifarch %arm
%define rusttarget armv7-unknown-linux-gnueabihf
%endif
%ifarch aarch64
%define rusttarget aarch64-unknown-linux-gnu
%endif
%ifarch %ix86
# NOTE: upstream publishes no 32-bit x86 deltachat-rpc-server build, so the
# i486 emulator target cannot be packaged; use the x86_64 emulator.
%define rusttarget i686-unknown-linux-gnu
%endif
%ifarch x86_64
%define rusttarget x86_64-unknown-linux-gnu
%endif

%define builddir rust/target/%{rusttarget}/release
%define appdatadir %{_datadir}/%{name}
%define appexecdir %{_libexecdir}/%{name}

%description
Postivene is a Silica/QML SailfishOS client for Delta Chat. It contributes
only the UI, Sailfish platform integration, and packaging; all
IMAP/SMTP/MIME/encryption logic is delegated to a bundled
deltachat-rpc-server binary, spoken to over JSON-RPC on stdio. See
docs/SCOPE.md in the source tree for the full project scope and, just as
importantly, its non-goals.

%prep
%setup -q -n %{name}-%{version}
%if %{with vendor}
# vendor.tar.xz contains rust/vendor/ (cargo vendor output); vendor.toml is
# the [source] replacement stanza cargo prints when generating it.
%setup -q -T -D -a 1 -n %{name}-%{version}
mkdir -p rust/.cargo
install -m 644 %{SOURCE2} rust/.cargo/config.toml
%endif

%build
rustc --version
cargo --version

# Cross-compiling Rust under Sailfish's scratchbox2 (sb2) requires the
# Docker build engine (the VirtualBox build engine does not support it);
# see docs/SCOPE.md §7. Under sb2 the accelerated rustc would otherwise
# emit host (x86) code, so tell it the real target explicitly -- same
# mechanism Whisperfish's spec uses (see the xulrunner-qt5.spec comment it
# cites). Unverified against a real SDK build so far; expect to iterate
# here once one is available.
export SB2_RUST_TARGET_TRIPLE=%{rusttarget}
cargo build \
    --release \
    --locked \
%if %{with vendor}
    --offline \
%endif
    --target %{rusttarget} \
    --manifest-path rust/Cargo.toml \
    --package postivene-app

%install
rm -rf %{buildroot}

install -Dm 755 %{builddir}/postivene \
    %{buildroot}%{_bindir}/postivene

# QML UI, installed under our own app-private data dir (not /usr/bin) so
# postivene-app's qml_dir() lookup (POSTIVENE_QML_DIR env var, then this
# path, then a source-tree-relative fallback for local dev) finds it.
(cd qml && find . -type f -exec \
    install -Dm 644 "{}" "%{buildroot}%{appdatadir}/qml/{}" \; )

# Bundled deltachat-rpc-server: see vendor/deltachat-rpc-server/ and
# scripts/fetch-rpc-server.sh. Installed under a private libexec dir rather
# than %{_bindir} so it can't be picked up as a generic system
# "deltachat-rpc-server" by anything else that happens to look for one on
# PATH -- postivene-app defaults to this exact path.
#
# %{_target_cpu}, *not* %{_arch}: rpm canonicalises every armv7h* target to
# the arch name "arm" (rpm's installplatform: `armv7h*) CANONARCH=arm`), so
# %{_arch} would look for vendor/deltachat-rpc-server/arm/ while
# scripts/fetch-rpc-server.sh writes armv7hl/. %{_target_cpu} keeps the
# Sailfish arch names (armv7hl, aarch64, x86_64) that the script uses.
install -Dm 755 vendor/deltachat-rpc-server/%{_target_cpu}/deltachat-rpc-server \
    %{buildroot}%{appexecdir}/deltachat-rpc-server

# MPL-2.0 obligation for bundling deltachat-rpc-server's Executable Form:
# recipients must be told how to obtain its Source Code Form. See
# docs/LICENSING.md and vendor/deltachat-rpc-server/SOURCE.md.
install -Dm 644 vendor/deltachat-rpc-server/SOURCE.md \
    %{buildroot}%{appdatadir}/vendor/deltachat-rpc-server/SOURCE.md
install -Dm 644 LICENSE \
    %{buildroot}%{appdatadir}/LICENSE
install -Dm 644 docs/LICENSING.md \
    %{buildroot}%{appdatadir}/docs/LICENSING.md

desktop-file-install \
    --dir %{buildroot}%{_datadir}/applications \
    postivene.desktop

install -Dm 644 icons/86x86/postivene.png \
    %{buildroot}%{_datadir}/icons/hicolor/86x86/apps/postivene.png
install -Dm 644 icons/108x108/postivene.png \
    %{buildroot}%{_datadir}/icons/hicolor/108x108/apps/postivene.png
install -Dm 644 icons/128x128/postivene.png \
    %{buildroot}%{_datadir}/icons/hicolor/128x128/apps/postivene.png
install -Dm 644 icons/172x172/postivene.png \
    %{buildroot}%{_datadir}/icons/hicolor/172x172/apps/postivene.png
install -Dm 644 icons/256x256/postivene.png \
    %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/postivene.png

%files
%defattr(-,root,root,-)
%{_bindir}/postivene
%{appdatadir}
%{appexecdir}/deltachat-rpc-server
%{_datadir}/applications/%{name}.desktop
%{_datadir}/icons/hicolor/*/apps/%{name}.png
