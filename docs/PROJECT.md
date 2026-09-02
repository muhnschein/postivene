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
- **A chat holds a row for every message in it.** The ids are cheap and the
  messages are not: `get_message_list_items` returns a list of numbers for
  the whole chat, while `get_messages` builds every field of every row. So
  the model builds one row per id the moment that list arrives, and fills in
  only the ones somebody is looking at. `hydrate(first, last)` is the only
  thing that fetches messages after a chat opens; the view asks as it
  scrolls.
- **Which is what makes the first message row 0, and keeps it there.** The
  model used to hold a moving *window* of loaded messages, and every way of
  getting somewhere -- the beginning of the chat, a search result -- meant
  replacing its contents. That is the shape the bug was in, and it took
  three attempts at the symptoms to be sure: positioning into a model that
  has just been reset is contingent on how fast rows are measured, a
  reconciliation that overlapped a move undid it, and the control that
  offered the move hid itself at the moment it was wanted. Each was a real
  defect. None of them was the cause. Whisperfish and deltachat-android both
  keep the whole conversation addressable and have none of this; going to
  the beginning of a chat is now a scroll, with nothing to fetch, nothing to
  replace, and nothing that can put the reader somewhere else.
- **Rows are filled in where they stand.** `change_line` rather than a
  reset, so the view keeps its position and its delegates. An arrival is one
  row appended; a deletion is one removed. Nothing else moves.
- **The day heading is drawn inside the row it heads.** `section.property`
  is set, so the view still groups by day and fills in `ListView.section`
  and `ListView.previousSection` on each delegate -- but there is no
  `section.delegate`. A section delegate is its own item, positioned above
  its row and sized from whatever height the view last measured, and a date
  drawn on top of the message beneath it was reported twice. Getting the
  height right at creation was not enough, because where the row goes is the
  view's bookkeeping rather than ours. Inside the row it is arithmetic: the
  heading is part of `contentHeight`, and a row cannot be drawn over itself.
- **A row knows its day before it knows its message.**
  `get_message_list_items` interleaves the core's own day markers when asked
  for them, so the day comes with the id rather than with the message. Not a
  nicety: the list is sectioned by day, so a row that does not know its day
  sits under day 0, and a screenful of them is a heading reading 1 January
  1970. Worse, each row moves to its real day as it is filled in, which
  resizes a heading the view has already laid out and leaves it drawn over
  the row beneath. The markers are local midnight, checked against the real
  `deltachat-rpc-server` in three zones -- a marker at *UTC* midnight would
  file every message in a zone behind UTC under yesterday -- and go through
  the same `local_day_number` as a fetched row, so a row's day cannot change
  when its message arrives.
- **Following the newest message stops the moment something else moves the
  view.** A chat opens at its newest message and stays there as messages
  arrive -- which means every change to the content height sends the view
  back to the end. The system's own scroll-to-top changes `contentY` without
  a drag, so nothing would otherwise tell the list that the reader has gone
  somewhere, and the first row measured after the jump would haul them back
  down. `ConversationList` reads a jump it did not make as the reader
  leaving, and only once the view has actually reached the end at least
  once: before that, a view far from the end is a chat that has not finished
  opening.
- **The chat is on the page before the page arrives.** `ChatPrefetch` loads
  it while the reader is still looking at the chat list, and
  `ConversationPage` takes it in `Component.onCompleted` -- after
  `reading_history` is bound, or the model reads the default and marks the
  chat read behind a page nobody has seen. A prefetch hit is a move, not a
  fetch. When a search result is what was tapped, the prefetch fills in the
  rows around *that message* rather than the newest ones, so the page never
  shows today and then jumps.
- **A row jumped to has to be held, not jumped to.** One
  `positionViewAtIndex` is enough only where every row is already its final
  height, which is nowhere real: rows are measured as they are laid out,
  wrapped text at the device's own metrics is taller than an estimate, a
  picture's row changes height again when the picture decodes, and a row
  gains its text the moment it is filled in. Each of those that happens
  above the reader moves them, and they all happen after the jump. So
  `holdAt` re-applies the row on every change to the content, until the
  reader takes the view over or it stops moving under them. Landing on a
  search result and back on the place a full-screen picture was opened from
  are both this.
- **A page pushed over the conversation takes its place with it.** The list
  is torn down far enough to forget where it was, and replaces what it
  forgets with the beginning. The row is *held* from `Deactivating`, not
  merely written down and restored on `Active`: restoring is too late,
  because the reset happens while the page is away and a frame showing the
  top of the chat is painted before anything corrects it. That was the
  flash of the oldest messages, and being yanked back from it. Held, the
  reset is undone in the same turn it happens and no wrong frame is drawn.
  A hold is let go of when the reader touches the list, and otherwise on a
  deadline -- but never on a quiet timer: a device lays its rows out,
  goes quiet while a picture decodes, and moves them again, and the gap is
  longer than any timer worth having.
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
- **Drafts belong to the core, not the page.** `misc_set_draft` and
  `get_draft` keep what was typed and not sent, so it survives the app
  being closed rather than only the trip back to the chat list. It also
  means the chat list needs no new field: a chat holding one comes back
  with `summaryText1` "Draft", which the row already shows in front of the
  preview, so it reads "Draft: ..." in whatever language the core is in.
  Written on a debounce while typing and again the moment the page goes,
  because leaving is exactly when the debounce has not fired yet.
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
- **What a chat is sits to the right of it.** Swiping left from a
  conversation reaches the group behind it -- picture, name, members --
  or, from a one-to-one chat, the contact: picture, name, the line they
  wrote about themselves, and whether the connection is checked and
  encrypted. Attached rather than pushed, so the page indicator says it is
  there; the header tap goes the same way. One `ChatInfo` serves both: it
  reads `get_full_chat_by_id`, which names the members by id, so the
  contacts come from `get_contacts_by_ids`, keyed by id as strings, and a
  one-to-one chat simply has one. Every group edit is one core call
  followed by a reload, since the core is what knows who is in the group
  now and where it put the picture. What can be changed is the core's
  answer too: `selfInGroup` goes false on leaving and every edit is then
  refused, so the controls are not offered.
- **No email addresses.** A reader of a chatmail app has no use for one,
  so a contact is its name, its picture and its status line everywhere a
  contact is shown. The one place an address is drawn is the profiles
  page, where it is the reader's own and tells two accounts apart.
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
messages, archive, contact requests, multiple profiles), groups (created,
and then renamed, given a picture, added to, removed from and left), a
contact page beside each one-to-one chat, the conversation view (bubbles, quotes, delivery marks, day separators, reply/copy/delete/
resend, a page of history at a time, and every kind of attachment the core
classifies: photos and
stickers inline, GIFs animated over a still poster, a video's poster frame
from the platform thumbnailer, voice and audio played where they sit, a
shared contact as a card, everything else named and sized), onboarding
rebuilt on the core's current transport API, `secure_join` invites in both
directions, encryption indicators, foreground notifications, and the
cover.

Packaging is real: `mb2` builds produce `harbour-postivene-0.1.0-<release>.aarch64.rpm`,
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
5. **Blocking** outside a request; a media grid on the group and contact
   pages; add-as-second-device and restore-from-backup.
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