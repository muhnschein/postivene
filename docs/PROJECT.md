# Postivene

*A native SailfishOS client for [Delta Chat](https://delta.chat).*

What the project is, what it deliberately is not, and where it currently
stands. Build and test procedure is in [`BUILDING.md`](BUILDING.md).

## What this is

Delta Chat is an email-based ("chatmail") messenger with end-to-end
encryption, no phone number and no central operator.

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
  majority of regular, non-technical people. That means also no developer mode,
  no SSH'ing to fix small things, no community repos. Most importantly, that
  means [dogfooding](https://en.wikipedia.org/wiki/Eating_your_own_dog_food).
  Lots and lots of dogfooding - and nagging Jolla about the things that the 
  platform is still missing. This is the way.

## Architecture

```
QML / Silica UI  +  background service (planned)
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
- **Background reception** relies on IMAP IDLE plus a Sailfish background
  process. It is the hardest platform problem and the largest open risk.
- **A chat loads a page at a time.** The ids are cheap and the messages are
  not: `get_message_list_items` returns a number per message for the whole
  chat, while `get_messages` builds every field of every row. So the model
  holds every id and fetches a window of messages out of that list, which
  is why paging needs no cursor and no server-side paging call. The window
  is anchored on *ids*, not on counts: one message arriving shifts every
  count-based slice by one, which drops the oldest loaded row off the front
  and reads as a deletion. Rows inserted above the ones on screen also
  carry the view with them -- Qt does not put it back, so
  `ConversationList` does, by index rather than by pixels.
- **The window has two ends.** It starts at the newest message and takes in
  arrivals, which is the ordinary case and the one `has_newer` is false
  for. Reaching somewhere it does not cover -- the beginning of the
  history, a search result from last March -- *moves* it rather than
  growing it back to today, because growing it back is the cost paging
  exists to avoid. A window that has been moved off the end stops taking in
  arrivals: swallowing them would drag a reader who went looking for
  something old back to today one message at a time. Sending from there
  moves it back, because a message you just sent is one you should be able
  to see.
- **The chat is on the page before the page arrives.** `ChatPrefetch` loads
  it while the reader is still looking at the chat list, and
  `ConversationPage` takes it in `Component.onCompleted` -- after
  `reading_history` is bound, or the model reads the default and marks the
  chat read behind a page nobody has seen. A prefetch hit is a move, not a
  fetch. When a search result is what was tapped, the prefetch loads the
  page *that message* is on, so the page never shows today and jumps.
- **A row jumped to has to be held, not jumped to.** One
  `positionViewAtIndex` is enough only where every row is already its final
  height, which is nowhere real: rows are measured as they are laid out,
  wrapped text at the device's own metrics is taller than an estimate, a
  picture's row changes height again when the picture decodes, and the
  header above the oldest row collapses when there is no more history to
  offer. Each of those that happens above the reader moves them, and they
  all happen after the jump. So `holdAt` re-applies the row on every change
  to the content, until the reader takes the view over or it stops moving
  under them. Landing on a search result, on the beginning of the chat, and
  back on the place a full-screen picture was opened from are all this.
- **A page pushed over the conversation takes its place with it.** The list
  is torn down far enough to forget where it was, and replaces what it
  forgets with the beginning. `rememberPlace` on `Deactivating` and
  `restorePlace` on `Active` put it back -- as a row, not a pixel offset,
  for the same reason a step back through the history is.
- **Zoom is about the point that was touched.** A picture that grows about
  its own top-left corner takes a reader who pinched a face in the bottom
  right to the top left instead. `PicturePage.zoomAt` works out where in
  the picture the fingers are, changes the zoom, and puts the view back so
  the same point is under them.
- **The top of the list offers the beginning of the chat.** Not decoration:
  the system's own scroll-to-top gesture goes to the top of what is loaded
  and gives no sign that it is not the top of the chat. That is where the
  gesture leaves the reader looking, so that is where the way to the real
  beginning goes.
- **One send at a time.** The compose state clears when the core answers,
  not when the button is tapped, so a send that fails leaves the reader
  holding what they chose. `ChatMessages.sending` closes the window that
  opens up in between: copying a large video into the core's blob directory
  takes seconds, and a second tap in those seconds used to send the whole
  thing again.
- **Pictures and video open in the app.** Handing them to the system took
  the reader out of Postivene to something that then failed to show them.
  A picture gets a page with a pinch-zoom flickable; a video gets
  QtMultimedia, which is already how a voice message plays in its own row.
  Everything else is still somebody else's file: a page here that could
  only say "cannot show this" would be worse than the handover. The way
  out to another app stays in each viewer's pull-down.
- **A photo is shown the way it was taken.** `Image.autoTransform` reads
  the EXIF orientation tag, which is where a camera records the turn
  rather than applying it to the pixels. The row is measured from the
  decoded picture rather than from the core's dimensions for the same
  reason: the core reads the file's header, so its answer is the size
  before the turn.
- **The core classifies attachments, not the app.** Every file goes to
  `misc_send_msg` and comes back with a `viewType` the core chose from the
  file itself; `AttachmentPreview` picks a renderer from that answer and
  nothing here inspects a file. What the core leaves blank matters as much:
  it reports no dimensions for a GIF and no duration for a sound file, so
  the row sizes pictures from the decoded image and lets the audio player
  report its own length (`deltachat-jsonrpc/tests/real_server.rs` pins
  both). It also declines to call a `.vcf` a contact card unless the file
  holds exactly one contact *with an email address* -- a phone-only contact
  exported from the address book is neither, and is not someone Delta Chat
  could open a chat with anyway -- so those land on the file row, which
  marks them as cards rather than as anonymous blobs.
- **The server is supervised.** Its event stream ending is the app's only
  notice that the core has gone -- a phone reclaiming memory kills it and
  says nothing -- so that is where the next one is started, with a backoff
  that resets after a healthy run, and IO resumed for whatever accounts were
  running. Twelve failures without a healthy run in between is a core that
  will not start, which the app says rather than retrying forever.

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

## What works

Upstream's release binaries are statically linked against musl, so no
Sailfish-specific core build is needed; `scripts/fetch-rpc-server.sh`
fetches v2.53.0 sha256-pinned, and the wire shapes are verified against
the real binary offline.

On top of that: the chat list (unread badges, timestamps, avatars,
encryption/pin/mute marks, context menu, search across chats/contacts/
messages, archive, contact requests, multiple profiles), the conversation
view (bubbles, quotes, delivery marks, day separators, reply/copy/delete/
resend, a page of history at a time, and every kind of attachment the core
classifies: photos and
stickers inline, GIFs animated over a still poster, a video's poster frame
from the platform thumbnailer, voice and audio played where they sit, a
shared contact as a card, everything else named and sized), onboarding
rebuilt on the core's current transport API, `secure_join` invites in both
directions, encryption indicators, foreground notifications, and the
cover.

Packaging is real: `mb2` builds produce `harbour-postivene-0.1.0-1.aarch64.rpm`,
and `.github/workflows/rpm.yml` builds it unattended on a GitHub runner in
about six minutes.

## What is missing

In order of what matters:

1. **Harbour-readiness.** Every rule a source tree can answer is now a
   mandatory CI gate (`ci/harbour-check.sh`, `HARBOUR.md`), and the real
   validator runs against each built RPM. One blocker remains, and it is not
   fixable here: the bundled `deltachat-rpc-server` is a second ELF
   executable, which Harbour permits nowhere.

   Of the three ways to hold the core, each is blocked differently.
   `libdeltachat.so` would satisfy the rule, but upstream is deprecating
   it. The `deltachat-jsonrpc` crate is its supported replacement, but
   needs Rust 1.89 where the SDK ships 1.75, and Rust will not link output
   from two compiler versions. The subprocess we use is the one upstream
   recommends. So the next move is to put the case to Jolla rather than to
   engineer around them.

   The QtWidgets blocker is gone: `third_party/qmetaobject` is upstream
   plus a three-line patch swapping `QApplication` for `QGuiApplication`,
   and `ci/vendor-check.sh` proves the copy is exactly that.

   Also unproven: a run under `sailjail` on a device, exercising every
   permission-dependent path. Store assets and a privacy policy are not
   started.
2. **A background service, and suspend handling.** Messages arrive only
   while the app is running, which is the one thing standing between this
   and a client someone could rely on. Notifications inherit the same limit.
   Minimised to the cover is covered -- nothing tears the event loop down,
   and `Notifier` is built around the app being behind something else --
   but closed or rebooted is not, and cannot be for a Harbour package:
   a systemd user unit and a D-Bus activation file both live outside the
   four paths Harbour allows. `harbour-whisperfish` ships its own service
   under `%if %{without harbour}` for exactly this reason. So this is the
   same conversation with Jolla as the bundled server, not a thing to
   engineer around.

   Restarting `deltachat-rpc-server` after it dies is done: see the
   architecture note above.
3. **Camera QR scanning**, and showing one's own invite as a QR image. The
   link form of every payload already works, so this is polish.
4. **Loading a translation.** The catalog is real and `ci/packaging-lint.sh`
   fails if it drifts from the `qsTr()` calls, but `QTranslator` is not
   bound by qmetaobject 0.2.10. That means a `cpp!` block, an `unsafe`
   exception to a workspace lint, and C++ build machinery in a second crate,
   in a build environment `BUILDING.md` already documents as fragile. Worth
   deciding deliberately rather than in passing.
5. **Group member management, contact profile pages, blocking** outside a
   request; add-as-second-device and restore-from-backup.
6. **Message polish**: avatars on bubbles, reactions, drafts, and an unread
   divider.
7. **Recording a voice message, and the camera.** Sending every kind of
   attachment works; making one does not. QML has no audio recorder on
   Qt 5.6 -- `harbour-whisperfish` wrote its own against gstreamer -- so a
   voice note needs native code, an `unsafe` exception and the `Microphone`
   permission. The camera is reachable from QML, but wants the `Camera`
   permission, which is better added once with QR scanning than twice.
8. **Running a webxdc app.** Sending one already works and the conversation
   names it honestly; running it needs `Sailfish.WebView`, the `WebView`
   permission and the webxdc bridge.

Also open: no `sfdk` or OBS build specifically, since CI drives `mb2` 
directly; icons are placeholders.