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
`env::set_var` before Qt initialises, and because one thing the app does
has no safe binding: installing a `QTranslator`, which qmetaobject does
not wrap. That is the one `cpp!` block in the tree, in
`postivene-app/src/translations.rs`, and the C++ build step in that
crate's `build.rs` exists for it alone. Every exception is at the
narrowest scope and says why.

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

### Wait for the thing, not the clock

The Qt tests schedule steps with `single_shot` at fixed seconds. That is a
bet that the work is done by then, and under a loaded `make check` it is not
always: `chat_model` failed about one run in three that way, asserting on
calls that had simply not been made yet.

Where a step depends on work finishing, wait for the work. A repeating
`Timer` in the probe that checks the condition and acts once is the pattern
— `chat_model.rs` and `qml_naming.rs` both do this — with `single_shot` left
as a backstop that reads results and quits. The other Qt tests still
schedule on the clock and should move over as they are touched.

## Profiling on a device

A phone has none of the tools a workstation profiles with, so the app
carries its own. The developer view is behind ten taps on the Settings
page's title within three seconds. *Start recording* there, then use the
app -- open a chat full of pictures, play a GIF, open one full screen,
scroll a long chat -- and come back to *Mark* what you just did, take a
*Memory snapshot*, or *Stop*. Everything lands in
`~/Documents/postivene-recordings/<date-time>/`, which the sandbox's
`UserDirs` grant lets the app write and an SSH session read:

- `timeline.tsv`, one line a second per kind, tab-separated, the first
  column milliseconds since the start. `frame`: frames the window
  presented, beats of the main-thread heartbeat, and the longest gap of
  each in the second, in ms -- a gap in the frames with none in the beats
  is the render thread stalling, a gap in both is the main thread. `mem`,
  per process (`app`, `core`): pid, resident, proportional (`Pss`),
  anonymous, private dirty, all KiB, then threads, open file descriptors
  and CPU as a percentage of one core. `thread`: the busiest threads of
  each process by name -- `QSGRenderThread`, `QQmlThread`, a pooled
  decoder -- with their share. `mark`: what you typed. `snapshot` and
  `stop`.
- `system.txt`: kernel, pids, seccomp mode and filter count, NoNewPrivs,
  the capability sets, the LSM list, whether Landlock is enabled, built
  in or absent, and the ptrace scope -- the facts `SECURITY.md` left to a
  device.
- `mounts.txt`, the sandbox's view of the filesystem, and
  `maps-app.txt` / `maps-core.txt`, every file each process has mapped.
- `snapshot-<n>/`: the full `smaps`, `status`, `maps`, open descriptors
  and thread names of both processes at that moment. `smaps` is where a
  memory question is answered: `Rss` per mapping, `[heap]` against
  `/dev/kgsl-3d0` or `/dev/mali0` (GPU allocations, which show up nowhere
  else) against the libraries.
- `strace.sh`: the syscall list the app cannot make itself. The
  sandbox's seccomp filter drops `ptrace`, so the tracer has to attach
  from outside: `devel-su sh <recording>/strace.sh 60` on the phone
  (`pkcon install strace` once) traces both processes for sixty seconds
  and writes `syscalls-app.txt` and `syscalls-core.txt`, each the
  distinct syscalls made with counts -- what a whitelist would have to
  allow, recorded from the paths you drove meanwhile.

Reading it back: `grep -P '\tframe\t' timeline.tsv | awk -F'\t' '$5 > 40'`
finds the seconds with a frame more than 40 ms late, `grep -P '\tmark\t'`
says what was happening, and the `mem` lines either side say what it
cost. The frame counter is the window's own `frameSwapped`, hooked from
C++ in `postivene-app/src/frames.rs` because that signal fires on the
render thread; the heartbeat is an animation the root window runs while
recording, which also keeps the scene graph presenting a frame every
refresh, so an idle screen does not read as a stall.

