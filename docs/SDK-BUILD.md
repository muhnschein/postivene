# Building with the Sailfish Platform SDK (mb2)

How the first real `mb2` builds of `rpm/postivene.spec` were performed, and
how to reproduce them. The normal developer path is the official Sailfish
SDK (`sfdk build`); this documents the headless/CI-style path through the
Platform SDK docker image, including the workarounds needed in a
network-restricted environment.

## Environment

- Image: `coderus/sailfishos-platform-sdk:5.0.0.43` from Docker Hub
  (the community Platform SDK image used by e.g. Whisperfish's CI and
  `coderus/github-sfos-build`). It ships `mb2` 1.4.89, scratchbox2, the
  SailfishOS-5.0.0.43 tooling, and pre-installed sb2 targets for
  `aarch64`, `armv7hl`, and `i486`.
- The image was used as a plain chroot (the container filesystem entered
  with `chroot --userspec=mersdk:mersdk`, with `/proc`, `/sys`, `/dev`,
  `/dev/pts` bind-mounted and the repo bind-mounted at
  `/home/mersdk/...`), which behaves identically to `docker run` for
  mb2's purposes.

## The build itself

From a copy of this repo owned by the `mersdk` user:

```sh
# once: fetch the bundled deltachat-rpc-server binaries + vendor crates
./scripts/fetch-rpc-server.sh
(cd rust && cargo vendor vendor)   # plus the .cargo/config.toml below

mb2 -t SailfishOS-5.0.0.43-aarch64 -X -n build --no-check -- --nodeps
mb2 -t SailfishOS-5.0.0.43-i486    -X -n build --no-check -- --nodeps
```

- `-X` (`--no-fix-version`): use the spec's `Version:`, don't derive one
  from git tags (this repo has no release tags yet).
- `-n` (`--no-pull-build-requires`) and rpmbuild's `--nodeps`: skip
  BuildRequires resolution/installation. Only needed because this
  environment has no network path to the Jolla package repositories --
  a normal SDK or OBS build should drop these and let zypper install
  `rust`, `cargo`, etc. into the target.
- `--no-check`: this repo's tests are host-oriented (`cargo test` against
  a fake stdio server); there is no `%check` section in the spec anyway.

Both runs produced real packages:
`postivene-0.1.0-1.aarch64.rpm` (app is a proper
`ELF 64-bit ... ARM aarch64` pie executable dynamically linked against the
target Qt 5.6.3/glibc stack, plus the bundled statically-linked musl
`deltachat-rpc-server`) and `postivene-0.1.0-1.i486.rpm` (32-bit Intel
ELF, no bundled server -- see the spec conditional).

One landmine encoded in the spec: **parallel cargo deadlocks under sb2**.
At the default `-j4`, cargo reproducibly futex-waits forever on an
unreaped child while qmetaobject's C++ glue compiles. The spec passes
`-j1` whenever `SBOX_SESSION_DIR` is set (i.e. inside an sb2 session) and
leaves parallelism alone elsewhere.

`.cargo/config.toml` at the repo root, so the `%build` cargo uses the
vendored crates instead of the network (scratchbox2 has no reliable
network path to crates.io here):

```toml
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "rust/vendor"
```

Note `rust/Cargo.lock` is kept at lockfile `version = 3`: the SDK's cargo
is 1.75 and cannot read the v4 lockfiles written by cargo >= 1.83. Newer
host cargos read v3 fine; avoid re-writing the lockfile with a new cargo
without checking this.

## Workaround: rust in the targets

The stock 5.0.0.43 targets ship **no rust at all** -- `mb2` normally
zypper-installs `rust`, `cargo`, and `rust-std-static-<triple>` from the
Jolla repos on first build, and those repos were unreachable from this
environment. The tooling *does* ship the cross `rustc`/`cargo` 1.75.0
(i686 binaries, all LLVM backends included); what's missing is purely the
per-target standard-library rlibs that the `rust-std-static` packages
would drop into `<target>/usr/lib/rustlib/<triple>/lib`. They were
reconstructed as follows (all inside the SDK chroot):

1. **Host (i686) std for build scripts and proc-macros**: copied from the
   tooling, which ships it:

   ```sh
   cp -a /srv/mer/toolings/SailfishOS-5.0.0.43/usr/lib/rustlib/i686-unknown-linux-gnu \
         /srv/mer/targets/SailfishOS-5.0.0.43-<arch>/usr/lib/rustlib/
   ```

