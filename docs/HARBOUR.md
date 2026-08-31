# Harbour

Postivene is packaged for [Jolla's Harbour store](https://harbour.jolla.com).
Harbour's rules are not advice: a validator failure is a guaranteed
rejection, and several of them constrain things — the package name, the
install paths, the linker flags — that are expensive to change once code
is built around them.

So they are a CI gate. `ci/harbour-check.sh` runs on every pull request
and is mandatory.

## The two checks

| | `ci/harbour-check.sh` | `sfdk check -s harbour` |
|---|---|---|
| Runs | every pull request (`ci.yml`) | when an RPM is built (`rpm.yml`) |
| Reads | the source tree | the built package |
| Needs | rpm, file, binutils, a host build | the Sailfish SDK, minutes of runner time |
| Authority | no | **yes** |

The second is Jolla's own `rpmvalidation.sh`, fetched and run against the
RPM the SDK produces. It is the one that decides. The first exists because
the second cannot run on every push, and because a rule broken in a pull
request is cheaper to fix than one discovered at intake.

They are kept honest against each other: `ci/harbour/` holds the
validator's own allow-lists, copied verbatim by
`scripts/update-harbour-rules.sh`, and the `rpm.yml` step warns if those
copies have fallen behind upstream. `ci/harbour-check.sh` reimplements the
logic around them, from `rpmvalidation.sh`.

## What the source check covers

Check IDs follow Jolla's own numbering.

**Naming** (1.1) — the `harbour-` prefix and lowercase package name;
Version digits and periods only; Release digits, underscores and periods,
*including the Release `rpm.yml` stamps onto each build*; only Harbour
architectures offered.

**Layout** (1.2) — every path in `%files`, for each device architecture,
against the four locations Harbour permits; the .desktop file and the
binary present; nothing under `/home`; no debug directories; no
world- or group-writable, setuid or setgid install modes.

**The .desktop file** (1.3) — a non-empty `Name=`; `Exec=` and `Icon=`
exactly the package name; `Type=Application`;
`X-Nemo-Application-Type=silica-qt5`; `[X-Sailjail]`, never `[Sailjail]`,
and never empty.

**Sailjail** (1.4) — only the four allowed keys; `OrganizationName` and
`ApplicationName` against their regexes and the reserved-name list; every
permission on the whitelist; `ExecDBus` agreeing with `Exec`.

**Icons** (1.5) — all four sizes present, real PNGs, pixel dimensions
matching their directory names.

**QML** (1.6) — every import against the allow-list and the blocked-prefix
patterns; no absolute-path imports; relative imports resolving inside the
installed tree; no ELF file outside the two places one may live.

**The binary** (1.6.1, 1.7) — every linked library against the allowed
list; that it links `__libc_start_main`; and that it **exports `main()` as
a dynamic symbol**. See below.

**RPM metadata** (1.8) — no `Vendor:`; no `Provides:`, `Obsoletes:`,
`Conflicts:`, `Recommends:`, `Suggests:`, `Supplements:` or `Enhances:`;
every `Requires:` unversioned and on the allowed list; no scriptlets or
triggers; `libsailfishapp-launcher` required if and only if the
`sailfish-qml` launcher is used; `qt5-qtdeclarative-import-xmllistmodel`
required if `QtQuick.XmlListModel` is imported.

**Runtime policy** (2.1, 2.5, 2.6) — no hardcoded `/home/nemo` or
`/home/defaultuser`; the data path the app builds spelled the same way as
the sandbox grant it depends on; nothing written to a path the package
installs.

`ci/harbour-check-selftest.sh` breaks each of these in a throwaway copy of
the tree and asserts the check names it. A gate that only ever prints
"ok" is indistinguishable from one that has stopped looking.

## What it cannot cover

Anything that needs the built package or a device, which is everything
below. `rpm.yml`'s validator step covers the first group; the rest is
§10's pre-submission sequence.

- The `Requires:` and `Provides:` **rpm generates** from the binary, as
  opposed to the ones the spec states.
- The RPATH (1.6.3), and the real file modes and ownership in the package.
  Linked libraries are checked, but against the *host* build — close
  enough for the Qt and C++ dependencies, which is what the rule is about.
- That the app works under Sailjail. Running it from a terminal or the
  IDE bypasses the sandbox entirely, so a missing permission does not
  surface until QA installs it. Force it: `sailjail /usr/bin/harbour-postivene`.
- Everything in the quality bar QA applies by hand — no placeholder
  content, translated strings, `Theme` values rather than pixel counts,
  recoverable errors, a useful cover.

## Exporting `main()`

Harbour rejects a Silica app whose binary does not export `main()`: the
`silica-qt5` booster in mapplauncherd `dlopen()`s the binary and looks the
symbol up dynamically. C++ apps mark it `Q_DECL_EXPORT`.

Rust has no equivalent. `fn main` becomes an ordinary global symbol, which
lives only in `.symtab` — and rpmbuild strips `.symtab` on the way into
the package, so by the time Harbour looks there is nothing there.
`rust/postivene-app/build.rs` passes `--dynamic-list` at link time to put
`main` in `.dynsym`, where stripping cannot reach it.

`--dynamic-list` rather than `--export-dynamic-symbol`, which needs
binutils 2.35 and so may not exist in the SDK, or `--export-dynamic`,
which would export every symbol in the binary.

## Waivers

`ci/harbour/waivers.conf` records rules this package knowingly breaks, one
line each with a reason. Nothing belongs there that can be fixed: every
entry is a submission blocker.

A waiver that stops matching anything fails the check, so the file cannot
outlive what it excuses. Its entries cover the two blockers below.

## The open blockers

**Postivene cannot be submitted to Harbour today.** Two rules are broken.
Both are structural, both are waived in `ci/harbour/waivers.conf`, and
neither can be fixed by editing anything in this repository.

### 1. QtWidgets

The binary links `libQt5Widgets.so.5`, which is not on Harbour's
allowed-libraries list — a Silica app is expected to use QtGui's
`QGuiApplication`. This is not our code: `qmetaobject-rs` builds its QML
engine on `QApplication`, from `QtWidgets`:

```cpp
#include <QtWidgets/QApplication>       // qmetaobject/src/qtdeclarative.rs
...
: app(new QApplication(argc, argv))
```

and `qttypes`' build script links the library unconditionally, not behind
a feature:

```rust
link_lib("Core");
link_lib("Gui");
link_lib("Widgets");                    // qttypes/build.rs
```

Both are still that way on upstream master, so there is no version to
upgrade to. `--as-needed` does not help twice over: the symbols
(`QApplication::QApplication`, `QApplication::exec`) really are
referenced, and an explicit `-lQt5Widgets` records a `NEEDED` entry
whether or not anything uses it.

Fixing it means patching both crates — `QGuiApplication` in place of
`QApplication`, and dropping the `Widgets` link — via `[patch.crates-io]`
against forks, and ideally upstreaming a feature flag. It is the same
change either way; the question is only who carries it.

This one is worth knowing before any more is built on qmetaobject: it
constrains every Sailfish app written with it, which may be why the
Rust/Qt apps on Sailfish live on OpenRepos and Chum rather than Harbour.

### 2. The bundled server

`deltachat-rpc-server` is an ELF executable, bundled at
`/usr/libexec/harbour-postivene/`, spawned as a subprocess and spoken to
over JSON-RPC on stdio. Harbour allows ELF files in exactly two places:
`/usr/bin/<NAME>`, and `*.so` under `/usr/share/<NAME>/lib/`. Neither fits
a second executable, so this is two validator errors — the path, and the
binary in it.

The clean fix is upstream's own shared library. `deltachat-ffi` in
[chatmail/core](https://github.com/chatmail/core) builds `libdeltachat.so`,
and its `dc_jsonrpc_init` / `dc_jsonrpc_request` /
`dc_jsonrpc_next_response` API carries **the same JSON-RPC protocol** the
shim already speaks — so only `rust/deltachat-jsonrpc`'s transport would
change, not the message shapes above it. At
`/usr/share/harbour-postivene/lib/libdeltachat.so`, with the binary's
RPATH set to `$ORIGIN/../share/harbour-postivene/lib`, that is exactly the
arrangement rules 1.6.2 and 1.6.3 describe.

Two things make it a project rather than a packaging change:

- **No prebuilt exists.** Upstream ships the rpc-server binary through
  PyPI wheels (`scripts/fetch-rpc-server.sh`) but publishes no
  `libdeltachat.so` for `aarch64` or `armv7hl`.
- **The static-musl trick does not transfer.** The bundled rpc-server runs
  on any Sailfish release because it is statically linked against musl. A
  shared library cannot be: it has to link the same libc as the process
  that loads it. So `libdeltachat.so` must be cross-built against a
  Sailfish glibc sysroot — with Rust ≥ 1.89, since core v2.53.0 declares
  `edition = "2024"`, well above the 1.75 the SDK ships.

Until both are resolved the gate's value is that it catches every *other*
rule, and any new violation of these two.

## Before submitting

1. Build an RPM per architecture (`rpm.yml`); the validator step runs
   automatically.
2. Read every warning, not just the errors — several describe things that
   will be dropped in a future release.
3. Install on a real device and launch it as
   `sailjail /usr/bin/harbour-postivene`.
4. Exercise every permission-dependent path under the sandbox: the
   profile picture picker needs both `Pictures` and `MediaIndexing`.
5. Delete the cache directory while the app runs; confirm nothing breaks.
6. Confirm **Version** was bumped, not just Release. Harbour refuses an
   update that does not sort higher than the one in the Store, and a
   Release-only bump is the most common avoidable resubmission.
7. Set "From OS version" to 4.5.0 on the submission form. The spec cannot
   say so — `sailfish-version` is not an allowed dependency, and a
   versioned one would be rejected twice over — but the `[X-Sailjail]`
   section needs it.
