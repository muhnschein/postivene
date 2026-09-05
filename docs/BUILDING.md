# Building, testing, and packaging

Engineering standards and the build procedure. The reference for the former
is [clove](https://github.com/muhnschein/clove)'s §9: pinned toolchain,
pedantic lints, CI-parity `make` targets, tests that drive the real
binaries.

## Toolchains

`rust-toolchain.toml` pins 1.94.1 so lint results are reproducible; the
device floor is **1.75.0**, enforced by CI's `msrv` job with warnings
denied. It is a rustup mechanism, and the Sailfish SDK's cargo ignores it.

`rust/Cargo.lock` stays at **v3**: cargo learned v4 in 1.78 and the SDK's
cargo 1.75 cannot read it, while a `cargo update` on a modern host rewrites
it silently. `ci/check-lockfile.sh` catches that.

## Lints

Workspace-level, so a bare `cargo clippy` fails the way CI does:
`clippy::all` and `pedantic` at deny, `unwrap_used`/`expect_used` denied
outside tests, `missing_docs` and `unsafe_code` denied.

`unsafe_code` is deny rather than forbid because the Qt harness tests need
`env::set_var` before Qt initialises, and because two things the app does
have no safe binding: installing a `QTranslator`, and recording a voice
message through `QAudioRecorder`, neither of which qmetaobject wraps.
Those are the two `cpp!` files in the tree, `postivene-app/src/translations.rs`
and `postivene-shim/src/recorder.rs`, and the C++ build step in each
crate's `build.rs` exists for them alone. Every exception is at the
narrowest scope and says why, and every block is short enough to be
checked by reading.

The recorder is also why the host build needs `qtmultimedia5-dev` (the
`Makefile` lists the packages) and the spec `pkgconfig(Qt5Multimedia)`:
the shim links `libQt5Multimedia`, which Harbour allows.

`rust/clippy.toml` bans two methods that have already caused device-only
failures: `tokio::runtime::Runtime::new` (must go through `CoreRuntime`) and
`qmetaobject::single_shot` (truncates sub-second `Duration`s).

## Testing

Postivene parses almost nothing — protocol and crypto are the core's, and
the subprocess we talk to is one we spawned. The failure mode is misreading
the core's JSON, or calling it wrongly, with nothing noticing until the app
is on a phone. The tests aim at that.

`make check` runs all of it from a clean checkout: no phone, account or
Sailfish SDK. Not quite offline — `msrv` fetches the 1.75 toolchain the
first time, and `deny` wants the advisory database.

1. **Transport unit tests** against a fake stdio server.
2. **Protocol-contract tests** against a recording double that journals
   every request, pinning the call sequence of each onboarding action.
3. **Qt event-loop tests** under `QT_QPA_PLATFORM=offscreen`.
4. **QML load tests** against stub Silica components (`tests/silica-stubs/`):
   the real page files, driven by `objectName`. The stubs imitate no layout,
   so nothing here says a page *looks* right. Silica's `EnterKey` attached
   property cannot be stubbed — QML forbids capitalised property names and
   `qmetaobject` cannot register attached types — so pages using it cannot
   be loaded. Put what such a page shows in a component that can be, and lay
   that component out with bindings rather than a `Column`: a positioner
   sizes itself in a polish pass, which never runs without a window, so its
   geometry reads as zero.
5. **Static QML tests** (`tests/qml_syntax.rs`) for what no host-Qt run
   can see: Qt 5.6 rules that Qt 5.15 accepts silently, and the rules the
   tree holds itself to -- every string the other end chose is drawn as
   plain text (Silica's own headers cannot be, so `ConversationHeader`
   exists), file URLs are encoded a segment at a time, only the picker
   pages import `Sailfish.Pickers`, and every `model.<role>` a delegate
   binds is one its model has.
6. **Real-core integration** (`real_server`, `real_core`), gated on
   `DELTACHAT_RPC_SERVER`, offline. `real_core.rs` distinguishes a request
   the real core could not decode from one it could not deliver.
7. **Packaging checks** (`ci/packaging-lint.sh`): spec parses, desktop
   entry validates, shell scripts clean, every translation catalog
   current and compiling cleanly with `lrelease`, every `docs/*.md` a
   comment points at exists. Locally a missing tool is
   a SKIP; CI sets `PACKAGING_LINT_STRICT=1` so it is a failure there, as
   `HARBOUR_CHECK_STRICT=1` already does for the Harbour check.

Aspiration, tracked not gated: test volume exceeds source volume.

## Translations

The strings are the `qsTr()` calls in `qml/`; `translations/postivene.ts`
is the untranslated source catalog and `translations/postivene-<lang>.ts`
one catalog per language Sailfish ships in. `scripts/update-translations.sh`
regenerates all of them from the source in one `lupdate` run, so a new
string turns up as `unfinished` in every language at once, and
`ci/packaging-lint.sh` fails when a committed catalog differs from what
that run produces. `tests/translation_catalogs.rs` fails when a string in
any language is left untranslated, so a new string is not done until every
catalog has it.

The app loads `postivene-<lang>.qm`, which `scripts/release-translations.sh`
compiles with `lrelease` -- in the RPM's `%build`, and locally with
`make translations`, which leaves them beside the `.ts` files where a
source-tree run finds them. `lupdate` and `lrelease` are Debian's
`qttools5-dev-tools`, and the SDK's `qt5-qttools-linguist`; the app's own
test compiles the German catalog, so the package is a test dependency too.

`<lang>` is what `QTranslator` matches against the reader's locale from
the most specific form down: `de` serves every German locale, `pt_BR`
only Brazil, and a language with no catalog gets the English one -- the
strings are English already, and that catalog holds their plural forms.
To add one, write the three-line header `update-translations.sh` documents to
`translations/postivene-<lang>.ts` and run the script; `lupdate` fills in
every string with as many plural forms as that language has.

## Dependencies

Few, and each for a reason. `cargo tree` on the app is the list; this is
why each entry is there, so that a proposal to drop one starts from what
it would cost.

| Crate | What it is for | Why it stays |
|---|---|---|
| `tokio` | the server subprocess, its pipes, the event loop | the transport is async; `process` is what reaps the child |
| `serde`, `serde_json` | the JSON on the wire | the contract with the core is JSON-RPC |
| `qmetaobject` (vendored), `qttypes`, `cpp`, `cpp_build` | Qt from Rust | the whole UI hangs off them; `default-features = false` keeps its `log` bridge out |
| `chrono` | the viewer's timezone, for the day headings | `std` has none, and the alternative is `localtime_r`, which `unsafe_code` denies |
| `qrcode` | an invite drawn as a code | one crate, no dependencies |
| `rqrr` (+ `g2p`, `lru`) | a code read off the camera | a QR decoder is not a small thing to vendor |

What is not there any more, and where the line is: `thiserror` was two
crates for a dozen lines of `Display`, so the transport's errors are
written out; the fake servers build their tokio runtime by hand, so
`macros` is a dev-dependency and the app's build carries no
`tokio-macros`; qmetaobject's `log` feature is off. `serde`'s `derive`
could go the same way for one crate less, at the cost of hand-written
`Deserialize` for the four wire types -- more code than it saves, so it
stays. Everything else is either the vendored qmetaobject's own
(`lazy_static`, `syn 1`) or a build script's (`cc`, `regex`, `semver`,
`rustversion`), and the platform-gated crates in `Cargo.lock` --
`windows-*`, `wasm-bindgen`, `js-sys` -- are resolved for other targets
and never built here.

