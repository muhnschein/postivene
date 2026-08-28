# Postivene — Project Scope

*A native SailfishOS client for Delta Chat*

**Status:** Draft scope / pre-implementation
**Working name:** Postivene (Finnish *posti* "mail" + *vene* "boat" → "mail boat")
**Document type:** Scope & non-goals

---

## 1. Summary

Postivene is a native SailfishOS chat application built on the Delta Chat
core. Delta Chat is an email-based ("chatmail") messenger with end-to-end
encryption that requires no phone number and no central service operator.
No native SailfishOS client exists today; users currently rely on the
Android app under Alien Dalvik/AppSupport or the desktop client via Flatpak,
both of which are workarounds rather than integrated Sailfish apps.

The project's thesis is deliberately narrow: **do not build a messenger,
build a SailfishOS UI on top of an existing, production-grade messenger
engine.** All protocol, cryptography, and storage logic is delegated to the
upstream Delta Chat core. Postivene contributes the presentation layer, the
Sailfish platform integration, and the packaging.

---

## 2. Goals

- Provide a Silica-native (Qt/QML) Delta Chat client that feels like a
  first-class Sailfish app, not a port.
- Integrate with the Delta Chat core through its **JSON-RPC interface**
  (`deltachat-rpc-server` over stdio), the same integration surface the
  official desktop client now uses.
- Support the core daily-messaging workflow: account setup, chat list,
  conversation view, sending/receiving text and basic media, and encryption
  status visibility.
- Handle background message reception within Sailfish's power-management
  constraints as well as the platform reasonably allows.
- Ship as an installable RPM through community channels (Chum / OpenRepos),
  built reproducibly via the Sailfish SDK and, ideally, OBS.

## 3. Non-goals (explicitly out of scope)

These are called out deliberately to prevent scope creep. Several are
tempting precisely because adjacent projects contain the code.

- **Reimplementing any protocol logic.** No IMAP/SMTP/MIME handling, no
  Autocrypt, no encryption implementation. If protocol code is being
  written, the core dependency is being misused. This is the single most
  important boundary in the project.
- **Hand-written C FFI bindings.** The core's CFFI still exists, but the
  sanctioned and lower-maintenance path is JSON-RPC. Wrapping the C API by
  hand would reinvent a now-secondary integration style.
- **A push-notification service.** There is no Delta Chat push
  infrastructure available to arbitrary third-party clients. Background
  delivery is IMAP IDLE plus a Sailfish background process — not a push
  relay. Building server-side push infrastructure is out of scope.
- **Multi-protocol / bridging.** No Signal, Matrix, XMPP, or other transport
  in the same app. Single-purpose.
- **Plain-email chats as a first-class way in.** A contact added by
  address alone cannot be encrypted to, so writing to one sends ordinary
  mail. The intent is that chats start from invites. Note this is intent,
  not yet fact: "New Contact" still opens an address-and-name form, and
  `docs/GAP-ANALYSIS.md` tracks replacing it with the invite page. The
  core marks a chat's encryption either way, which the chat list shows,
  because such a chat can also arrive from elsewhere.
- **Borrowing Whisperfish's cryptographic / Signal stack.** Whisperfish is
  valuable as an *architectural* reference (see §5) but solves a different
  protocol problem; its `libsignal-service-rs` / protocol code is not a
  dependency here.
- **A desktop or web build.** Upstream already ships those. Postivene is a
  Sailfish app only.
- **Broad backwards compatibility with old Sailfish releases** at launch.
  Pick one modern baseline (see §7) and expand only if justified.
- **Server operation.** Postivene does not run or recommend operating a
  chatmail server; it is a client that works with existing servers and the
  default onboarding server.

## 4. What can be reused (do not start from scratch)

| Component | Reuse as | Notes |
|---|---|---|
| `deltachat-rpc-server` (from `chatmail/core`) | **Bundled binary dependency** | Provides the entire Delta Chat core over JSON-RPC/stdio. Prebuilt `armv7l` and `aarch64` Linux binaries exist upstream, but a generic-Linux build may not run cleanly on Sailfish; plan to build against Sailfish targets. |
| JSON-RPC API + OpenRPC spec | **Interface contract** | Method and event surface is auto-generated from the Rust core. The TypeScript `@deltachat/jsonrpc-client` and Python `deltachat-rpc-client` are useful references for the full API even though neither is used directly. |
| Whisperfish | **Architectural template** | Closest prior art: Rust + QML on Sailfish, with solved problems around Rust cross-compilation, RPM packaging via `sfdk`, OBS builds, and background/device-suspend handling. Study its build tooling and structure, not its Signal code. |
| `qmetaobject-rs` | **Optional direct dependency** | Exposes Rust structs to QML as `QObject`s with compile-time `QMetaObject` generation. Natural fit if the RPC client shim is written in Rust. |
| Existing open-source Silica apps | **UI convention reference** | For idiomatic page flow (setup wizard, list view, thread view) so the result feels native. |