The lighter tool is still there: `POSTIVENE_MEMORY_LOG=5
/usr/bin/harbour-postivene` from a terminal on the phone prints the
resident size of both processes every five seconds on stderr, for a
first look with nothing to copy off.

What each side is made of, and how to look closer:

- **The app** is Qt: every decoded picture on screen is a texture, and a
  texture is width times height times four bytes whatever the file
  weighed -- a phone screenshot is 20 MB decoded. `sourceSize` on an
  `Image` is what bounds that; a row that decodes at the file's own size
  rather than the row's is the usual leak-shaped growth. `QSG_INFO=1` on
  the same command line has the scene graph say what it allocates, and
  `QML_DISABLE_DISK_CACHE=1` takes the QML cache out of the picture.
- **The core** is upstream's: SQLite's page cache, the accounts'
  in-memory state, and the tokio runtime. It is the same binary the
  desktop client and the Android app run, so a core that grows without
  bound is a bug to take upstream with the log that shows it.
- **Proportional sizes.** `VmRSS` counts shared pages -- Qt's libraries,
  the GL driver -- in full for each process. The recording's `Pss`
  column is the proportional figure (`/proc/<pid>/smaps_rollup`, which
  `smem` reads too); the resident numbers are for spotting the movement,
  not the total.

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

## Comments

One sentence where one will do. A comment states what is true now and why.
It is not a changelog, a bug report, or a story about how the code got here
— that belongs in git history. Delete a comment rather than update it into a
history of its own subject. If the reasoning needs a paragraph, it is a note
in `docs/` and the comment is a pointer to it.

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

`i486` packages exclude the bundled server: upstream publishes no 32-bit x86
musl binary.

## Building without network access

OBS and Chum build offline. Vendor the crates and switch the spec over:

```sh
scripts/vendor-crates.sh
sfdk build -- --with vendor
```

with the `[source.crates-io] replace-with = "vendored-sources"` stanza
`cargo vendor` prints installed as `rust/.cargo/config.toml` -- beside the
workspace, where `%build` now runs cargo from, since cargo finds its
config by working directory and not by manifest.
Add `-n` (`--no-pull-build-requires`) and rpmbuild's `--nodeps` to `mb2` to
skip BuildRequires resolution.

Stock SDK targets ship **no rust at all**, so an environment that cannot
reach the Jolla repos must also graft in the standard-library rlibs that
`rust-std-static` would provide: the tooling's own i686 std copied into the
target *and its `.default` snapshot* (mb2 builds against the snapshot), plus
a target std built from source with **the tooling's own rustc**. Do not
substitute rustup's std for the same version — Jolla's compiler reports
`1.75.0-nightly (82e1608df)` against upstream's `1.75.0 (82e1608df)`, same
commit but a different release string, and rustc rejects the rlibs with
`E0514`. Unpack `rust-src-1.75.0` into the tooling and build a dummy crate
with `-Zbuild-std=std,panic_unwind` and `RUSTC_BOOTSTRAP=1`; its final link
fails on the tooling's i686 linker, which is fine — the rlibs under
`target/<triple>/release/deps/` are what get installed.

## What the builds have proven

Verified on the produced package, not just the build log: the app binary
is an aarch64 pie executable linked against the target's Qt 5.6.3 with a
highest requirement of `GLIBC_2.29`; the bundled server survives rpm's strip
pass and still answers `--version` → `2.59.0` and passes the full
`real_server` suite under `qemu-aarch64`; and `rpm -qlp` shows every file
where `qml_dir()` and the rpc-server lookup expect it. Those builds predate
the Harbour rename and produced `postivene`, not `harbour-postivene`;
nothing else about them changed. They also linked `libQt5Widgets.so.5`,
which `HARBOUR.md` explains is a blocker.

Not proven: Silica's real rendering, notifications, background sync and suspend
stay out of reach of `make check` entirely — see `PROJECT.md`.