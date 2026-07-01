# Milestones & status

Tracks the milestones from `docs/SCOPE.md` §6. Updated as work lands.

## 1. Core bring-up

Cross-compile / obtain `deltachat-rpc-server` for Sailfish target
architectures; confirm it runs on-device and answers a `get_system_info`
health check over stdio.

- [x] Documented how to fetch/verify upstream prebuilt binaries and how to
      cross-compile for `aarch64`/`armv7hl` (`vendor/deltachat-rpc-server/`,
      `scripts/fetch-rpc-server.sh`).
- [ ] Confirmed running on real Sailfish hardware or emulator (requires
      Sailfish SDK / device access not available in this environment).
- [ ] `get_system_info` health check exercised on-device.

## 2. Headless RPC shim

Spawn the server, complete a JSON-RPC round trip, receive events, from
within a minimal Sailfish harness.

- [x] `rust/deltachat-jsonrpc`: spawns `deltachat-rpc-server`, JSON-RPC 2.0
      request/response correlation, event stream, unit-tested against a
      fake stdio server (no Sailfish/Qt dependency required).
- [x] `rust/postivene-shim`: `DeltaChatCore` QObject wrapping the client,
      marshals core events to queued Qt signals; builds against host Qt5
      (Sailfish-target cross-compile still to be validated against the
      real SDK).
- [ ] Validated inside an actual Sailfish harness/emulator.

## 3. Minimal UI

Single account, single conversation: read and send text. No notifications,
no polish.

- [x] QML skeleton: setup/login page, chat list page, conversation page,
      wired to shim context properties/models.
- [ ] Exercised against a live account on-device/emulator.

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

- [x] `rpm/postivene.spec` draft, `.desktop` entry, packaging notes.
- [ ] Verified with `sfdk build` inside the real Sailfish SDK.
- [ ] Chum/OpenRepos submission.

## Environment constraint

This working environment has no Sailfish SDK, emulator, or device, and no
network path to Sailfish OBS. Everything markable as done above was
built/tested with a host Rust + Qt5 toolchain; anything requiring the
actual Sailfish SDK/mb2/sfdk or on-device testing is left unchecked and
called out explicitly rather than assumed to work.