2. **aarch64 std**: built from source with the tooling's own rustc, so the
   compiler metadata matches exactly. Jolla's rustc reports
   `1.75.0-nightly (82e1608df 2023-12-21)` -- commit `82e1608df` is the
   upstream Rust **1.75.0 release** commit, so upstream's
   `rust-src-1.75.0.tar.gz` (from static.rust-lang.org, sha256
   `078b1c23...`) is the matching source. Unpacked into the tooling at
   `usr/lib/rustlib/src/rust`, then, with the tooling's bin/lib dirs on
   `PATH`/`LD_LIBRARY_PATH`:

   ```sh
   cargo build --release -Zbuild-std=std,panic_unwind \
       --target aarch64-unknown-linux-gnu    # in a dummy crate
   ```

   (`RUSTC_BOOTSTRAP=1`; the std dependency crates come from crates.io.)
   The resulting `lib*.rlib` from
   `target/aarch64-unknown-linux-gnu/release/deps/` were installed into
   the target's `usr/lib/rustlib/aarch64-unknown-linux-gnu/lib/`.

   Sanity check: a hello-world built through
   `sb2 -t SailfishOS-5.0.0.43-aarch64 rustc --target aarch64-unknown-linux-gnu`
   links via the sb2-mapped cross gcc and `file` reports a proper
   `ELF 64-bit LSB pie executable, ARM aarch64` binary.

mb2 builds against a *snapshot* of the target
(`SailfishOS-5.0.0.43-<arch>.default`), so the rustlib graft must exist in
the snapshot too (patch both, or let mb2 re-sync the snapshot).

None of this is needed in an environment that can reach the Jolla repos:
there, `mb2 build` installs the real `rust`/`cargo`/`rust-std-static`
packages and the spec builds as-is. It is recorded here because it also
documents *what those packages actually provide* under sb2.

## Known limitations

- The `armv7hl` build has not been exercised yet (same recipe as aarch64
  should apply: build std for `armv7-unknown-linux-gnueabihf`).
- `i486` packages exclude the bundled `deltachat-rpc-server` (upstream
  publishes no 32-bit x86 musl binary); see the conditional in the spec.
- These builds validate compilation, linking against the real Sailfish
  Qt 5.6 stack, and packaging -- not runtime behavior. On-device/emulator
  runs are still tracked in `docs/MILESTONES.md`.


## Second run: what a from-scratch reproduction needs (2026-08-27)

Rebuilt from a clean container to produce a device RPM. Everything above
still applies; these are the additional details that reproduction needed.

### Getting the image without a Docker daemon

No dockerd in that environment, so the image was pulled straight from the
registry API and unpacked as a chroot:

```sh
curl -sS -H 'Accept: application/vnd.docker.distribution.manifest.v2+json' \
    https://mirror.gcr.io/v2/coderus/sailfishos-platform-sdk/manifests/5.0.0.43
# then, per layer digest (note -L: blob URLs redirect to a storage CDN,
# and without it you silently save a 140-byte redirect body):
curl -sSL https://mirror.gcr.io/v2/coderus/sailfishos-platform-sdk/blobs/sha256:<digest> \
    | tar -xz -C rootfs
```

12 layers, ~4.6 GB compressed, ~13 GB unpacked. Then bind-mount `/proc`,
`/sys`, `/dev`, `/dev/pts` and enter with
`chroot --userspec=mersdk:mersdk`, with `HOME=/home/mersdk` in the
environment -- the sb2 targets are registered under that user's
`~/.scratchbox2`, so as root sb2 just says "Invalid target specified".

### Reconstructing rust in the targets, exactly

The stock targets ship no rust (see above). Three pieces are needed, and
the *third* is easy to miss:

1. `cp -a <tooling>/usr/lib/rustlib/i686-unknown-linux-gnu` into the
   target (and its `.default` snapshot) -- host std for build scripts.
2. aarch64 std built from source with the **tooling's own rustc**. Do not
   substitute rustup's std for the same version: Jolla's compiler reports
   `1.75.0-nightly (82e1608df)` and upstream's stable 1.75.0 reports
   `1.75.0 (82e1608df)`, same commit but a different release string, and
   rustc rejects the rlibs with `E0514: found crate compiled by an
   incompatible version of rustc`. Building `-Zbuild-std=std,panic_unwind`
   with the tooling rustc (rust-src 1.75.0 unpacked into
   `<tooling>/usr/lib/rustlib/src/rust`) produces metadata that matches.
   The dummy crate's own final link fails -- it uses the tooling's i686
   linker -- which is fine, the `lib*.rlib` in
   `target/aarch64-unknown-linux-gnu/release/deps/` are what you install
   into the target's `usr/lib/rustlib/aarch64-unknown-linux-gnu/lib/`.
3. The i686 rustlib **also** has to exist at `/usr/lib/rustlib` in the SDK
   chroot itself, not only in the target. Build-script links run in sb2's
   *host* mode, where `/usr` maps to the SDK filesystem; without this the
   link fails with `gcc: error: /usr/lib/rustlib/i686-unknown-linux-gnu/
   lib/libcore-*.rlib: No such file or directory`.

None of this is needed where the Jolla repos are reachable: `mb2 build`
installs the real `rust`/`cargo`/`rust-std-static` packages instead.

### Two spec fixes this run forced

- **No `--target` for cargo.** Jolla's cargo pins build scripts to the
  tooling's host triple, and passing `--target` on top makes cargo treat
  the whole build as a cross build. `SB2_RUST_TARGET_TRIPLE` already tells
  the sb2-accelerated rustc what to emit, and cargo still writes to
  `target/<triple>/release`. Whisperfish's spec passes no `--target`
  either; ours now matches.
