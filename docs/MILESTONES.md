# Milestones & status

Tracks `docs/SCOPE.md` §6. Unchecked items are unproven, not merely
unfinished.

## 1. Core bring-up

- [x] No Sailfish-specific build is needed: upstream's release binaries are
      statically linked against musl, so the target's glibc is irrelevant.
      The armv7l build is hard-float, matching Sailfish `armv7hl`.
- [x] `scripts/fetch-rpc-server.sh` fetches v2.53.0 for
      `aarch64`/`armv7hl`/`x86_64` from upstream's PyPI wheels, sha256-pinned
      on both wheel and extracted binary.
- [x] `get_system_info` and the chat/message/account wire shapes verified
      against the real binary, offline
      (`deltachat-jsonrpc/tests/real_server.rs`).
- [x] The accounts directory is pinned to
      `$XDG_DATA_HOME/postivene/postivene/accounts` -- inside the directory
      sailjail grants the app
      (`POSTIVENE_ACCOUNTS_DIR` overrides). Without it the core stores state
      relative to the launcher's working directory.
- [x] Both ARM binaries pass the integration suite under `qemu-user`. That
      does not prove behaviour against Sailfish's kernel, but static musl
      binaries use a conservative syscall ABI.
- [x] Runs on real Sailfish hardware. The emulator is still unused;
      `docs/DEVICE-CHECKS.md` records what has been seen on a phone.

## 2. Headless RPC shim

- [x] `rust/deltachat-jsonrpc`: process spawn, JSON-RPC 2.0 correlation,
      event stream via the `get_next_event_batch` long poll. No Qt
      dependency.
- [x] `rust/postivene-shim`: `DeltaChatCore` as a `QObject`, chat/message/
      account list models.
- [x] The async round trip proven under a real offscreen Qt event loop
      (`tests/smoke.rs`): a `qt_method` spawns work on a background runtime
      and the result returns through `queued_callback` on the Qt thread.
- [x] Validated on a device: every behaviour in `docs/DEVICE-CHECKS.md`
      has now been walked there.

## 3. Minimal UI

- [x] `chat_list`/`message_list` models populated from the core;
      `refresh_chat_list`, `open_chat`, `send_text`.
- [x] `rust/postivene-app`: registers the context properties and loads
      `qml/postivene.qml` into a `QQuickView`.
- [x] `qml/`: root, cover, chat list, conversation.
- [x] Live updates: both list pages re-fetch on the relevant core events.
- [x] Two constraints the QML depends on, each covered by a test:
      `qmetaobject` exposes Rust identifiers to QML verbatim
      (`tests/qml_naming.rs`), and Qt 5.6 connects only `onFoo:` bindings
      (`tests/qml_syntax.rs`).

## 4. Full messaging UI

- [x] Account bootstrap and resume at startup; `account_list` model.
- [x] Message delivery states (`DC_STATE_*`), ticks on outgoing messages,
      unread-badge clearing via `marknoticed_chat`.
- [ ] Media, contact pages, notifications, background sync and suspend
      handling. The last two need a real Sailfish target.
- [x] Starting a conversation: contacts, new chat, groups and invites.
      What is left of it is in `docs/GAP-ANALYSIS.md`, which is canonical
      for feature status.

## 5. Onboarding & security UX

- [x] Encryption indicators: `is_encrypted` per chat, `show_padlock` per
      message, both verified against the real core.
- [x] Onboarding rebuilt on the core's current API (`docs/ONBOARDING.md`):
      `create_profile` via `add_transport_from_qr`,
      `create_profile_with_email` via `add_or_update_transport`,
      `check_invite`, `cancel_ongoing`, `list_transports`, and a typed
      `configure_progress` signal. `WelcomePage`, `CreateProfilePage` and
      `EmailLoginPage` replace the address-and-password `SetupPage`.
      Both create paths reuse an existing unconfigured account.
- [x] `secure_join`: a pasted `https://i.delta.chat/...` invite is
      classified by the core and followed, in both directions
      (`docs/GAP-ANALYSIS.md`).
- [ ] Reading an invite off the camera, and showing one's own as a QR
      image. The link form of every payload already works, so this is
      polish rather than a missing capability.
- [ ] Add as second device; restore from backup.

## 6. Packaging & release

