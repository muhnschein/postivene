# Harbour and the single-executable rule

Postivene installs two executables: the app, and the bundled
`deltachat-rpc-server` under `/usr/libexec/postivene/`. Jolla's Harbour
store takes one. This is what the rule actually says, which of Postivene's
other Harbour gaps are independent of it, what the ways out cost, and why
none of them requires giving up GPL-3.0-or-later.

§1 and §2 are read off the validator's own source --
`sailfishos/sdk-harbour-rpmvalidator` (`rpmvalidation.sh`,
`allowed_libraries.conf`, `allowed_requires.conf`), `master` as of
2026-08-30 -- with two details checked against real binaries where that was
possible. §5 is measured rather than read: the core was linked into a
binary and run.

What has *not* happened: no Harbour submission has been attempted, and the
validator has not been pointed at `postivene-0.1.0-1.aarch64.rpm`. Doing
that is the cheapest next step and would turn most of §2 from "read" into
"seen".

## 1. The rule, from the validator

Three checks decide the question. `$NAME` is the package name, which
`validatenames` requires to match `^harbour-[-a-z0-9_\.]+$`:

- **`validatelibraries`** runs `file` over every file in the package. For
  anything `file` calls an ELF of the package's architecture it accepts
  exactly two shapes -- `usr/bin/$NAME`, and `usr/share/$NAME/lib/*.so*`
  (plus a `.so` sitting beside a `qmldir`, for a private QML plugin).
  Anything else is
  `ELF binary in wrong location (must be /usr/bin/$NAME)`.
- **`validatepermissions`** allows an execute bit on `usr/bin/$NAME` and on
  `usr/share/$NAME/*.so*`. On any other file:
  `File must not be executable`.
- **`validatepaths`** rejects anything outside `usr/bin/$NAME`,
  `usr/share/$NAME/**`, `usr/share/applications/$NAME.desktop` and
  `usr/share/icons/hicolor/*/apps/$NAME.png`. `/usr/libexec` is not a
  location a Harbour package may install into at all.

So the rule is sharper than "one executable": **one executable, at one
path, plus private shared libraries under `usr/share/$NAME/lib/`.** Three
consequences worth spelling out:

1. `/usr/libexec/postivene/deltachat-rpc-server` fails all three checks.
   `file` calls the bundled aarch64 binary
   `ELF 64-bit LSB executable, ARM aarch64, ..., statically linked,
   stripped`, which is exactly the case `validatelibraries` matches before
   falling through to its error.
2. Moving it under `usr/share/$NAME/` and shipping it mode 0644 as "data"
   does not help. The ELF check keys off `file`, not off permissions, so it
   is still a binary in the wrong location.
3. Shared libraries *are* allowed. The rule is not "nothing but your own
   code"; it is "one entry point, plus whatever libraries that entry point
   loads". Every option in §3 goes through that seam.

## 2. The other Harbour gaps

Worth listing, because making the package one executable is necessary and
nowhere near sufficient. In rough order of effort:

| Gap | Where | Status |
|---|---|---|
| Package, binary, data dir, desktop entry and icons must all be named `harbour-postivene` | spec, `postivene.desktop`, `icons/`, `qml_dir()` | mechanical rename, touches everything |
| `Requires: libsailfishapp-launcher` | spec | **explicit error** for a binary app: the validator only permits it when the desktop entry uses the `sailfish-qml` launcher, which ours does not |
| `Requires: sailfish-version >= 4.5.0` | spec | not on `allowed_requires.conf` -> `Dependency not allowed` |
| `libQt5Widgets.so.5` | linked by `qttypes` | not on `allowed_libraries.conf` (Core/Gui/Qml/Quick/Network/DBus are) -> `Cannot link to shared library` |
| Binary must export `main()` | `validatesymbols` | an **error**, not a warning, for anything importing `Sailfish.Silica`; Sailfish's booster `dlopen`s the app and calls `main` |
| `__libc_start_main@GLIBC_2.34` | `validatesymbols` | `master`'s config expects 2.34; our aarch64 binary tops out at `GLIBC_2.29` (`docs/SDK-BUILD.md`). Validate with the release the package targets, not with `master` |
| Icons named `harbour-postivene.png` at 86x86, 108x108, 128x128 and 172x172 | `icons/` | all four sizes exist already, under the wrong name; the 256x256 we also ship is allowed but not counted |

Two of these are worth more than a table row.

