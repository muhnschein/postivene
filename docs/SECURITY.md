# Security: what guards the app, and what was evaluated

What stands between a hostile message and this phone today, and what was
looked at for tightening it further -- seccomp-bpf, Landlock, and
decoding media in a sandbox -- with a verdict on each. The roadmap asked
for the evaluation before anything was built; this is it, kept so the
reasoning is not redone the next time.

## What is in place

- **The core is a separate process.** Everything that touches the
  network or parses mail -- IMAP, SMTP, MIME, OpenPGP -- is upstream's
  `deltachat-rpc-server`, spawned by the app and spoken to over JSON-RPC
  on stdio. The binary is a release upstream built, fetched by
  `scripts/fetch-rpc-server.sh` against a pinned checksum. The app never
  holds a key or a socket to a mail server.
- **Sailjail.** Launched from the launcher, the app runs in the sandbox
  the desktop file declares: `Internet`, `Pictures`, `MediaIndexing`,
  `UserDirs`, `Audio`, `Camera`, and nothing else. Sailjail is Firejail
  underneath, which brings a private mount namespace, a D-Bus proxy that
  admits only the names the permissions grant, and Firejail's own
  seccomp filter, which drops the syscalls no application should make
  (mount, kernel modules, ptrace, raw I/O, and so on). The child inherits
  all of it: the core runs in the same jail as the app.
- **No code from the network.** The app draws what the core hands it:
  text as `PlainText` (pinned by `tests/qml_syntax.rs`), Markdown through
  its own renderer, links opened by the platform. Webxdc apps are not run
  (docs/PROJECT.md).
- **No unsafe outside one file** in the app's own code
  (`unsafe_code = "deny"` in the workspace lints; the one allowance is
  `postivene-app/src/translations.rs`). The vendored `qmetaobject` is
  upstream plus a three-line patch, proven by `ci/vendor-check.sh`.

## seccomp-bpf, with a whitelist generated in CI

**Feasible, but not for the app, and not from CI.**

The mechanism is available: every kernel a Sailfish phone ships has
seccomp-bpf (Android has required it since 8.0), and a filter is set with
`prctl(PR_SET_NO_NEW_PRIVS)` followed by `prctl(PR_SET_SECCOMP,
SECCOMP_MODE_FILTER, &prog)`, both reachable through the `libc` crate the
tree already depends on. Harbour's library list has no `libseccomp`, so
the BPF program would be assembled by hand -- a whitelist filter is
around forty instructions, which is the easy part.

What is not easy is the whitelist. Three problems:

1. **The app's syscall profile is not ours to know.** Qt, EGL through
   libhybris, the Mali driver, PulseAudio, the camera stack and the
   thumbnailer all make syscalls this tree never sees -- `ioctl`s with
   driver-specific arguments, `membarrier`, `memfd_create`, whatever the
   next platform update adds. A whitelist that misses one kills the app
   with `SIGSYS`, on a device, in a code path that only runs when the
   camera is opened or a video plays. Firejail's filter is a blacklist
   for exactly this reason.
2. **CI runs on x86_64; the phone is aarch64.** Syscall numbers and, in
   places, syscall *names* differ (`open` is `openat`, `fork` is `clone`,
   `select` is `pselect6` on arm64). A list recorded under `strace` on the
   runner is not the list the device needs. It could be recorded on a
   device and checked in -- but then it is recorded from one phone, one
   OS version, one set of paths taken.
3. **Threads.** A filter applies to threads created after it is
   installed (or to all of them with `SECCOMP_FILTER_FLAG_TSYNC`). Qt's
   render thread, the tokio runtime and the thumbnailer's threads all
   start after `main` begins, so the filter has to be complete before
   the first line of the app runs, which is problem 1 again.

The process worth filtering is the other one. `deltachat-rpc-server`
is what parses mail from strangers, and its syscall profile is narrow
and portable: files under one directory, TCP sockets, threads, clocks,
`getrandom`. A filter installed in `Command::pre_exec` before the child
`exec`s would cover it without touching the app's own process. That is
the shape this should take if it is taken up: the list recorded from the
server alone (`strace -f -qq -e trace=all` around the `real_server`
suite gives its names; `libc`'s `SYS_*` constants map them per
architecture at build time), the filter applied to the child, and a
`SIGSYS` in the child surfacing as the core going away, which the
supervisor already handles by restarting it. Cost: one `unsafe` block
for `pre_exec`, an allowance in the lints, and a device to test on.

