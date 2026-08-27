# Engineering standards

What this project holds itself to, and why each rule exists. The reference
is [clove](https://github.com/muhnschein/clove)'s §9 — pinned toolchain,
pedantic lints, CI-parity `make` targets, tests that drive the real
binaries — adapted where Postivene's shape genuinely differs.

## Where Postivene differs from that reference

Clove is a daemon that parses hostile input off a network, so its testing
centre of gravity is fuzzing and chaos: adversarial bytes, crash
resilience, a fake SAM bridge.

Postivene parses almost nothing. `docs/SCOPE.md` §3 makes protocol and
crypto work a non-goal: all of it lives in the upstream core, and this
repository is a UI shell plus a JSON-RPC transport to a *trusted*
subprocess we spawn ourselves. Fuzzing our own parsers would be fuzzing
`serde_json`.

The failure mode that actually bites here is different, and the whole test
strategy is aimed at it: **we misread what the core says, or we call it
wrongly, and nothing notices until the app is on a phone.** That has
happened repeatedly on this codebase already — a `msgId`/`msg_id` spelling,
a Qt 5.15-only signal-handler syntax that silently never connects on Qt 5.6,
a tokio runtime built on the wrong thread, a Silica window loaded into an
engine that never shows it. Every one was invisible to a host build.

So the equivalents of clove's fuzzing and chaos jobs are:

- **Wire-shape pinning against the real core.** Offline, but against the
  actual pinned `deltachat-rpc-server`, asserting the exact JSON we send
  and the exact field spellings we read back.
- **Protocol-contract tests against a recording fake.** For each UI action,
  assert precisely which RPC calls it makes, in order, with which params —
  so a refactor cannot quietly start calling a deprecated method again.
- **Dialect tests for the Qt version we do not have.** Host Qt is 5.15 and
  accepts syntax that Sailfish's 5.6 silently ignores, so the rules Qt
  cannot enforce for us are enforced as tests.

## The rules

### Toolchains: two, deliberately

`rust-toolchain.toml` pins **1.94.1** so lint results are reproducible and a
new lint in a later stable cannot fail CI out from under a green local run.

The device floor is **1.75.0**, which is what Sailfish ships, declared as
`rust-version` in `rust/Cargo.toml` and enforced by CI's `msrv` job with
warnings denied. A workspace that only builds on modern stable is a
workspace that does not build for the phone. `rust-toolchain.toml` is a
rustup mechanism; the Sailfish SDK's cargo is not rustup-managed and
ignores it.

`rust/Cargo.lock` stays at lockfile **v3**: cargo learned v4 in 1.78, and a
`cargo update` on a modern host rewrites the file silently. `ci/check-lockfile.sh`
fails the build instead of letting the SDK discover it days later.

### Lints

Workspace-level, so a bare `cargo clippy` fails the same way CI does:
`clippy::all` and `clippy::pedantic` at deny, `unwrap_used`/`expect_used`
denied outside tests, `missing_docs` denied, `unsafe_code` denied.

`unsafe_code` is **deny**, not `forbid`, for one reason: the Qt harness
tests need `env::set_var` before Qt initialises, and a local `allow` with a
stated justification is better than a test suite that cannot run. Every
exception in this repository is at the narrowest possible scope and says
why in a comment above it.

`rust/clippy.toml` bans two methods outright, both because this project has
already been bitten by them on device and neither reproduces on a host:

- `tokio::runtime::Runtime::new` — building a runtime on the Qt main thread
  panics on the Sailfish aarch64 build. Everything goes through
  `CoreRuntime`, which owns a thread of its own.
- `qmetaobject::single_shot` — qmetaobject 0.2.10 truncates the sub-second
  part of a `Duration` to zero. Whole seconds are safe; the ban makes the
  next person read the note instead of rediscovering it.

### Testing

The layers, cheapest first. Every one of them runs from a clean checkout
with `make check`; nothing needs a phone, an account, or a network.

1. **Transport unit tests** against a fake stdio server: request/response
   correlation, out-of-order replies, error propagation, event batching.
2. **Protocol-contract tests**: a recording fake core that journals every
   request, so a test can assert the exact call sequence a UI action
   produces.
3. **Qt event-loop tests** under `QT_QPA_PLATFORM=offscreen`: the async
   round trip from a `qt_method` through a background runtime and back onto
   the Qt thread via `queued_callback`.
4. **QML load tests** against stub Silica components: the real page files,
   loaded and driven headlessly, asserting navigation and what they call on
   the core.
5. **Static QML dialect tests**: the Qt 5.6 rules host Qt will not enforce.
6. **Real-core integration** (`--test real_server`, `--test real_core`):
   gated on `DELTACHAT_RPC_SERVER`, offline, against the pinned binary.
7. **Packaging checks**: the spec parses, the desktop entry validates, the
   shell scripts are clean.

Aspiration, tracked and not gated: test code volume exceeds source volume.

What none of this reaches, and what therefore stays explicitly unproven
until someone runs it on hardware: Silica's real rendering and layout,
notifications, background sync and device suspend, and the on-device
behaviour of the packaged RPM. `docs/MILESTONES.md` says so per item rather
than implying coverage that does not exist.

### Documentation

`missing_docs` is denied, and `cargo doc` runs with `-D warnings` in CI, so
a link to a renamed item is a build failure. Module docs explain *why*.
Every deviation — an `allow`, a workaround, a bug worked around upstream —
carries the reasoning at the point of deviation, because the person who
needs it is reading the code, not the commit log.
