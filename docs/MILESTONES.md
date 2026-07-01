# Milestones & status

Tracks the milestones from `docs/SCOPE.md` §6. Updated as work lands.

## 1. Core bring-up

Cross-compile / obtain `deltachat-rpc-server` for Sailfish target
architectures; confirm it runs on-device and answers a `get_system_info`
health check over stdio.

- [ ] Fetch/verify upstream prebuilt binaries, or cross-compile for
      `aarch64`/`armv7hl`.
- [ ] Confirmed running on real Sailfish hardware or emulator (requires
      Sailfish SDK / device access not available in this environment).
- [ ] `get_system_info` health check exercised on-device.

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

## 4. Full messaging UI

Chat list, multiple accounts, media, background sync and notifications via
Sailfish's notification APIs.

- [ ] Not started.

## 5. Onboarding & security UX

Account creation on the default server, QR-based contact/verification
setup, encryption-state indicators.

- [ ] Not started.

## 6. Packaging & release

RPM via `sfdk`, OBS build, distribution through Chum / OpenRepos.

- [ ] Not started.

## Environment constraint

This working environment has no Sailfish SDK, emulator, or device, and no
network path to Sailfish OBS. Everything markable as done above was
built/tested with a host Rust + Qt5 toolchain (Qt5 dev packages installed
locally in this environment for that purpose); anything requiring the
actual Sailfish SDK/mb2/sfdk or on-device testing is left unchecked and
called out explicitly rather than assumed to work.
