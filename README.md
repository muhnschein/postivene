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
docs/                  Scope, architecture notes, licensing analysis.
```

Not yet present (see `docs/MILESTONES.md` for what's next): the Sailfish
RPM packaging (spec file, desktop entry) and the vendored/fetched
`deltachat-rpc-server` binary.

## Building

Postivene targets the Sailfish OS SDK (`sfdk`/`mb2`) for real device/RPM
builds; see `docs/MILESTONES.md` for what that still requires. The
`rust/deltachat-jsonrpc` crate has no Sailfish/Qt dependency and can be
built and tested with a plain host Rust toolchain:

```sh
cd rust
cargo test -p deltachat-jsonrpc
```

`rust/postivene-shim` and `rust/postivene-app` additionally require Qt5
dev packages (`qtbase5-dev`, `qtdeclarative5-dev` on Debian/Ubuntu-family
hosts) for local iteration outside the Sailfish SDK:

```sh
cd rust
cargo test --workspace   # includes an offscreen Qt event-loop smoke test
```

Note: `qml/`'s pages import `Sailfish.Silica`, which only ships with the
Sailfish SDK/target, so `postivene-app` won't actually render anything on
a plain desktop Linux host -- the SDK is required for that and for the
final Sailfish-target build and packaging.

## License

MPL-2.0, matching the upstream Delta Chat core. See [`LICENSE`](LICENSE)
and [`docs/LICENSING.md`](docs/LICENSING.md) for the reasoning and the
obligations that come with bundling `deltachat-rpc-server`.

## Non-goals

Most importantly: **no protocol/crypto reimplementation**. See
[`docs/SCOPE.md`](docs/SCOPE.md) §3 for the full list.