**`libQt5Widgets.so.5`.** `qttypes` 0.2.12's build script links `Core`,
`Gui` and `Widgets` unconditionally -- `Quick`/`Qml` are behind the
`qtquick` feature, `Widgets` is behind nothing (`build.rs:244-246`). So
every `qmetaobject` app links Qt Widgets whether or not it uses it, and
our aarch64 RPM does (`docs/SDK-BUILD.md`). Postivene calls nothing in
Widgets, so `-C link-arg=-Wl,--as-needed` should drop the `DT_NEEDED`
entry without patching the crate. Untested here -- there is no Qt in the
environment this was written in -- and it is the first thing to try.

**The exported `main()`.** Sailfish's `mapplauncherd` booster does not
`exec` a `silica-qt5` application; it `dlopen`s the binary in a pre-forked
process and calls its `main`, which is why the validator insists the symbol
be dynamic. This has a direct bearing on §3: **under the booster,
`/proc/self/exe` is the booster, not Postivene.** Any "re-run myself as the
server" design must spawn the compiled-in installed path
(`/usr/bin/harbour-postivene`), not `/proc/self/exe`.

It also needs a link flag. rustc does not put `main` in `.dynsym`:
`readelf --dyn-syms` on a stock `rustc -O` binary finds no `main` at all,
which is what the validator would report. `-C
link-arg=-Wl,--export-dynamic-symbol=main` fixes it and exports only that
one symbol; plain `--export-dynamic` also works but publishes everything
(1930 dynamic symbols against 73, on a hello-world). Both checked here, on
a host toolchain rather than the SDK's.

## 3. Ways to one executable

One clarification first, because the obvious phrasing of the idea is "just
link the whole app statically": that cannot mean the whole app. Postivene
links the device's Qt 5.6 and has to. Silica is resolved at run time by
that Qt, the SDK ships no static Qt build, and Harbour's own gate is a
whitelist of *shared* libraries a binary may link -- a fully static app
would be the odd one out, not the compliant one. What can be static is
everything Postivene brings with it, the core included: a Rust link plus
the C libraries core vendors. That is what §5 measures.

### A. Link the core in, and re-run the app as the server

`postivene --rpc-server` serves the core's JSON-RPC API on its own stdio;
the app spawns `/usr/bin/harbour-postivene --rpc-server` instead of a
separate server binary. One file in the package, still two processes at
run time -- which is what we want, since the core does the blocking work,
and a core panic taking the UI down with it would be a regression.

Nothing in `rust/deltachat-jsonrpc` changes: `RpcClient::spawn` is handed a
different path and keeps framing newline JSON over the child's stdio. The
server loop itself is ~90 lines around `deltachat_jsonrpc::api::CommandApi`
and `yerpc::RpcSession` (upstream's `deltachat-rpc-server/src/main.rs` is
the reference implementation, and §6 is why we may read it).

Cost: the toolchain wall (§4), and the build in §5.

### B. Link the core in, and drop the subprocess

Same dependency, no child: `RpcClient` grows a second constructor that
talks to an in-process `RpcSession` over channels. Marginally less code at
run time, and it gives up the isolation A keeps -- a panic in core work, or
the OOM killer, now takes the UI with it, and every test that drives a
spawned server needs a second shape. Same toolchain cost as A, no Harbour
advantage over it. Not recommended.

### C. Ship the core as a private shared library

`usr/share/harbour-postivene/lib/libdeltachat_rpc_server.so`, a Rust
`cdylib` exporting one C entry point; the app `dlopen`s it (or links it
with `rpath=$ORIGIN/../share/harbour-postivene/lib`, which is the rpath
`validaterpath` demands) and otherwise behaves as in A.

Its one virtue is decoupling: the `.so` is built by whatever modern rustc
core needs, while the app keeps building on the SDK's 1.75. That is a real
virtue -- it is the only option here that does not move the app's own
toolchain floor.

Its costs are equally real. The `.so` has to be cross-built for
`aarch64`/`armv7hl` against a glibc no newer than the oldest device's,
because a shared library loaded into a glibc process cannot be the static
musl artefact upstream publishes. Everything the current bundled binary
gets for free -- "upstream builds it, it is statically linked, the target's
glibc is irrelevant" (`docs/MILESTONES.md` §1) -- has to be rebuilt and
maintained. A Rust `staticlib` in the same role is not an alternative:
linking one into another Rust program duplicates std and the panic runtime.

### D. Embed the prebuilt server as a payload

`include_bytes!` the existing musl binary into the app, and at run time
write it to a `memfd_create` descriptor and `execveat` it, never touching
the filesystem. The package holds one ELF and passes §1; the validator's
only content-level check (`validatesandboxing`) greps for hardcoded
`/home/nemo|defaultuser` paths and would see nothing.

