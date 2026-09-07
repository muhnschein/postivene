# Postivene

*A native SailfishOS client for [Delta Chat](https://delta.chat).*

## What this is

Delta Chat is a chatmail messenger with end-to-end encryption, no phone
number and no central operator.

The thesis is narrow: **do not build a messenger, build a SailfishOS UI on
top of one.** Protocol, cryptography and storage are the upstream core's.
Postivene contributes the presentation layer, the platform integration and
the packaging, and aspires to ships to Jolla's Harbour app store.

## What this isn't

- **Reimplementing any protocol logic** — no IMAP/SMTP/MIME, no Autocrypt,
  no encryption. If protocol code is being written, the core dependency is
  being misused. This is the most important boundary in the project.
- **Hand-written C FFI bindings.** The CFFI exists; JSON-RPC is the
  sanctioned, lower-maintenance path.
- **A push-notification service.** No Delta Chat push infrastructure is
  available to third-party clients.
- **Plain-email chats.**
- **Multi-protocol bridging**, a **desktop or web build**, and **running a
  chatmail server**. Single-purpose client only.
- **Old Sailfish releases.** One modern baseline, expanded only for future
  Jolla products. [Buy a Jolla Phone 2026](https://commerce.jolla.com/) and
  support European-made alternatives. 👊🇪🇺🔥
- **Shipping via OpenRepos/Chum.** We need to improve this platform and get
  it to a point where it is competitive. That means appealing to a broad
  majority of regular, non-technical people. That also means no developer mode,
  no SSH'ing to fix small things, no community repos. Most importantly, that
  means [dogfooding](https://en.wikipedia.org/wiki/Eating_your_own_dog_food).
  Lots and lots of dogfooding - and nagging Jolla about the things that the 
  platform is still missing. This is the way.

## Architecture

```
QML / Silica UI
        |  models / signals
Rust shim (qmetaobject-rs): JSON-RPC client, event loop -> Qt queued
signals, QAbstractListModel adapters for chats/messages/accounts
        |  JSON-RPC over stdio
deltachat-rpc-server (bundled binary, subprocess) = the entire core
```

- **The shim spawns the server as a subprocess.** This keeps the integration
  surface small and stable, and mirrors the desktop client's own migration
  away from CFFI. The OpenRPC spec is the interface contract.
- **Core events run off the main thread**, marshalled to the Qt main thread
  via queued signals.
- **A webxdc app is served, not unpacked.** An app somebody sent is a zip
  with an index.html in it, and the core reads the archive
  (`get_webxdc_blob`). So the shim puts one app on a loopback address of
  its own while it is open and answers every request out of the core
  (`webxdc_host.rs`); the `WebView` is pointed at that address and needs
  nothing else. The app sits at the root of it, as every other client
  serves one: an app built with a bundler's default settings asks for
  `/assets/index-1a2b.js`, so a host that keeps the app's files under a
  prefix draws a blank screen for every app that does not happen to use
  relative paths. What the token guards is the chat, not the files --
  the two API paths are under `/webxdc-api/<token>/`, and the files were
  sent to this reader anyway. The same host carries that API --
  `sendUpdate` is a POST, the updates from everyone else are a poll -- so
  the bridge is not a Gecko frame script, and no archive format is parsed
  here.
  Where a new app comes from is the store, a website
  (`WebxdcStorePage.qml`); a tap on a link to a `.xdc` is caught before
  the engine can download it and fetched through the core instead
  (`get_http_response`), which is deltachat-android's shape too. It
  catches the tap where the tap happens: `qml/webxdc/catch.js` is a frame
  script loaded into the engine's own world, and it stops the click and
  sends the address back. deltachat-android decides every navigation in
  `shouldOverrideUrlLoading`; this `WebView` has no such hook, and
  watching where the view goes is not one -- a `.xdc` is a download, and
  a download is not a navigation, which is why the first version of that
  page did nothing on a phone.
- **The app is in the phone's share sheet, and installs nothing to be
  in it.** A picture from the gallery or a link from the browser can be
  shared to Postivene because the desktop entry says so --
  `X-Share-Methods`, and a group per method saying what it is called and
  what it takes -- and because a `ShareProvider` of the same name is
  running in the app (`qml/share/ShareTarget.qml`). The older way, a
  transfer-engine plugin, is a `.so` in a system directory and is not
  open to a Harbour package; this is, and `tests/qml_syntax.rs` checks
  that the names in the two files still agree. What arrives goes to the
  window, which asks which chat it is for and opens that chat with the
  file on its attachment bar or the text in its field.
- **The `WebView`'s own bindings are left alone.** Silica's `WebView.qml`
  decides when the engine renders from the page's status and whether the
  app is in front. Overriding `active` cost a device build: the view was
  never activated by the page transition and drew as a grey rectangle.
  `tests/qml_syntax.rs` keeps it that way. Where the view is pointed is a
  plain `url:` binding for the same sort of reason -- the store page,
  which draws, has always had one, and the app page, which did not, was
  pointed from a signal handler instead.
- **A `WebView` that draws nothing says why.** Nothing about the browser
  engine can be tested off a phone, so a failure is put where the app
  would have been: the host answers a refused blob with the core's own
  reason rather than an empty body, and the page keeps that reason on
  the screen. The banner clears itself after a few seconds, which is
  right for something that happened and wrong for a view that never drew
  anything.
- **A bubble holds a remark; anything longer gets a page.** A message
  over a dozen lines is folded in the conversation, with Expand and View
  full message under it: drawn whole, somebody's to-do document fills
  the screen, pushes the chat out of it and leaves a row nobody can
  scroll past. The fold is a cap on the label's lines rather than a cut
  in the text, so nothing has to slice a rendering in half and leave a
  tag open. And past a length the core does not carry a message whole at
  all: it cuts the body and puts the rest in an HTML part, so the whole
  of such a message is only behind `get_message_html` -- read as words,
  never rendered as markup (`html.rs`), for the reason every label in
  the app is pinned to plain text. The same rule read from the other
  end is the notice above the field while a long message is being
  written, which is where parla puts its own.
- **A file the phone cannot open is opened here.** A picture and a video
  have pages of their own; everything else used to be handed to the
  system, which for a note, a to-do list or a patch means nothing
  happens at all -- no installed app claims those and the handover fails
  without a word. Those go to a page that names the file, shows it when
  it is words, and offers the two things there are to do with it: open
  it elsewhere, or keep a copy in Downloads. Whether a file is words is
  decided once, in the shim, from what the core says it is
  (`full_text.rs`), so the row, its menu and the page cannot disagree.
- **What is made on the phone is made by the platform.** A picture or a
  video comes from QML's `Camera`; a voice message from `QAudioRecorder`,
  which QML on Qt 5.6 does not offer and the shim reaches through the
  tree's second `cpp!` block (`BUILDING.md`). Either waits in the app's
  cache directory until the core has copied it, and is sent as any other
  file -- a voice message with the core's `Voice` view type, the one kind
  the core has to be told.

## Platform baseline

- Toolchain floor **Rust 1.75.0, Qt 5.6.3** — what Sailfish ships.
- Built against the **5.2** SDK, the Jolla Phone's baseline. Anything older
  is out of scope: a binary from a newer SDK can call symbols an older
  phone lacks, and that is accepted rather than worked around. Harbour
  requires it too -- it rejects a binary that does not link
  `__libc_start_main@GLIBC_2.34`, which only a 5.x glibc provides.
- `aarch64` and `armv7hl` for devices; `i486`/`x86_64` for the emulator.
- Account storage is the core's own, pinned inside the sailjail grant at
  `$XDG_DATA_HOME/postivene/postivene/accounts` (`POSTIVENE_ACCOUNTS_DIR`
  overrides).

## What is missing

In order of what matters:

1. **Harbour-readiness.** Every rule a source tree can answer is now a
   mandatory CI gate (`ci/harbour-check.sh`, `HARBOUR.md`), and the real
   validator runs against each built RPM. One blocker remains, and it is not
   fixable here: the bundled `deltachat-rpc-server` is a second ELF
   executable, which Harbour permits nowhere.
2. **Blocking** outside a request; a media grid on the group and contact
   pages; add-as-second-device and restore-from-backup.
3. **Message polish**: avatars on bubbles, and a way to react with an
   emoji the quick row does not offer.
4. **The rest of the webxdc API.** Apps are sent, shown and run
   (`webxdc.rs`, `WebxdcPage.qml`), and status updates go both ways. What
   is not offered is the newer calls -- `sendToChat`, `importFiles`,
   realtime channels -- which are absent rather than present and failing,
   so an app that feature-tests for one takes its own other path. Nor is
   an app's `source_code_url` shown anywhere: the page has no pulley to
   put it in (a WebView cannot sit in the flickable one needs), and a tap
   on the app's own name that opens a URL its sender chose is a worse
   answer than none.
5. **The store page loads itself.** The app a reader takes from the store
   is fetched by the core, but the store's own page is loaded by the
   engine straight off the web -- so that one page does not follow
   whatever the core has been told to reach the network through, and the
   site sees the device rather than the core. deltachat-android proxies
   every request through `get_http_response`; doing the same here means
   serving the site from the shim's own loopback host and rewriting the
   links in it, which is a page-shaped guess this repository cannot test
   against.
