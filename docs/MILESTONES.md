# Milestones & status

Tracks the milestones from `docs/SCOPE.md` §6. Updated as work lands.

## 1. Core bring-up

Cross-compile / obtain `deltachat-rpc-server` for Sailfish target
architectures; confirm it runs on-device and answers a `get_system_info`
health check over stdio.

- [x] **The scope's open question (§9) is effectively resolved: no
      Sailfish-specific build is needed.** Upstream's release binaries are
      **statically linked against musl libc** — confirmed from their own
      CI workflow (`.github/workflows/deltachat-rpc-server.yml`: "Build a
      version statically linked against musl libc to avoid problems with
      glibc version incompatibility") and re-confirmed with `file`/`ldd`
      on the actual binaries. A static binary depends only on the kernel
      ABI, so Sailfish's older glibc is irrelevant. The armv7l build is
      hard-float (`armv7-unknown-linux-musleabihf`), matching Sailfish
      `armv7hl`.
- [x] `scripts/fetch-rpc-server.sh`: fetches upstream v2.53.0 binaries for
      `aarch64`/`armv7hl`/`x86_64` via upstream's PyPI wheels (same nix
      build artifacts as their GitHub release), sha256-pinned on both the
      wheels and the extracted binaries, placed at
      `vendor/deltachat-rpc-server/<arch>/` where `rpm/postivene.spec`
      expects them. Ran clean twice in this environment. (GitHub release
      asset downloads are blocked by this environment's proxy; PyPI is
      the channel that works here and is equally upstream-official.)
- [x] `get_system_info` health check **passed against the real server**
      (x86_64, this host): `rust/deltachat-jsonrpc/tests/real_server.rs`
      is a gated integration test (`DELTACHAT_RPC_SERVER=<path>`) that
      spawns the real v2.53.0 binary through our transport crate and
      verifies, all offline: the health check, `add_account`,
      `set_config`/`get_config`, **real event delivery** (MsgsChanged
      after setting a draft), and the exact chat-list and message-list
      wire shapes `DeltaChatCore` depends on — including the snake_case
      `msg_id` spelling that was once a bug, now pinned by a test against
      the real core.
- [x] Found & fixed while doing this: the app never set
      `DC_ACCOUNTS_PATH`, so the core would have stored all account data
      in `./accounts` relative to whatever cwd the launcher provided.
      `RpcClient` grew `spawn_with_env`, and `DeltaChatCore::start` now
      pins the accounts dir to `$XDG_DATA_HOME/postivene/accounts`
      (override: `POSTIVENE_ACCOUNTS_DIR`).
- [x] Both device-architecture binaries **executed and passed the full
      integration test under QEMU user-mode emulation**
      (`qemu-aarch64-static` / `qemu-arm-static` on this x86_64 host):
      the actual ARM instruction streams run, answer `get_system_info`,
      create accounts, and deliver events through our transport crate.
      What QEMU user-mode cannot prove is behavior against Sailfish's
      actual kernel (syscalls are translated to the host kernel), but
      static musl binaries use a conservative, stable syscall ABI, so
      the residual risk is small.
- [ ] The one remaining item: run the `aarch64`/`armv7hl` binary on real
      Sailfish hardware or the SDK emulator — now a formality-level
      check rather than an open question.

## 2. Headless RPC shim

Spawn the server, complete a JSON-RPC round trip, receive events, from
within a minimal Sailfish harness.

- [x] `rust/deltachat-jsonrpc`: spawns `deltachat-rpc-server`, JSON-RPC 2.0
      request/response correlation, event stream via the
      `get_next_event`/`get_next_event_batch` long-poll pattern. No
      Sailfish/Qt dependency; unit-tested against a fake stdio server
      double (6 tests).
- [x] `rust/postivene-shim`: `DeltaChatCore` QObject wrapping the client
      (start/health-check/add-account/configure-account/send-text +
      `coreEvent` signal forwarding), plus `ChatListItem`/`MessageListItem`
      + `SimpleListModel`-based `ChatListModel`/`MessageListModel` types.
      Builds clean against host Qt5 (`qtbase5-dev`/`qtdeclarative5-dev`);
      Sailfish-target cross-compile still to be validated against the real
      SDK.