**Verdict:** not now. Firejail's filter already covers the syscalls that
matter most, and a whitelist on the app is a crash waiting for a
platform update. A whitelist on the core is worth doing once a device
has recorded its profile, and the developer view now writes the script
that records it: `strace.sh` in every recording attaches from outside
the sandbox, as root over SSH, and lists the distinct syscalls of each
process (docs/BUILDING.md, "Profiling on a device"). The core's list
from a session that sends, receives, downloads and plays is the
whitelist's first draft.

## Landlock, best-effort

**Feasible in principle; unknown on the device; worth a probe.**

Landlock restricts what a process can do with the filesystem (and, since
ABI 4, TCP) after it has started, and unlike seccomp it fails safe: a
rule that is too tight denies a file access, which shows up as an error
rather than a dead process. It needs a kernel of 5.13 or later with the
LSM enabled. The Jolla Phone 2026's kernel is an Android GKI one of
that generation, but whether `landlock` is in
`/sys/kernel/security/lsm` is a fact to read off the device, not to
assume; Android's own configurations do not enable it.

What it would buy: the core confined to its accounts directory, the
blob directory under it, and the TLS roots -- so a bug in mail parsing
cannot read the reader's photos or the other apps' data, which Sailjail's
`Pictures` and `UserDirs` grants otherwise leave open to both processes.
The app itself gains less: it needs the pickers' directories by design.

The calls are three raw syscalls (`landlock_create_ruleset`,
`landlock_add_rule`, `landlock_restrict_self`), reachable through
`libc::syscall` with the numbers from the `libc` crate, in
`Command::pre_exec` for the child. Best-effort is the right posture:
`ENOSYS` or `EOPNOTSUPP` means carry on without it, logged once.

**Verdict:** worth a probe on a device, and the developer view makes it:
`system.txt` in every recording carries the LSM list and a `Landlock:`
line that reads it and, as a second opinion, the kernel's symbol table
(docs/BUILDING.md, "Profiling on a device"). If it says enabled,
confining the core is a contained change: one `pre_exec`, one lint
allowance, no new dependency. If it says absent, there is nothing to
build.

## Sandboxed media decoding

**Not available under Harbour's rules; the cheaper defences are.**

glycin's model is a loader process per image, in a bubblewrap sandbox
with its own seccomp filter, handing the decoded pixels back over a
socket. Sailfish has no bubblewrap, and Harbour allows an app one
executable -- the bundled core already breaks that rule, and a decoder
helper would be a second exception to ask Jolla for. Qt's image plugins
(`libpng`, `libjpeg`, Qt's own GIF reader) decode in the app's process,
so a crafted image that exploits one runs with the app's Sailjail grants.

What can be done inside the process:

- **Decode only what the core classified.** A message is drawn as an
  image only when the core's `viewType` says so, and never from a MIME
  type the sender chose -- already the case.
- **Bound the decode.** `sourceSize` on the row's picture and on the
  full-screen page caps the pixels a file can make the app keep: a memory
  bound, and a brake on decompression bombs as far as Qt's decoders
  honour it -- JPEG scales as it decodes, PNG decodes in full and scales
  after. See the profiling notes in docs/BUILDING.md; the same bound
  serves both.
- **Do not decode for strangers.** A chat that is still a contact
  request could draw its attachments as placeholders until it is
  accepted, so the first thing an unknown sender's file does is not run
  a decoder. Not done yet; a small change to the delegate.

**Verdict:** the helper process is the real answer and is closed off by
the same rule as the background service (docs/PROJECT.md, "What is
missing"). The three points above are the part that can be had now; the
third is the one still open.

## Where this leaves things

In order of value for effort: Landlock on the core (read `system.txt`
off a device, then one contained change), placeholders for contact
requests' attachments, then seccomp on the core from the list
`strace.sh` records. Nothing here changes the app's own sandbox, which
is Sailjail's to provide.
