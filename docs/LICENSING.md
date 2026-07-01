# Licensing

## Upstream: Delta Chat core

`chatmail/core` (which builds `deltachat-rpc-server`) is licensed under the
**Mozilla Public License 2.0 (MPL-2.0)**, not GPL. MPL-2.0 is a file-level
copyleft: it requires modifications to *its own* source files to stay under
MPL-2.0, but explicitly permits combining Covered Software into a "Larger
Work" under different terms (MPL-2.0 §3.3). Proprietary or differently
licensed code does not become MPL-2.0 merely by being distributed alongside
or linked against MPL-2.0 code.

Postivene never modifies or vendors core source; it consumes the prebuilt
`deltachat-rpc-server` executable and talks to it over JSON-RPC on stdio, an
arm's-length IPC boundary rather than static/dynamic linking. Even under
stricter copyleft licenses this pattern (separate process, communicating over
a documented protocol) is widely understood not to create a combined/derived
work. Under MPL-2.0's own "Larger Work" language it is unambiguous.

## Postivene's own license

Postivene (the QML UI, the Rust JSON-RPC shim, packaging) is released under
**MPL-2.0** (see `LICENSE`), matching upstream:

- Keeps the whole stack under one well-understood, OSI-approved license.
- File-level copyleft is a low-friction fit for a community Sailfish app:
  contributions to Postivene's own files stay open, without imposing
  copyleft on unrelated code someone might combine it with.
- Compatible with distributing the bundled `deltachat-rpc-server` binary
  alongside the RPM (see `rpm/postivene.spec` for how the binary is
  packaged and its own license/source-availability notice is carried).

## Obligations when packaging the RPM

Because the RPM bundles the `deltachat-rpc-server` Executable Form, MPL-2.0
§3.2(a) requires that recipients be told how to obtain its Source Code Form.
`rpm/postivene.spec` and the app's "About" page must point at the exact
upstream source tag/commit the bundled binary was built from
(`https://github.com/chatmail/core`). This must be kept in sync whenever the
bundled binary is updated — see `vendor/deltachat-rpc-server/SOURCE.md`.

## Not yet settled

- Trademark/name clearance for "Postivene" is out of scope for this document.
- If a future dependency pulls in GPL-2/GPL-3 code directly (not just via a
  separate subprocess), this analysis must be revisited — MPL-2.0's
  Secondary License mechanism (§1.12, §3.3) allows combining with GPL family
  licenses, but the resulting combined work's terms would need re-checking.