## Comments

One sentence where one will do. A comment states what is true now and why.
It is not a changelog, a bug report, or a story about how the code got here
— that belongs in git history. Delete a comment rather than update it into a
history of its own subject.

## Packaging: the supported path

`.github/workflows/rpm.yml` builds a device RPM unattended on an
`ubuntu-latest` runner, from a `docker run` of
`coderus/sailfishos-platform-sdk`. Dispatch it from the Actions tab (arch
and SDK version are inputs) or push a `v*` tag. A runner reaches the Jolla
repositories, so `mb2` zypper-installs `rust`, `cargo` and `rust-std-static`
into the target itself.

```sh
./scripts/fetch-rpc-server.sh                        # bundled server binaries
mb2 -t SailfishOS-<ver>-<arch> -X build-init
mb2 -t SailfishOS-<ver>-<arch> -X build --no-check
```

- `-X` (`--no-fix-version`) uses the spec's `Version:` rather than deriving
  one from git tags. It is needed **by `build-init`** too: without it that
  step gives up at version-fixing and never writes `.mb2/spec`, so `build`
  fails identically and the flag looks innocent.
- `build-init` must precede `build`, which queries `.mb2/spec` within a
  second of starting.
- `--no-check`: the tests are host-oriented, and the spec has no `%check`.

