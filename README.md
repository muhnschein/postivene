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
qml/                   Silica UI: pages, cover, main.qml.
rpm/                   Sailfish packaging (spec file, desktop entry).
vendor/                Bundled deltachat-rpc-server binary + provenance
                       (fetched, not built by this repo's own CI here).
docs/                  Scope, architecture notes, licensing analysis.
```

## Building

Postivene targets the Sailfish OS SDK (`sfdk`/`mb2`) for real device/RPM
builds; see `rpm/postivene.spec`. The `rust/deltachat-jsonrpc` crate has no
Sailfish/Qt dependency and can be built and tested with a plain host Rust
toolchain:

```sh
cd rust
cargo test -p deltachat-jsonrpc
```

`rust/postivene-shim` additionally requires Qt5 dev packages
(`qtbase5-dev`, `qtdeclarative5-dev` on Debian/Ubuntu-family hosts) for
local iteration outside the Sailfish SDK; the SDK is still required for the
final Sailfish-target build and packaging.

## License

MPL-2.0, matching the upstream Delta Chat core. See [`LICENSE`](LICENSE)
and [`docs/LICENSING.md`](docs/LICENSING.md) for the reasoning and the
obligations that come with bundling `deltachat-rpc-server`.

## Non-goals

Most importantly: **no protocol/crypto reimplementation**. See
[`docs/SCOPE.md`](docs/SCOPE.md) §3 for the full list.