- **`CARGO_TARGET_<HOST>_LINKER=host-gcc` inside sb2 sessions.** rustc
  links build scripts by calling plain `cc`, which sb2 rewrites to the
  *cross* compiler: `aarch64-meego-linux-gnu-cc: error: unrecognized
  command-line option '-m32'`. scratchbox2 exposes the native compiler as
  `host-gcc` (`SBOX_HOST_GCC_NAME` in the target's `sb2.config`); pointing
  the host triple's linker at it is what unblocks the build. Pointing it
  at the tooling's gcc by absolute path is *not* enough -- sb2 still
  rewrites the `ld` that gcc invokes, and you get
  `cannot find /lib/libgcc_s.so.1`.
- Also corrected: `QT_LIBRARY_PATH` now uses `%{_libdir}`. Qt lives in
  `/usr/lib64` on the aarch64 target, not `/usr/lib`.

### Result

```sh
mb2 -t SailfishOS-5.0.0.43-aarch64 -X -n build --no-check -- --nodeps
# -> RPMS/postivene-0.1.0-1.aarch64.rpm  (11 MB)
```

Verified on the produced package, not just on the build log:

- `/usr/bin/postivene` is an `ELF 64-bit LSB pie executable, ARM aarch64`
  dynamically linked against the target's Qt 5.6.3
  (`libQt5Core/Gui/Qml/Quick/Widgets.so.5`), with a highest glibc
  requirement of `GLIBC_2.29`.
- `/usr/libexec/postivene/deltachat-rpc-server` is the statically linked
  musl aarch64 build; rpm's own strip pass changes its hash but not its
  behavior -- extracted from the finished RPM it still answers
  `--version` -> `2.53.0`, and it passes the full
  `deltachat-jsonrpc --test real_server` integration suite (health check,
  `add_account`, config round trip, live event delivery, chat/message
  wire shapes) when run under `qemu-aarch64`.
- `rpm -qlp` shows every file where `qml_dir()` and the rpc-server lookup
  expect it, and the desktop entry's `Exec=postivene` (no `env` wrapper).

Still unproven at the time of writing: running it on a real device or
emulator, `BuildRequires` resolution against zypper, and the armv7hl
build. The next section settles the second of those.

## Third run: on a GitHub runner, unattended (2026-08-29)

`.github/workflows/rpm.yml` now does all of the above on an
`ubuntu-latest` runner, from a `docker run` of the same image. Run it
from the Actions tab (arch and SDK version are inputs) or push a `v*`
tag. It produced `postivene-0.1.0-1.aarch64.rpm` against the
**4.6.0.13** SDK in six minutes.

This settles one of the open questions above: **`BuildRequires`
resolution against zypper works**. A runner reaches the Jolla
repositories, so `mb2` installs `rust`, `cargo` and `rust-std-static`
into the target itself and none of the hand-reconstruction in the
previous section is needed. The `-n` / `--nodeps` of the earlier
invocations are offline workarounds and are deliberately not passed.

Four things were needed that the chroot runs never met, each of which
cost an attempt:

- **Mount the tree inside the SDK user's home** (`/home/mersdk/<name>`),
  not somewhere like `/build` or `/share`. rpm runs under scratchbox2,
  which redirects absolute paths it does not recognise into the target
  rootfs. With the tree elsewhere, `mb2` writes `.mb2/spec` and rpm then
  fails to open *the same path*, because the write landed outside and
  the read went looking inside the target. A file that exists and cannot
  be opened is the signature of this. The directory must keep the
  package's name either way: `mb2` derives the package from it.
- **`mb2 build-init` before `mb2 build`**, as `mb2 --help` prescribes.
  `build` queries `.mb2/spec` within a second of starting, long before
  anything writes one.
- **`-X`**, for the reason given above -- and note it is needed *by
  `build-init`*. Without it that step gives up at version-fixing and
  never writes the spec, so `build` fails identically and the flag looks
  innocent. Each of these two was tried without the other before the
  combination was.
- **The i686 rustlib in the SDK's own `/usr/lib/rustlib`**, which is
  point 3 of the previous section arriving by a different route: mb2
  installs rust into the *target*, and build-script links resolve `/usr`
  to the SDK. The workflow copies it across from the tooling.

Not root. `sdk-manage` refuses outright ("Cannot determine Mer SDK
user") and the target snapshot never initialises. The container's own
`mersdk` is the build user, so the checkout is `chown`ed to its uid --
read from the image, not assumed -- and handed back afterwards so the
artifact upload can read the result.

One spec bug fell out of this that no host build could have found: rpm
expands macros inside comments, and a comment here mentioned `%build`,
which on the SDK's older rpm is a macro for the whole build preamble
whose first line is `LANG=C`. rpm read it as a tag and rejected the
spec. Host rpm 4.18 leaves comments alone and had parsed the same file
through an entire successful x86_64 build. `ci/packaging-lint.sh` now
checks for a bare `%` in a spec comment directly, since `rpmspec` on the
host cannot.

Still unproven: running it on a real device or emulator, and the
armv7hl build.
