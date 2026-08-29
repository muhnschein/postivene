# `deltachat-rpc-server` provenance

The `deltachat-rpc-server` binaries bundled by Postivene's RPM (installed
to `/usr/libexec/postivene/deltachat-rpc-server`) are **unmodified upstream
builds**:

- **Project:** Delta Chat core (chatmail core library)
- **Source code:** https://github.com/chatmail/core
- **Version / tag:** `v2.53.0`
- **License:** MPL-2.0. Postivene itself is GPL-3.0-or-later; the two sit
  side by side in the RPM as separate works, and this file is installed
  with the package to satisfy MPL-2.0 §3.2(a)'s requirement that recipients
  of the Executable Form be told how to obtain the corresponding Source
  Code Form. See `docs/LICENSING.md`.
- **Build:** upstream's own release builds, produced by their
  `.github/workflows/deltachat-rpc-server.yml` nix builds, **statically
  linked against musl libc** ("to avoid problems with glibc version
  incompatibility", per that workflow). Target triples:
  `aarch64-unknown-linux-musl`, `armv7-unknown-linux-musleabihf`
  (hard-float, matching Sailfish `armv7hl`), `x86_64-unknown-linux-musl`.
- **Distribution channel used:** the PyPI `deltachat-rpc-server==2.53.0`
  wheels, which contain the same binaries upstream attaches to its GitHub
  release (both are the nix build output). Fetched, checksum-verified, and
  placed here by `scripts/fetch-rpc-server.sh` — that script pins the
  exact sha256 of every wheel *and* every extracted binary.

## Layout

```
vendor/deltachat-rpc-server/
  aarch64/deltachat-rpc-server   sha256 2df89ca213948e4557a11eff3ffff05efd46c0314374fc791309bd1b7fe6b769
  armv7hl/deltachat-rpc-server   sha256 d7c20192ab29b0bc80e15a464b436a0ffcc4b0e21c4f43f3fffc6c5268410645
  x86_64/deltachat-rpc-server    sha256 dcc37af7b7e95aae714c2366ff9f6d5c64170d0a7bb72cd7e3926a1a46e7e750
```

Directory names follow Sailfish's `%{_arch}` values, which is what
`rpm/postivene.spec` keys its `%install` step on. Binaries are not
committed to git (see `.gitignore`); run the fetch script to populate.

## Verified so far / still open

- [x] All three binaries confirmed statically linked and stripped
      (`file`, `ldd`).
- [x] The x86_64 binary runs and answers over stdio: the gated integration
      test `rust/deltachat-jsonrpc/tests/real_server.rs` (run with
      `DELTACHAT_RPC_SERVER=<path> cargo test -p deltachat-jsonrpc`)
      passes against it — `get_system_info` health check, account
      creation, config round trip, real event delivery, and the chat/
      message list wire shapes the UI depends on.
- [ ] `aarch64`/`armv7hl` binaries exercised on an actual Sailfish device
      or emulator (static musl linking means only the kernel ABI matters,
      so these are expected to run as-is — but this is the one claim that
      can only be proven on hardware).

## Updating

Bump `VERSION` and all sha256 pins in `scripts/fetch-rpc-server.sh`
together, re-run it, and update this file to match. The version bundled
and the version stated here must never drift apart.