Environment requirements, each of which cost an attempt:

- **Mount the tree inside the SDK user's home** (`/home/mersdk/<name>`), not
  `/build` or `/share`. rpm runs under scratchbox2, which redirects
  unrecognised absolute paths into the target rootfs; with the tree
  elsewhere `mb2` writes `.mb2/spec` outside and rpm reads inside. A file
  that exists and cannot be opened is the signature. The directory must keep
  the package's name — `mb2` derives the package from it.
- **The i686 rustlib at the SDK's own `/usr/lib/rustlib`.** `mb2` installs
  rust into the *target*, but build-script links run in sb2's host mode
  where `/usr` maps to the SDK filesystem. Copy it from the tooling.
- **Not root.** `sdk-manage` refuses ("Cannot determine Mer SDK user") and
  the target snapshot never initialises. Chown the checkout to the
  container's `mersdk` uid — read it from the image, don't assume it — and
  hand it back so the artifact upload can read the result.

`scripts/build-rpm.sh` wraps the ordinary developer path, `sfdk build`.

## Spec constraints

Landmines encoded in `rpm/harbour-postivene.spec`, each found the hard way:

- **`-j1` for cargo under sb2.** At `-j4` cargo reproducibly futex-waits
  forever on an unreaped child while qmetaobject's C++ glue compiles. The
  spec forces `-j1` whenever `SBOX_SESSION_DIR` is set.
- **No `--target` for cargo.** Jolla's cargo pins build scripts to the
  tooling's host triple; `--target` on top makes cargo treat the whole build
  as a cross build. `SB2_RUST_TARGET_TRIPLE` already tells the accelerated
  rustc what to emit. Whisperfish's spec passes none either.
- **`CARGO_TARGET_<HOST>_LINKER=host-gcc` inside sb2.** rustc links build
  scripts by calling plain `cc`, which sb2 rewrites to the *cross* compiler
  (`aarch64-meego-linux-gnu-cc: unrecognized option '-m32'`). scratchbox2
  exposes the native compiler as `host-gcc`. Pointing at the tooling's gcc
  by absolute path is not enough — sb2 still rewrites the `ld` that gcc
  invokes, giving `cannot find /lib/libgcc_s.so.1`.
- **`QT_INCLUDE_PATH`/`QT_LIBRARY_PATH` exported in `%build`**: qttypes
  cannot exec the target `qmake` under sb2. `QT_LIBRARY_PATH` uses
  `%{_libdir}` — Qt is in `/usr/lib64` on aarch64, not `/usr/lib`.
- **`%{_target_cpu}`, not `%{_arch}`**, for the bundled server path.
- **`Exec=harbour-postivene`** in the desktop file: the invoker does not
  honour an `Exec=env FOO=bar` wrapper, so the bundled server path is a
  fallback inside the binary.
- **Harbour constrains the name, the paths and every `Requires:`.**
  `ci/harbour-check.sh` fails a build that breaks one; `HARBOUR.md` is
  the map, including the two rules this package still breaks.
- **No bare `%` in a spec comment.** rpm expands macros inside comments, and
  on the SDK's older rpm a comment mentioning `%build` expands to a preamble
  starting `LANG=C`, which rpm reads as a tag. Host rpm 4.18 leaves comments
  alone and had parsed the same file through an entire successful build.
  `ci/packaging-lint.sh` checks for this directly.