- [x] `rpm/postivene.spec`, `postivene.desktop`, placeholder icons.
      Modeled on Whisperfish's spec, simplified: no vendored C/C++ deps.
- [x] Real `mb2` builds inside the Platform SDK for **aarch64** and
      **i486**, producing real RPMs (`docs/SDK-BUILD.md`). Spec constraints
      that came out of it:
  - `%{_target_cpu}`, not `%{_arch}`, for the bundled server path.
  - `QT_INCLUDE_PATH`/`QT_LIBRARY_PATH` exported in `%build`: qttypes cannot
    exec the target `qmake` under sb2.
  - i486 bundles no server; upstream ships no 32-bit x86 musl binary.
  - `-j1` for cargo inside sb2: parallel cargo deadlocks there.
  - cargo must not be passed `--target` under sb2, and the host triple's
    linker must be scratchbox2's `host-gcc`.
  - `Cargo.lock` stays v3; the SDK's cargo 1.75 cannot read v4.
  - `--with vendor` mode for OBS/Chum, which build without network.
  - `Exec=postivene`: the invoker does not honour an `Exec=env FOO=bar`
    wrapper, so the bundled server path is a fallback inside the binary.
- [x] A device RPM exists: `postivene-0.1.0-1.aarch64.rpm`, aarch64 ELF
      linked against the target's Qt 5.6.3, with a bundled server that still
      passes the integration suite under `qemu-aarch64`.
- [x] `.github/workflows/rpm.yml` builds that package unattended on a
      GitHub runner from the same Platform SDK image, in about six
      minutes. Dispatch it with an arch and an SDK version, or push a
      `v*` tag. What a container run needs that a chroot run does not is
      in `docs/SDK-BUILD.md`.
- [x] Real `BuildRequires` resolution against zypper. A runner reaches
      the Jolla repositories, so `mb2` installs `rust`, `cargo` and
      `rust-std-static` into the target itself, and none of the
      hand-reconstructed rustlib in `docs/SDK-BUILD.md` is needed there.
- [ ] `armv7hl` build. The workflow takes it as an input; nobody has run
      it.
- [ ] An `sfdk` or OBS build specifically. The CI path drives `mb2`
      directly, which is not the tooling OBS runs.
- [ ] Chum/OpenRepos submission.
- [ ] Harbour is not reachable from this packaging and will not be without
      a decision first. Jolla's validator takes one executable per package
      (plus private `.so`s), and the bundled `deltachat-rpc-server` is a
      second one; three further gaps are independent of it, including a
      `Requires:` the validator rejects outright and `libQt5Widgets.so.5`,
      which `qttypes` links unconditionally. `docs/HARBOUR.md` reads the
      rules off the validator's source, prices the four ways out, and
      settles the licensing question they raise (no, linking the core in
      does not mean going back to MPL-2.0).

## Sailfish OS 5.2 readiness

No public 5.2 SDK build target exists yet. CI builds against 4.6.0.13 by
default and takes `sfos_version` to select another; build against the
oldest release worth supporting, since a binary from a newer SDK can call
symbols an older phone does not have. Sailfish keeps binary compatibility
across point releases, but that is untested here. Sailfish's Rust is 1.75.0 and its Qt is
5.6.3; `qttypes` requires Qt >= 5.6.

## Verification

`make check` runs everything below from a clean checkout: no phone, account,
or network. See `docs/ENGINEERING.md`.

- Lints at `clippy::pedantic`, `missing_docs` and `unsafe_code` denied, plus
  two banned methods in `rust/clippy.toml`.
- `msrv`: the workspace compiles on Rust 1.75.0 with warnings denied.
- The real page files load and run in tests against `tests/silica-stubs/`.
  The stubs imitate no layout, and pages using Silica's `EnterKey` cannot be
  loaded at all.
- Contract tests pin the JSON-RPC call sequence of each onboarding action.
- `real_core.rs` distinguishes a request the real core could not decode from
  one it could not deliver.

## Environment constraint

No Sailfish SDK, emulator or OBS access in the environment this is
developed in. CI has an SDK (`.github/workflows/rpm.yml`), which is where
device packages come from; anything still needing an emulator or OBS is
left unchecked above rather than assumed. A device has been available, and
`docs/DEVICE-CHECKS.md` -- what has actually been seen on a phone -- is now
walked in full.
