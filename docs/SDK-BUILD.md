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