This preserves everything that already works: upstream's exact
checksum-pinned binary, static musl, Rust 1.75, the current spec, and the
crisp MPL boundary. It costs a ~20 MB payload, and an `unsafe` block in a
workspace that denies `unsafe_code`.

It is also a way around the rule rather than a way to satisfy it. The
package would contain two programs, one of them in a data section, and a
reviewer who noticed would be right to say so. Also unverified: whether
sailjail's seccomp profile permits `execveat` on a memfd. Ask Jolla before
building on this, not after.

### E. Require the server as a separate package

Not available. `validaterpmrequires` whitelists `Requires:` to Jolla's own
packages (`allowed_requires.conf`); a Harbour package cannot depend on a
`deltachat-rpc-server` package from Chum, and telling a user to install a
binary by hand is not shipping.

### F. Don't use Harbour

The status quo, and the baseline the others are measured against: Chum and
OpenRepos take the package as it stands, and `docs/MILESTONES.md` already
targets them. What Harbour buys is the store that is on the phone by
default. What it costs is §2 plus one of A-D. That trade is a product
decision, not a technical one, and it should be made before any of this is
built.

## 4. The toolchain wall

A, B and C all need the Delta Chat core compiled for a Sailfish target.
Today that cannot be done with the toolchain Sailfish ships:

- `chatmail/core` v2.53.0 -- the version `scripts/fetch-rpc-server.sh` pins
  -- declares `edition = "2024"` and `rust-version = "1.89"`.
- Sailfish ships Rust 1.75.0, which is why this workspace declares
  `rust-version = "1.75"`, keeps `Cargo.lock` at v3, and has an `msrv` CI
  job (`docs/ENGINEERING.md`). Edition 2024 needs 1.85 at the earliest.

So there is no version of "add a dependency and rebuild in the SDK". The
options are:

1. **Cross-build with a rustup toolchain against the SDK's sysroot.**
   Harbour takes an RPM you built; it does not build it for you, so nothing
   requires the SDK's cargo. `.github/workflows/rpm.yml` already drives the
   Platform SDK image on a runner and could install rustup beside it. The
   work is pointing a modern rustc at the target's linker, sysroot and Qt
   5.6 headers -- i.e. re-doing by hand what scratchbox2 currently does,
   for the C parts of the build as well.
2. **Option C's decoupling**: only the `.so` needs the modern toolchain.
3. **Wait for Jolla's Rust to move.** No 5.2 SDK target exists publicly yet
   (`docs/MILESTONES.md`); if it lands with a current Rust, option 1's work
   mostly evaporates.

Two more costs that come with the core's own build, whichever route:

- Core's default `vendored` feature builds **SQLCipher and OpenSSL from
  source** (`rusqlite/bundled-sqlcipher-vendored-openssl`,
  `async-native-tls/vendored`), and the rustls stack pulls `aws-lc-sys`.
  All three did compile in §5's build, and all three are C: the spec's
  `BuildRequires` would grow `cmake` and `perl` alongside `gcc-c++`, and
  every one of them has to find the *cross* compiler under sb2, which is
  the same class of problem that `CARGO_TARGET_<HOST>_LINKER=host-gcc`
  already exists to solve (`docs/SDK-BUILD.md`). Under sb2 the build is
  also forced to `cargo -j1`, because parallel cargo deadlocks there. This
  is not a six-minute CI job any more.
- Turning `vendored` off instead means linking the target's OpenSSL and a
  system SQLCipher. `libcrypto.so.3`/`libssl.so.3` are on Harbour's allowed
  list; `libsqlcipher` is not, so at least that half stays vendored.

## 5. What option A costs, measured

Guesses about build time and binary size are worth little, so option A was
built and run on an x86_64 Linux host: a throwaway crate depending on
`deltachat-jsonrpc` from `chatmail/core` v2.53.0 -- the tag
`scripts/fetch-rpc-server.sh` already pins -- with ~90 lines around
`CommandApi` and `yerpc::RpcSession` serving JSON-RPC on stdio, which is
what `postivene --rpc-server` would run:

```toml
[dependencies]
deltachat = { git = "https://github.com/chatmail/core.git", tag = "v2.53.0" }
deltachat-jsonrpc = { git = "https://github.com/chatmail/core.git", tag = "v2.53.0" }
anyhow = "1"
futures-lite = "2"
log = "0.4"
serde_json = "1"
tokio = { version = "1", features = ["io-std", "io-util", "rt-multi-thread", "macros", "signal", "sync"] }
tokio-util = "0.7"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
yerpc = { version = "0.6.4", features = ["anyhow_expose", "openrpc"] }
```

