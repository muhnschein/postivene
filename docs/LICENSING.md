# Licensing

## Postivene's own license

Postivene — the QML UI, the Rust JSON-RPC transport and Qt shim, the
packaging, the docs — is released under the **GNU General Public License,
version 3 or (at your option) any later version** (`GPL-3.0-or-later`,
"GPLv3+"). The full text is in [`LICENSE`](../LICENSE).

    Copyright (C) 2026 Postivene contributors

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

Every file in this repository is under `GPL-3.0-or-later` unless that file
says otherwise. Nothing currently says otherwise; `vendor/` is fetched, not
committed, and carries its own terms (below).

The project was originally released under MPL-2.0. It was relicensed to
GPLv3+ while every line was still held by a single copyright holder, so no
third-party consent was required.

### Why GPLv3+

- **It matches the app it is a client for.** Delta Chat's own Android app,
  [`deltachat/deltachat-android`](https://github.com/deltachat/deltachat-android),
  is GPLv3+. Postivene is the same kind of artefact — a native UI over the
  same core — and the same terms are the least surprising ones for users and
  contributors arriving from that side. (`deltachat-ios` is MPL-2.0; the
  family does not pick one license for its front-ends.)
- **A messaging client is exactly what strong copyleft is for.** The value
  a user gets from a chat app depends on being able to verify and rebuild
  the thing handling their messages. GPLv3+ makes that guarantee survive
  redistribution — an improved-but-closed Postivene fork on a phone someone
  else controls is the case worth foreclosing. MPL-2.0's file-level copyleft
  does not: it lets a "Larger Work" around the same files ship under any
  terms at all.
- **GPLv3 specifically**, not GPLv2: §6's installation-information and
  anti-tivoisation terms are the ones that bite on a phone, and §7 gives a
  clean way to grant additional permissions later (see *Sailfish Silica*).
- **`-or-later`**, not `-only`: it keeps a future FSF revision available
  without needing to re-poll contributors, which is the whole reason the
  relicense was cheap this time.
- Nothing in the dependency graph or the runtime stack argues against it
  (see the two sections below).

## Upstream: Delta Chat core

`chatmail/core` (which builds `deltachat-rpc-server`) is licensed under the
**Mozilla Public License 2.0 (MPL-2.0)** — not GPL, despite what older notes
in this tree used to say. MPL-2.0 is a file-level copyleft: modifications to
*its own* source files stay under MPL-2.0, but it explicitly permits
combining Covered Software into a "Larger Work" under different terms
(§3.3).

Two independent reasons this is fine next to GPLv3+ code:

1. **It is a separate process.** Postivene never modifies or vendors core
   source; it spawns the prebuilt `deltachat-rpc-server` executable and
   talks to it over JSON-RPC on stdio. Separate process, documented
   protocol, arm's-length IPC — the pattern that is widely understood not to
   create a single combined work. On the distribution medium the two are
   mere aggregation (GPLv3 §5, final paragraph).
2. **Even as a combined work it would be compatible.** MPL-2.0 §1.12 and
   §3.3 name the GPL family as "Secondary Licenses": MPL-covered files may
   be distributed as part of a Larger Work under GPLv3, provided they are
   not marked "Incompatible With Secondary Licenses" (MPL Exhibit B).
   `chatmail/core` carries the ordinary Exhibit A notice, not Exhibit B, so
   the mechanism is available. The core's own files keep their MPL terms
   either way.

Note the direction of travel: MPL-2.0 → GPLv3 works, GPLv3 → MPL-2.0 does
not. Postivene must therefore never grow a code path that copies core source
*into* this tree expecting to relicense it, and any patch to core itself
belongs upstream under MPL-2.0.

## Runtime stack: Qt and Sailfish Silica

- **Rust dependencies** are all permissive (MIT / Apache-2.0 / BSD / ISC /
  Zlib / Unicode-3.0), every one of which may be conveyed as part of a
  GPLv3 work. `rust/deny.toml` enforces the allow-list and CI runs
  `cargo deny`, so a dependency arriving under something else fails the
  build rather than quietly becoming a distribution problem.
- **Qt 5.6** as Sailfish ships it is LGPL (v2.1 and/or v3). LGPLv2.1 §3
  permits relicensing a copy under "GPL version 2 or any later version", and
  LGPLv3 is written as a set of additional permissions on top of GPLv3, so
  both are GPLv3-compatible. Linking Qt from a GPLv3+ application is
  ordinary and uncontroversial.
- **Sailfish Silica** (`import Sailfish.Silica 1.0`, `sailfishsilica-qt5`)
  is Jolla's, and closed-source. This is the one place worth being explicit
  about, and it is not a blocker:
  - Silica is not linked. It is a QML module resolved *at runtime* by the
    Qt QML engine, which GPLv3 §1 names outright as a Major Component ("an
    object code interpreter used to run it").
  - Silica ships as part of Sailfish OS itself — the platform's UI toolkit,
    included in the normal packaging of the operating system the work runs
    on, and serving only to let the work run against it. That is the
    "System Libraries" carve-out in GPLv3 §1, the same reasoning that lets
    GPL software target any platform with proprietary system libraries.
  - The ecosystem has already settled this in practice. Whisperfish — the
    Rust + Qt5/QML Sailfish app this repo uses as its architectural and
    packaging template (`docs/SCOPE.md` §4, `rpm/harbour-postivene.spec`) — is
    **AGPLv3**, and its spec carries the identical
    `Requires: sailfishsilica-qt5` line.
  - If it is ever contested, GPLv3 §7 allows an explicit additional
    permission (a "linking exception" naming Silica) to be added without
    changing the license. That is a one-paragraph change, deliberately
    *not* made pre-emptively: it gives away copyleft that nothing has yet
    asked for.

## Obligations when distributing

**For Postivene itself (GPLv3 §4–§6).** Recipients of a binary must be able
to get the Corresponding Source, under GPLv3, by one of §6's routes. The
intended route is §6(d): the source is this public repository, and the
package points at it (the spec's `URL:`, plus the installed `LICENSE` and
`docs/LICENSING.md`). For that to actually hold, an RPM must be built from
an immutable tag whose tree is what `rpm/harbour-postivene.spec`'s `Version:` says
— so tags are not to be moved, and the version is to be bumped with them.
Where a channel (OBS/Chum, OpenRepos) distributes the RPM away from the
repository, those installed pointers are the "clear directions next to the
object code" §6(d) asks for.

**For the bundled `deltachat-rpc-server` (MPL-2.0 §3.2(a)).** Because the RPM
bundles the Executable Form, recipients must be told how to obtain its
Source Code Form. `rpm/harbour-postivene.spec` installs
`vendor/deltachat-rpc-server/SOURCE.md`, which names the exact upstream tag
and the sha256 of every binary. This must be kept in sync whenever the
bundled binary is updated — `scripts/fetch-rpc-server.sh` pins the version
and checksums in one place, and the app's future "About" page should point
at `https://github.com/chatmail/core` at that tag too. The duty to do this
is Postivene's as the distributor, but it flows from upstream's license
rather than ours, so the relicense changes nothing about it.

**In the RPM metadata.** The package's `License:` tag covers the contents of
the *binary* package, so on the architectures that bundle the server it
reads `GPL-3.0-or-later AND MPL-2.0`, and `GPL-3.0-or-later` alone where no
server is bundled (i486, for which upstream publishes no build).

## Not yet settled

- Trademark/name clearance for "Postivene" is out of scope for this
  document.
- Should Postivene ever link GPL-incompatible code *directly* — in-process,
  not over the JSON-RPC boundary — this analysis has to be redone before
  that ships. `cargo deny` catches the Rust half of that automatically; the
  C++/Qt half is a review question.
