# Postivene

*A native SailfishOS client for [Delta Chat](https://delta.chat).*

What the project is, what it deliberately is not, and where it currently
stands. Build and test procedure is in [`BUILDING.md`](BUILDING.md).

## What this is

Delta Chat is an email-based ("chatmail") messenger with end-to-end
encryption, no phone number and no central operator.

The thesis is narrow: **do not build a messenger, build a SailfishOS UI on
top of one.** Protocol, cryptography and storage are the upstream core's.
Postivene contributes the presentation layer, the platform integration and
the packaging, and aspires to ships to Jolla's Harbour app store.

## What this isn't

- **Reimplementing any protocol logic** — no IMAP/SMTP/MIME, no Autocrypt,
  no encryption. If protocol code is being written, the core dependency is
  being misused. This is the most important boundary in the project.
- **Hand-written C FFI bindings.** The CFFI exists; JSON-RPC is the
  sanctioned, lower-maintenance path.
- **A push-notification service.** No Delta Chat push infrastructure is
  available to third-party clients.
- **Plain-email chats.**
- **Multi-protocol bridging**, a **desktop or web build**, and **running a
  chatmail server**. Single-purpose client only.
- **Old Sailfish releases.** One modern baseline, expanded only for future
  Jolla products. [Buy a Jolla Phone 2026](https://commerce.jolla.com/) and
  support European-made alternatives. 👊🇪🇺🔥
- **Shipping via OpenRepos/Chum.** We need to improve this platform and get
  it to a point where it is competitive. That means appealing to a broad
  majority of regular, non-technical people. That means also no developer mode,
  no SSH'ing to fix small things, no community repos. Most importantly, that
  means [dogfooding](https://en.wikipedia.org/wiki/Eating_your_own_dog_food).
  Lots and lots of dogfooding - and nagging Jolla about the things that the 
  platform is still missing. This is the way.

## Architecture

```
QML / Silica UI  +  background service (planned)
        |  models / signals
Rust shim (qmetaobject-rs): JSON-RPC client, event loop -> Qt queued
signals, QAbstractListModel adapters for chats/messages/accounts
        |  JSON-RPC over stdio
deltachat-rpc-server (bundled binary, subprocess) = the entire core
```

- **The shim spawns the server as a subprocess.** This keeps the integration
  surface small and stable, and mirrors the desktop client's own migration
  away from CFFI. The OpenRPC spec is the interface contract.
- **Core events run off the main thread**, marshalled to the Qt main thread
  via queued signals.
- **Background reception** relies on IMAP IDLE plus a Sailfish background
  process. It is the hardest platform problem and the largest open risk.

## Platform baseline

- Toolchain floor **Rust 1.75.0, Qt 5.6.3** — what Sailfish ships.
- Build against the oldest release worth supporting: a binary from a newer
  SDK can call symbols an older phone lacks. CI defaults to 4.6.0.13. No
  public 5.2 target exists yet.
- `aarch64` and `armv7hl` for devices; `i486`/`x86_64` for the emulator.
- Account storage is the core's own, pinned inside the sailjail grant at
  `$XDG_DATA_HOME/postivene/postivene/accounts` (`POSTIVENE_ACCOUNTS_DIR`
  overrides).

## What works

Upstream's release binaries are statically linked against musl, so no
Sailfish-specific core build is needed; `scripts/fetch-rpc-server.sh`
fetches v2.53.0 sha256-pinned, and the wire shapes are verified against
the real binary offline.

On top of that: the chat list (unread badges, timestamps, avatars,
encryption/pin/mute marks, context menu, search across chats/contacts/
messages, archive, contact requests, multiple profiles), the conversation
view (bubbles, quotes, delivery marks, day separators, image previews,
attachments, reply/copy/delete/resend), onboarding rebuilt on the core's
current transport API, `secure_join` invites in both directions, encryption
indicators, foreground notifications, and the cover.

Packaging is real: `mb2` builds produce `harbour-postivene-0.1.0-1.aarch64.rpm`,
and `.github/workflows/rpm.yml` builds it unattended on a GitHub runner in
about six minutes.

## What is missing

In order of what matters:

1. **Harbour-readiness.** Every rule a source tree can answer is now a
   mandatory CI gate (`ci/harbour-check.sh`, `HARBOUR.md`), and the real
   validator runs against each built RPM. Two blockers remain, neither
   fixable here:
   - The binary links `libQt5Widgets.so.5`, which Harbour does not allow.
     `qmetaobject-rs` builds its QML engine on `QApplication` rather than
     `QGuiApplication` and `qttypes` links Widgets unconditionally; both
     are still that way upstream, so it takes a patch to each crate. This
     one constrains every Sailfish app built on qmetaobject.
   - The bundled `deltachat-rpc-server` is a second ELF executable, which
     Harbour permits nowhere. The clean fix is upstream's `libdeltachat.so`,
     whose `dc_jsonrpc_*` API carries the same protocol, but it has no ARM
     prebuilt and cannot use the static-musl build that makes the current
     binary portable.

   Also unproven: a run under `sailjail` on a device, exercising every
   permission-dependent path. Store assets and a privacy policy are not
   started.
2. **A background service, and suspend handling.** Messages arrive only
   while the app is open and awake, which is the one thing standing between
   this and a client someone could rely on. Notifications inherit the same
   limit. Restarting `deltachat-rpc-server` after it dies belongs here
   rather than after: a supervised service that dies and stays dead is worse
   than one that was never there, because nobody is watching the screen to
   see the banner.
3. **Camera QR scanning**, and showing one's own invite as a QR image. The
   link form of every payload already works, so this is polish.
4. **Loading a translation.** The catalog is real and `ci/packaging-lint.sh`
   fails if it drifts from the `qsTr()` calls, but `QTranslator` is not
   bound by qmetaobject 0.2.10. That means a `cpp!` block, an `unsafe`
   exception to a workspace lint, and C++ build machinery in a second crate,
   in a build environment `BUILDING.md` already documents as fragile. Worth
   deciding deliberately rather than in passing.
5. **Group member management, contact profile pages, blocking** outside a
   request; add-as-second-device and restore-from-backup.
6. **Message polish**: avatars on bubbles, voice messages and audio,
   reactions, drafts, an unread divider, paging for long histories, and
   sending attachments.

Also open: no `sfdk` or OBS build specifically, since CI drives `mb2` 
directly; icons are placeholders.