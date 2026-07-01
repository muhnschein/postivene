# `deltachat-rpc-server` provenance

**Status: not yet populated.** This directory is where a `deltachat-rpc-server`
binary for each target architecture should live (`aarch64/`, `armv7hl/`, and
optionally `x86_64/`/`i486/` for the emulator), matching
`rpm/postivene.spec`'s `%install` step
(`vendor/deltachat-rpc-server/%{_arch}/deltachat-rpc-server`). This is
Milestone 1 in `docs/MILESTONES.md` and has not been done in this
environment (no Sailfish SDK, device, or `chatmail/core` build toolchain
available here).

## What needs to happen here

1. Either:
   - Obtain a prebuilt `deltachat-rpc-server` release binary from
     upstream `chatmail/core` for the target triple, **and confirm it
     actually runs unmodified on a Sailfish target** (the scope's open
     question in `docs/SCOPE.md` §9) -- Sailfish's userland/libc and
     linked library versions may not match a generic Linux build; or
   - Cross-compile it from source against the Sailfish SDK's target
     sysroot, following the Rust cross-compilation notes in
     `docs/SCOPE.md` §7 (Docker build engine required; VirtualBox does
     not support the Rust toolchain there).
2. Place the resulting binary at
   `vendor/deltachat-rpc-server/<arch>/deltachat-rpc-server` for each
   architecture being packaged.
3. **Record exactly which upstream commit/tag was built**, right here in
   this file (replace this whole document with: upstream repository URL,
   exact git commit hash or release tag, build date, and target
   triple(s)). This isn't optional bookkeeping -- MPL-2.0 §3.2(a) requires
   that recipients of the Executable Form be told how to obtain the
   corresponding Source Code Form, and this file is what
   `rpm/postivene.spec` installs to `/usr/share/postivene/vendor/deltachat-rpc-server/SOURCE.md`
   to satisfy that. See `docs/LICENSING.md`.

## Why this isn't just committed as a binary blob

Binaries are architecture-specific, change with every core update, and
(per the above) still need on-device verification before they can be
trusted -- there's no "fetch once and forget" shortcut here. A follow-up
increment should add a small fetch/verify script
(`scripts/fetch-rpc-server.sh`, checksum-pinned to a specific upstream
release) rather than committing binaries directly to git.
