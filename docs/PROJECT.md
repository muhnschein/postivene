# Postivene

*A native SailfishOS client for [Delta Chat](https://delta.chat).*

## What this is

Delta Chat is a chatmail messenger with end-to-end encryption, no phone
number and no central operator.

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
  majority of regular, non-technical people. That also means no developer mode,
  no SSH'ing to fix small things, no community repos. Most importantly, that
  means [dogfooding](https://en.wikipedia.org/wiki/Eating_your_own_dog_food).
  Lots and lots of dogfooding - and nagging Jolla about the things that the 
  platform is still missing. This is the way.

## Architecture

```
QML / Silica UI
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

## Platform baseline

- Toolchain floor **Rust 1.75.0, Qt 5.6.3** — what Sailfish ships.
- Built against the **5.2** SDK, the Jolla Phone's baseline. Anything older
  is out of scope: a binary from a newer SDK can call symbols an older
  phone lacks, and that is accepted rather than worked around. Harbour
  requires it too -- it rejects a binary that does not link
  `__libc_start_main@GLIBC_2.34`, which only a 5.x glibc provides.
- `aarch64` and `armv7hl` for devices; `i486`/`x86_64` for the emulator.
- Account storage is the core's own, pinned inside the sailjail grant at
  `$XDG_DATA_HOME/postivene/postivene/accounts` (`POSTIVENE_ACCOUNTS_DIR`
  overrides).

## What is missing

In order of what matters:

1. **Harbour-readiness.** Every rule a source tree can answer is now a
   mandatory CI gate (`ci/harbour-check.sh`, `HARBOUR.md`), and the real
   validator runs against each built RPM. One blocker remains, and it is not
   fixable here: the bundled `deltachat-rpc-server` is a second ELF
   executable, which Harbour permits nowhere.
2. **Blocking** outside a request; a media grid on the group and contact
   pages; add-as-second-device and restore-from-backup.
3. **Message polish**: avatars on bubbles, an unread divider, and a way
   to react with an emoji the quick row does not offer.
4. **Recording a voice message, and taking a picture.** Sending every
   kind of attachment works; making one does not. QML has no audio
   recorder on Qt 5.6 -- `harbour-whisperfish` wrote its own against
   gstreamer -- so a voice note needs native code, an `unsafe` exception
   and the `Microphone` permission. The camera is already granted for the
   QR scanner, so a picture is the smaller step.
5. **Running a webxdc app.** Sending one already works, but is not shown in
   the GUI; running it needs `Sailfish.WebView`, the `WebView` permission
   and the webxdc bridge.