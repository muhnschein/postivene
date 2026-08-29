# Postivene

*A native SailfishOS client for [Delta Chat](https://delta.chat).*

Postivene (Finnish *posti* "mail" + *vene* "boat" — "mail boat") is a
Silica/QML SailfishOS application built on top of the Delta Chat core. It
does not implement any messaging protocol itself: all IMAP/SMTP/MIME/crypto
logic lives in the upstream `deltachat-rpc-server`, which Postivene spawns
as a subprocess and talks to over JSON-RPC (stdio). Postivene contributes
the Sailfish-native UI, the Qt/QML integration shim, and the packaging.

See [`docs/SCOPE.md`](docs/SCOPE.md) for the full project scope, goals, and
explicit non-goals, and [`docs/MILESTONES.md`](docs/MILESTONES.md) for the
current implementation status.

## Repository layout

```
rust/
  deltachat-jsonrpc/   Transport-only JSON-RPC client (spawns
                       deltachat-rpc-server, request/response correlation,
                       event stream). Pure Rust, no Qt dependency, builds
                       and tests on any host.
  postivene-shim/      Qt/QML integration layer (qmetaobject-rs): exposes
                       DeltaChatCore as a QObject and chat/message lists as
                       QAbstractListModel, for use from QML. Requires Qt5
                       dev headers.
  postivene-app/       main.rs harness: registers DeltaChatCore as a QML
                       context property and loads qml/postivene.qml.
qml/                   Silica UI: postivene.qml (root), cover/, pages/
                       (setup/login, chat list, conversation).
rpm/                   postivene.spec: Sailfish/OBS RPM packaging.
postivene.desktop      Launcher entry. Plain `Exec=postivene`: Sailfish
                       runs silica-qt5 apps through the invoker, which does
                       not reliably honour an `Exec=env FOO=bar app`
                       wrapper, so the server path is worked out in
                       main.rs instead.
icons/                 Placeholder app icons per hicolor size.
vendor/                deltachat-rpc-server binaries per target arch
                       (not committed; run scripts/fetch-rpc-server.sh
                       to populate -- see vendor/deltachat-rpc-server/
                       SOURCE.md for provenance and checksums).
scripts/               fetch-rpc-server.sh: pinned, checksum-verified
                       fetch of upstream's static-musl rpc-server builds.
docs/                  Scope, architecture notes, licensing analysis.
                       GAP-ANALYSIS.md (what is missing), ONBOARDING.md
                       (how Delta Chat onboards a user), ENGINEERING.md
                       (standards).
```

## Building

### Device RPM (Sailfish SDK)

Real device packages must be built inside the Sailfish SDK: the app links
against the target's Qt 5.6 and glibc, so a host-built binary will not run
on a phone. With the SDK installed (**Docker** build engine -- the
VirtualBox one cannot compile Rust) and a build target for your device's
architecture:

```sh
scripts/fetch-rpc-server.sh          # bundled deltachat-rpc-server binaries
scripts/build-rpm.sh aarch64         # or: armv7hl; add a version, e.g. 5.0.0.62
```

`build-rpm.sh` is a thin wrapper around `sfdk -c target=<target> build`,
which runs every SPEC section except `%prep` -- it builds this working tree
in place, so the (gitignored) binaries fetched above are used directly.

For OBS/Chum, which build without network access, generate the offline
crate sources first and switch the spec into vendor mode:

```sh
scripts/vendor-crates.sh
sfdk build -- --with vendor
```

The toolchain floor is the one Sailfish ships: **Rust 1.75**, Qt **5.6**.
`rust/Cargo.lock` is deliberately kept in the v3 lockfile format (Cargo
only learned v4 in 1.78) and the workspace declares `rust-version = 1.75`.

### Checks

`make check` runs what CI runs -- formatting, clippy, tests, qmllint,
packaging checks -- from a clean checkout, with no phone, account, or
network. `make msrv` compiles against Sailfish's Rust 1.75 floor. See
[`docs/ENGINEERING.md`](docs/ENGINEERING.md).

### Host builds (development)

The `rust/deltachat-jsonrpc` crate has no Sailfish/Qt dependency and can be
built and tested with a plain host Rust toolchain:

```sh
cd rust
cargo test -p deltachat-jsonrpc
```

`rust/postivene-shim` and `rust/postivene-app` need Qt5 packages: on
Debian/Ubuntu `qtbase5-dev`, `qtdeclarative5-dev`,
`qtdeclarative5-dev-tools` (for `qmllint`) and `qml-module-qtquick2` (the
QtQuick runtime plugin, which the -dev packages omit):

```sh
cd rust
cargo test --workspace   # includes an offscreen Qt event-loop smoke test
```

To also run the integration test against the **real** Delta Chat core
(offline; no account is configured against any server):

```sh
scripts/fetch-rpc-server.sh   # fetch upstream static binaries into vendor/
cd rust
DELTACHAT_RPC_SERVER=vendor/deltachat-rpc-server/x86_64/deltachat-rpc-server \
    cargo test -p deltachat-jsonrpc --test real_server
```

A relative `DELTACHAT_RPC_SERVER` resolves from the repository root, since
cargo runs integration tests from the package root. The same gate covers
`cargo test -p postivene-shim --test real_core`.

Note: `qml/`'s pages import `Sailfish.Silica`, which only ships with the
Sailfish SDK/target, so `postivene-app` won't actually render anything on
a plain desktop Linux host -- the SDK is required for that and for the
final Sailfish-target build and packaging.

See `docs/MILESTONES.md` for what remains unverified, including the state
of Sailfish OS 5.2 build targets.

## License

MPL-2.0, matching the upstream Delta Chat core. See [`LICENSE`](LICENSE)
and [`docs/LICENSING.md`](docs/LICENSING.md) for the reasoning and the
obligations that come with bundling `deltachat-rpc-server`.

## Non-goals

Most importantly: **no protocol/crypto reimplementation**. See
[`docs/SCOPE.md`](docs/SCOPE.md) §3 for the full list.