| | |
|---|---|
| Crates compiled | 578 (671 in the graph across all platforms) |
| Build | 11m25s wall, 36m CPU, 4 cores, stock `release` profile |
| `target/` | 1.5 GB |
| Binary, stock `release` | 92 MB, 73 MB stripped |
| Binary, upstream's release profile (`lto`, `opt-level = "z"`, `codegen-units = 1`, `panic = "abort"`, `strip`) | 27.5 MB, 10m10s wall / 19m CPU (it also still passes the suite below) |
| For comparison, the bundled binary today | 21.7 MB aarch64, 20.8 MB armv7hl, 28.5 MB x86_64 |
| `ldd` | `libgcc_s`, `libm`, `libc` and nothing else: OpenSSL and SQLCipher are vendored *into* the binary, so linking the core adds no shared-library dependency and nothing new for Harbour's allow-list |
| `--version` | `2.53.0` -- the same core the bundled binary is |
| The repo's own integration suite | **passes against it.** `DELTACHAT_RPC_SERVER=<that binary> cargo test -p deltachat-jsonrpc --test real_server` walks `get_system_info`, `add_account`, a config round trip, contact/chat/draft creation and live event delivery, all offline |

That last row is the one worth having. The transport does not care that the
"server" it spawned is another copy of the app: option A needs no change to
`rust/deltachat-jsonrpc` at all, and the existing gate tests it.

The size rows say something too. Linking the core in does not add its
weight to the package -- it moves it. Built with upstream's own release
profile the result is 27.5 MB against the 28.5 MB x86_64 binary the RPM
bundles today: the same code, in one file instead of two. That profile is
not optional, though. The stock `release` profile gives 73 MB stripped for
the same thing, so whichever option is taken, the app's `[profile.release]`
has to be tuned the way upstream tunes core's -- and `panic = "abort"`
among those settings is a decision for the UI process, not a free one.

Read the build numbers as a floor, not an estimate. They are a host build
with a modern rustc, network access, and four cores. The Sailfish path adds
cross-compilation, `cargo -j1` under sb2 (`docs/SDK-BUILD.md`), and -- for
OBS/Chum, which build offline -- a `cargo vendor` tarball that now has to
carry ~600 crates plus the OpenSSL and SQLCipher sources. The six-minute
RPM workflow does not stay six minutes.

## 6. Licensing: no, this does not need MPL-2.0

Summarised here because it is the question that usually comes with this
one; the argument is in [`LICENSING.md`](LICENSING.md), and the dependency
tree it turns on was measured there too.

Linking the core in does **not** require relicensing Postivene back to
MPL-2.0, and doing so would not help. MPL-2.0 §1.12 counts GPL-3.0-or-later
as a "Secondary License", and §3.3 lets Covered Software be combined into a
Larger Work under other terms -- that clause is about combining, not about
process boundaries, so it is exactly the permission a static link needs.
Core's own files keep their MPL terms; Postivene stays GPLv3+.

What linking changes is *scope*: an aggregated pair of binaries becomes one
combined work, so core's whole dependency tree has to be GPLv3-compatible,
and `rust/deny.toml`'s allow-list has to be re-run across it. That is where
the licensing work actually is, and it was done for §5's build: nothing in
the 671-package graph is GPL-incompatible, the vendored C is OpenSSL 3.6.3
(Apache-2.0) and SQLCipher (BSD-3-Clause), and the allow-list needs two
additions plus a git-source exception. See LICENSING.md, "If the core is
linked in".

## 7. Recommendation

1. **Decide whether Harbour is worth it at all** (§3F). Everything below is
   weeks of work on a project whose packaging currently works.
2. If it is: **fix §2 first.** The rename, the two bad `Requires:`,
   `--as-needed` for Qt Widgets and the `main()` export are cheap,
   independent of the server question, and they are what makes the next
   step meaningful -- run the real validator against the RPM CI already
   builds and replace this document's readings with its output.
3. Then **A**, with **C** as the fallback if the app must keep building on
   the SDK's Rust. Not D: it is the cheapest and it is the one that could
   get the package pulled.
4. In either case the gating work is §4, and it is worth doing on its own
   terms. A core built from source for the target is also what removes the
   last "trust upstream's binary" step from the supply chain.
