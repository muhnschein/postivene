# postivene ⛵💬

A native SailfishOS client for Delta Chat.

> ⚠️ **Work in progress:** postivene is under very active development.
> Expect things to break.
>
> 🤖 **Vibe-coded:** Much of this project was developed using AI. If that
> provenance troubles you, use something else. That being said, this project
> is largely a pretty wrapper around official, handmade code from the upstream
> Delta Chat project. More on that below.
>
> 📱 **Modern SailfishOS-only:** postivene currently targets the 
> Jolla Phone 2026 and nothing else. No effort is made to accommodate older
> targets. [Buy a Jolla Phone 2026](https://commerce.jolla.com/) and support
> European-made alternatives. 👊🇪🇺🔥

## Overview

postivene is a Silica/QML application built on top of the Delta Chat core.

It implements no messaging protocol of its own. Every piece of
IMAP/SMTP/MIME/crypto logic lives in upstream's `deltachat-rpc-server`, which
postivene spawns as a subprocess and drives over JSON-RPC on stdio. postivene
itself *never* touches a mail server or a key.

## Limitations

- No protocol or crypto reimplementation — ever. We never roll our own crypto.
- Chats via regular email or any non-chatmail servers.
- Voice and video calling, for the time being.
- No OpenRepos/Chum.

See the non-goals in [`docs/PROJECT.md`](docs/PROJECT.md) for more information.

## Documentation

- [`docs/PROJECT.md`](docs/PROJECT.md) — project scope and explicit non-goals,
current implementation status, and what is missing
- [`docs/BUILDING.md`](docs/BUILDING.md) — engineering standards, testing, and
how a device RPM is built
- [`docs/HARBOUR.md`](docs/HARBOUR.md) — Jolla's store rules, how CI gates
them, and the two that still block submission

## Building

Device packages need the Sailfish SDK with the **Docker** build engine — the
VirtualBox one cannot compile Rust — and a build target for your device's
architecture.

```
$ git clone https://github.com/muhnschein/postivene.git
$ cd postivene
$ scripts/fetch-rpc-server.sh     # bundled deltachat-rpc-server binaries
$ scripts/build-rpm.sh aarch64    # or: armv7hl; add a version, e.g. 5.0.0.62
```

Everything else needs no phone, account, or network:

```
# Everything CI runs: fmt, clippy, tests, qmllint, packaging checks.
$ make check

# Compile against Sailfish's Rust 1.75 floor.
$ make msrv
```

Host-side Qt5 requirements, the real-core integration tests, OBS/Chum vendor
mode and the toolchain floor are in [`docs/BUILDING.md`](docs/BUILDING.md).

## License

Licensed GPLv3+, see the LICENSE file for details.

Copyright © postivene contributors.