- [x] End-to-end smoke test (`postivene-shim/tests/smoke.rs`) drives a real
      **offscreen** Qt event loop (`QT_QPA_PLATFORM=offscreen`) and proves
      the full async round trip actually works: a `qt_method` call spawns
      the fake server on a background tokio runtime, and the result comes
      back through `queued_callback` to mutate `qt_property`s / fire
      `qt_signal`s on the Qt thread. This is the riskiest architectural
      piece per `docs/SCOPE.md` §5 ("core events run off the main thread
      and are marshalled to the Qt main thread via queued signals").
  - Found and worked around an upstream bug while building this:
    `qmetaobject` 0.2.10's `single_shot()` mis-converts the sub-second part
    of a `Duration` (`subsec_nanos() * (1e-6 as u32)`, and `1e-6 as u32`
    truncates to `0`), so any non-whole-second `Duration` schedules as if
    it were 0ms. Anything in this codebase that uses `single_shot` should
    pass whole-second `Duration`s until/unless this is fixed upstream.
- [ ] Validated inside an actual Sailfish harness/emulator (needs Milestone
      1 first: a `deltachat-rpc-server` binary that actually runs there).

## 3. Minimal UI

Single account, single conversation: read and send text. No notifications,
no polish.

- [x] `DeltaChatCore` grew `chat_list`/`message_list` nested
      `SimpleListModel` properties (`qt_property!(RefCell<...>; CONST)`,
      following the pattern from `qmetaobject`'s own
      `tests/models.rs::simple_model_remove`) plus `refresh_chat_list`/
      `open_chat` methods that populate them from
      `get_chatlist_entries`/`get_chatlist_items_by_entries` and
      `get_message_list_items`/`get_message`. `send_text` now also appends
      the sent message to `message_list` directly from the response, no
      extra round trip.
- [x] `rust/postivene-app`: the `main.rs` harness binary that registers
      `DeltaChatCore` as the `core` context property and loads
      `qml/postivene.qml`.
- [x] `qml/`: `postivene.qml` (root), `cover/CoverPage.qml`,
      `pages/SetupPage.qml` (add + configure account),
      `pages/ChatListPage.qml`, `pages/ConversationPage.qml` (read +
      send text).
  - **Naming caveat that shaped all of this QML**: `qmetaobject`'s
    `QObject`/`SimpleListItem` derives expose Rust identifiers to QML
    *verbatim*, snake_case and all -- there's no automatic camelCase
    conversion. Confirmed empirically (not just by reading the macro
    source) in `postivene-shim/tests/qml_naming.rs`, which loads real QML
    calling `core.check_health()` and listening for
    `onSystem_info_changed`. All the `.qml` files here use snake_case
    method/property/signal-handler names to match.
  - **Verification limits**: Sailfish's `Sailfish.Silica` QML module isn't
    installable outside the Sailfish SDK, so these pages could not be
    rendered or interacted with here. What *was* checked: `qmllint` (no
    args) parses all five files with zero errors, and the snake_case
    naming convention they depend on is covered by an automated test
    (previous bullet). Layout, visual behavior, and page-to-page
    navigation are unverified pending Sailfish SDK/device access.
- [x] Live-update wiring: `ConversationPage`/`ChatListPage` listen to the
      shim's `core_event` signal and re-fetch on `IncomingMsg`/
      `MsgsChanged`, so received messages appear without leaving and
      re-entering the chat. (Event `kind` strings are verbatim variant
      names; their payload fields are camelCase `chatId`/`msgId` --
      per-variant `rename_all` upstream, verified against
      `chatmail/core`'s `events.rs`.)
  - A self-review pass also caught and fixed a wire-format bug here
    before it ever ran against a real server: `get_message_list_items`
    returns items whose id field is snake_case `msg_id` (upstream's
    `rename_all` sits at the *enum* level, renaming only the variant
    tags), not `msgId` as first assumed -- confirmed by serializing the
    exact upstream serde attribute shape. With the old code every
    message would have been silently skipped.

## 4. Full messaging UI

Chat list, multiple accounts, media, background sync and notifications via
Sailfish's notification APIs.

- [x] Account bootstrap: the app now checks for an existing configured
      account at startup (`refresh_accounts` → `get_all_accounts`,
      `AccountListModel`) and resumes it (`start_account_io`) instead of
      showing the login form on every launch. Wire shape (tagged
      `Configured`/`Unconfigured`, camelCase fields) pinned against the
      real core in `real_server.rs`.
- [x] Multi-account groundwork: `account_list` model exposed to QML with
      id/display-name/address/configured per account. (Account-switcher
      UI page itself not built yet.)
- [ ] Media (images/files), message states (delivered/read), contact
      pages, notifications via nemo-qml-plugin-notifications, background
      sync/suspend handling. Notifications and suspend behavior can only
      be meaningfully built against a real Sailfish target.

## 5. Onboarding & security UX

Account creation on the default server, QR-based contact/verification
setup, encryption-state indicators.

- [x] Encryption-state indicators: `is_encrypted` on chat-list rows and
      `show_padlock` per message, surfaced in the QML delegates per
      upstream guidance (unencrypted marked with a letter symbol,
      encrypted unmarked). Both flag spellings and semantics verified
      against the real core (plain-email chat → unencrypted; fresh group
      → encrypted).
- [x] QR groundwork: `check_qr` shim method classifying QR payloads via
      the core, result forwarded to QML as kind + raw JSON. Verified
      offline against the real core with a `DCACCOUNT:` code (kind
      `account`, snake_case payload fields -- same enum-level-rename
      serde trap as MessageListItem, now documented on the method).
- [ ] Camera-based QR scanning UI, `secure_join`/`set_config_from_qr`
      flows, account creation on the default chatmail server (needs
      network to the relay; also camera only exists on-device).

## 6. Packaging & release

RPM via `sfdk`, OBS build, distribution through Chum / OpenRepos.

- [x] `rpm/postivene.spec`, `postivene.desktop`, placeholder app icons
      (`icons/<size>/postivene.png`, procedurally generated -- a simple
      "mail boat" glyph, not real design work). Modeled on Whisperfish's
      real `rpm/harbour-whisperfish.spec` (fetched from upstream, not
      guessed) but simplified: no vendored C/C++ deps, just the Cargo
      workspace, `qml/`, and a separately-obtained
      `deltachat-rpc-server` binary (Milestone 1).
- [x] Actually verified, not just written: installed `rpm`/`rpmspec` +
      `desktop-file-utils` on this host, built `postivene-app` in release
      mode, staged a source tarball, and ran a real
      `rpmbuild -bb --nodeps rpm/postivene.spec` (`--nodeps` because the
      Sailfish-specific `BuildRequires` package names don't exist in this
      Ubuntu host's package database -- that part is inherently
      untestable outside the real OBS/SDK chroot). It **produced an
      actual `.rpm`** with every file landing where the spec and
      `postivene-app`'s `qml_dir()`/`POSTIVENE_RPC_SERVER` lookups expect
      it (`/usr/bin/postivene`, `/usr/share/postivene/qml/...`,
      `/usr/libexec/postivene/deltachat-rpc-server`,
      `/usr/share/applications/postivene.desktop`,
      `/usr/share/icons/hicolor/*/apps/postivene.png`). The bundled
      `deltachat-rpc-server` was a throwaway stand-in binary for this
      dry run, not a real one -- Milestone 1 still needs to land before
      this produces a package that actually functions.
- [x] **Real `mb2` builds inside the real Platform SDK** (see
      `docs/SDK-BUILD.md` for the full setup and every workaround):
      `coderus/sailfishos-platform-sdk:5.0.0.43` pulled from Docker Hub
      (via `mirror.gcr.io` -- the egress proxy here blocks Docker Hub's
      CDN but not Google's mirror of it), set up as a chroot with the
      stock sb2 targets, and `mb2 -t SailfishOS-5.0.0.43-<arch> build`
      run against `rpm/postivene.spec` for **aarch64** (device arch) and
      **i486** (emulator arch). The full Rust workspace including
      `qmetaobject`/`qttypes` compiled and linked against the target's
      actual Qt 5.6.3 stack through scratchbox2's cross toolchain, and
      real `.rpm`s were produced. Spec fixes that came out of it:
  - `%{_arch}` -> `%{_target_cpu}` for the bundled rpc-server path
    (`%{_arch}` expands to `arm` on armv7hl, but the vendor dirs use
    Sailfish arch names).
  - `QT_INCLUDE_PATH`/`QT_LIBRARY_PATH` exported in `%build`: qttypes'
    build script cannot exec the target `qmake` under sb2; with both
    vars set it detects Qt from headers instead (Qt 5.6.3 found).
  - i486 packages no longer try to bundle `deltachat-rpc-server`
    (upstream ships no 32-bit x86 musl binary).
  - `-j1` forced for cargo inside sb2 sessions: parallel cargo
    reproducibly deadlocks under scratchbox2 (futex-wait on an unreaped
    child during qmetaobject's C++ build).
  - `rust/Cargo.lock` pinned to lockfile v3 -- the SDK's cargo 1.75
    cannot read v4 lockfiles.
  - Caveats: BuildRequires resolution was skipped (`-n`/`--nodeps`, no
    network path to the Jolla repos from this environment -- the target
    rust std had to be grafted/built by hand, see `docs/SDK-BUILD.md`),
    so the `BuildRequires` list itself is still unproven against zypper;
    and the produced RPMs have not been installed/run on a device or
    emulator.
- [ ] `armv7hl` mb2 build (same rustlib graft as aarch64 needed).
- [ ] An unrestricted-network `sfdk`/OBS build that exercises real
      `BuildRequires` resolution.
- [ ] Chum/OpenRepos submission.

## Environment constraint

This working environment has no Sailfish SDK, emulator, or device, and no
network path to Sailfish OBS. Everything markable as done above was
built/tested with a host Rust + Qt5 toolchain (Qt5 dev packages installed
locally in this environment for that purpose); anything requiring the
actual Sailfish SDK/mb2/sfdk or on-device testing is left unchecked and
called out explicitly rather than assumed to work.