## 5. Architecture (intended)

```
+-------------------------------------------------------------+
|  Postivene (SailfishOS RPM)                                 |
|                                                             |
|  +-----------------------------+   +---------------------+  |
|  |  QML / Silica UI            |   |  Background service |  |
|  |  - chat list (ListView)     |   |  - keeps RPC alive  |  |
|  |  - conversation view        |   |  - IMAP IDLE / sync |  |
|  |  - account setup wizard     |   |  - suspend handling |  |
|  +--------------+--------------+   +----------+----------+  |
|                 |  models / signals           |            |
|  +--------------v-----------------------------v----------+  |
|  |  Integration shim (Rust via qmetaobject-rs, or C++)  |  |
|  |  - JSON-RPC client over stdio                        |  |
|  |  - event loop -> Qt queued signals                   |  |
|  |  - QAbstractListModel adapters for chats/messages    |  |
|  +--------------------------+---------------------------+  |
|                             | JSON-RPC (stdio)            |
|  +--------------------------v---------------------------+  |
|  |  deltachat-rpc-server  (bundled binary, subprocess)  |  |
|  |  = the entire Delta Chat core                        |  |
|  +------------------------------------------------------+  |
+-------------------------------------------------------------+
```

Key decisions:

- **The shim talks JSON-RPC to a spawned `deltachat-rpc-server` subprocess.**
  This keeps the integration surface small and stable and mirrors the
  desktop client's own migration away from CFFI/Node transport.
- **Core events run off the main thread** and are marshalled to the Qt main
  thread via queued signals. Chat and message lists are exposed as
  `QAbstractListModel` subclasses for `SilicaListView`.
- **Background reception** relies on the core's IMAP IDLE plus a Sailfish
  background process, respecting device-suspend rules. This is expected to
  be the hardest platform-integration problem and should be prototyped early.

## 6. Milestones

1. **Core bring-up.** Cross-compile / obtain `deltachat-rpc-server` for
   Sailfish target architectures; confirm it runs on-device and answers a
   `get_system_info` health check over stdio.
2. **Headless RPC shim.** Spawn the server, complete a JSON-RPC round trip,
   receive events, from within a minimal Sailfish harness.
3. **Minimal UI.** Single account, single conversation: read and send text.
   No notifications, no polish.
4. **Full messaging UI.** Chat list, multiple accounts, media, background
   sync and notifications via Sailfish's notification APIs.
5. **Onboarding & security UX.** Account creation on the default server,
   QR-based contact/verification setup, encryption-state indicators.
6. **Packaging & release.** RPM via `sfdk`, OBS build, distribution through
   Chum / OpenRepos.

## 7. Platform baseline & constraints

- **Target one modern Sailfish baseline first** (a release providing a
  recent Rust toolchain, e.g. Sailfish 4.5+/4.6 era), rather than chasing
  backward compatibility from day one.
- **Build engine:** Rust compilation on Sailfish requires the Docker build
  engine; the VirtualBox build engine does not support it. Expect
  single-threaded Rust builds to be slow on first compile due to a known
  `sb2` limitation.
- **Architectures:** `aarch64` and `armv7hl` for devices; `i486`/`x86_64`
  optionally for the emulator.
- **Storage/encryption:** rely on the core's own account storage; if
  additional local encryption is desired, follow the SQLCipher pattern used
  by comparable Sailfish apps.

## 8. Licensing

The Delta Chat core is copyleft (GPL-family). Linking against it and
bundling `deltachat-rpc-server` has license implications for the combined
work; Postivene should adopt a compatible license and confirm obligations
before distribution. This should be settled before the first public release,
not after.

## 9. Open questions

- Does an upstream prebuilt `deltachat-rpc-server` run on Sailfish as-is, or
  is a Sailfish-target build strictly required? (Resolve in Milestone 1.)
- Rust-based shim (`qmetaobject-rs`) vs. C++ shim — decide based on team
  familiarity and how much model/glue code is expected.
- How reliably can background reception wake the device within Sailfish's
  power rules? (Prototype in Milestone 2–4; this carries the most risk.)
- Community coordination: is there interest from the SailfishOS forum
  thread contributors and/or the Delta Chat team in co-maintaining?

---

*Naming note:* alternatives to "Postivene" considered — **Kyyhky**
("dove/pigeon", carrier-pigeon metaphor), **Postikyyhky** ("carrier pigeon"),
and **Venhe** (poetic/archaic variant of *vene*). Only verified Finnish
vocabulary was used; no invented compounds.
