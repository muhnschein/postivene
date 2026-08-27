# Engineering standards

The reference is [clove](https://github.com/muhnschein/clove)'s §9: pinned
toolchain, pedantic lints, CI-parity `make` targets, tests that drive the
real binaries.

## Comments and documentation

One sentence where one will do. A comment states what is true now and why.
It is not a changelog, a bug report, or a story about how the code got
here — that belongs in git history. Delete a comment rather than update it
into a history of its own subject.

If the reasoning needs a paragraph, it is a design note in `docs/`, and the
comment is a pointer to it.

## Where this differs from clove

Clove parses hostile network input, so it leans on fuzzing and chaos
testing. Postivene parses almost nothing: protocol and crypto are the
core's (`docs/SCOPE.md` §3), and the subprocess we talk to is one we
spawned.

The failure mode here is misreading the core's JSON, or calling it wrongly,
with nothing noticing until the app is on a phone. The tests aim at that:

- **Wire-shape pinning** against the real `deltachat-rpc-server`, offline.
- **Protocol-contract tests** asserting which calls each UI action makes.
- **Dialect tests** for Qt 5.6 rules that host Qt 5.15 accepts silently.

## Toolchains

`rust-toolchain.toml` pins 1.94.1 so lint results are reproducible. The
device floor is 1.75.0, what Sailfish ships, enforced by CI's `msrv` job
with warnings denied. `rust-toolchain.toml` is a rustup mechanism; the
Sailfish SDK's cargo ignores it.

`rust/Cargo.lock` stays at v3: cargo learned v4 in 1.78, and a `cargo
update` on a modern host rewrites it silently. `ci/check-lockfile.sh`
catches that.

## Lints

Workspace-level, so a bare `cargo clippy` fails the way CI does:
`clippy::all` and `pedantic` at deny, `unwrap_used`/`expect_used` denied
outside tests, `missing_docs` and `unsafe_code` denied.

`unsafe_code` is deny rather than forbid because the Qt harness tests need
`env::set_var` before Qt initialises. Every exception is at the narrowest
scope and says why.

`rust/clippy.toml` bans two methods that have already caused device-only
failures: `tokio::runtime::Runtime::new` (must go through `CoreRuntime`)
and `qmetaobject::single_shot` (truncates sub-second `Duration`s).

## Testing

`make check` runs all of it from a clean checkout — no phone, account, or
network.

1. **Transport unit tests** against a fake stdio server.
2. **Protocol-contract tests** against a recording double that journals
   every request.
3. **Qt event-loop tests** under `QT_QPA_PLATFORM=offscreen`.
4. **QML load tests** against stub Silica components
   (`tests/silica-stubs/`): the real page files, driven by `objectName`.
   The stubs imitate no layout, so nothing here says a page *looks* right.
   Silica's `EnterKey` attached property cannot be stubbed — QML forbids
   capitalised property names and `qmetaobject` cannot register attached
   types — so pages using it cannot be loaded.
5. **Static QML dialect tests** for the Qt 5.6 rules.
6. **Real-core integration** (`real_server`, `real_core`), gated on
   `DELTACHAT_RPC_SERVER`, offline.
7. **Packaging checks**: spec parses, desktop entry validates, shell
   scripts clean.

Aspiration, tracked not gated: test volume exceeds source volume.

Out of reach until someone runs it on hardware: Silica's real rendering,
notifications, background sync and suspend, and the packaged RPM's
on-device behaviour. `docs/MILESTONES.md` says so per item.
