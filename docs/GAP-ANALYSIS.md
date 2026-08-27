# Gap analysis: what stands between Postivene and a usable Delta Chat client

Written 2026-08-27, against `5308322`. A companion to `MILESTONES.md`:
that file tracks what has been *built*; this one tracks what is *missing*
and why it matters. `MILESTONES.md` is organized by the scope's milestone
list and is optimistic by construction (it records completed work);
this document is the honest read of the app as a user meets it.

## Where things stand

`claude/delta-chat-orientation-0q0nj4` == `origin/main` == `5308322`.
PRs #1 and #2 are closed but their commits are in main, so nothing is in
flight.

The base is healthy: `cargo test -p deltachat-jsonrpc` builds and passes
7 tests. Qt5 dev packages are not installed in a fresh container, so the
shim/app and the QML tests need `qtbase5-dev`/`qtdeclarative5-dev` first;
`vendor/deltachat-rpc-server/` is empty (gitignored, populated by
`scripts/fetch-rpc-server.sh`).

The architecture is sound and the risky parts are genuinely done:
transport, off-thread tokio runtime, queued callbacks onto the Qt thread,
an aarch64 RPM that builds against real Qt 5.6.3, ARM binaries verified
under QEMU. What is thin is everything above that line.

## Why it is a far cry from a functioning app

### It dead-ends after login

The shim wires 13 of the core's ~100 JSON-RPC methods
(`rust/postivene-shim/src/core.rs`). Missing: `get_contacts`,
`create_contact`, `create_chat_by_contact_id`, `create_group_chat`,
`get_chat_contacts`, `add_contact_to_chat`. No UI entry point creates a
chat. Even with a working account you can only look at conversations that
arrive on their own -- you cannot start one. That alone makes it
non-functional as a messenger.

### Onboarding covers one case out of four

`qml/pages/SetupPage.qml` is address + password -> `configure`. No
chatmail account creation on the default server, no invite link /
`DCACCOUNT:` QR (`check_qr` exists at `core.rs:706` but nothing in QML
calls it and there is no scanner), no manual IMAP/SMTP server settings,
no OAuth. `ConfigureProgress` events are never displayed and there is no
cancel, so configure is an indefinite BusyIndicator.

Worse than incomplete, it is the wrong shape: a new Delta Chat user never
types an address or a password, and the `set_config` + `configure` call
sequence it uses was deprecated upstream in 2025-02 in favour of
`add_transport_from_qr` / `add_or_update_transport`. See `ONBOARDING.md`
for the real user journey, read out of the official Android client, and
for the concrete replacement.

### The conversation view is a debug view

One `Label` per message (`qml/pages/ConversationPage.qml:57`). No sender
name -- group chats are unreadable. No timestamps (the model carries
`timestamp`; QML never reads it). No day markers, avatars, bubbles,
images, files, voice messages, quotes/replies, reactions, message actions
(delete/forward/copy/resend/info), drafts, unread divider, or paging.

### The chat list drops data it already has

`unread_count` is populated in the model and never rendered -- no badge
anywhere. No last-message time, no avatars, no pinned/archived/muted, no
search, no context menu, no account switcher (the `account_list` model
exists with no UI behind it).

### Real defects in the path that does exist

- `open_chat` (`core.rs:517`) does 1 + N round trips
  (`get_message_list_items`, then `get_message` per message) over the
  entire chat history, unbounded, and re-runs the whole thing on every
  incoming/delivery event (`ConversationPage.qml:37`). A 500-message chat
  is ~500 stdio round trips per received message. Upstream has a batch
  `get_messages`.
- Every refresh is `reset_data`, so the model is fully replaced: scroll
  position lost, whole list re-rendered.
- There is **one** shared `message_list` and `chat_list` for the whole
  app. Push a second conversation and the first page's model is reset
  underneath it; navigating back shows the wrong chat's messages.
  `send_text` appends to whatever `message_list` currently holds,
  regardless of which chat the send belonged to.
- Errors are emitted and dropped: nothing in QML listens to `send_error`,
  `chat_list_error`, `message_list_error`, `io_started`, or `qr_error`. A
  failed send just makes the message disappear.
- If `deltachat-rpc-server` dies, the reader task ends and calls fail with
  `TransportClosed`, but `status` stays `"ready"` and nothing restarts it.
- Only `marknoticed_chat` is called, never `markseen_msgs` -- read
  receipts (MDNs) are never sent and messages are never marked seen on
  IMAP or on other devices.

### No platform integration at all

No notifications (nemo-qml-plugin-notifications), no background service,
no suspend handling -- Sailfish suspends apps, so today you receive
messages only while the app is open and awake. The cover is a static
label with no counts or actions. No sailjail permissions in the desktop
entry or spec. `qsTr()` everywhere with no `.ts` catalogs. Icons are
placeholder glyphs. And there is no `.github/` -- zero CI, so nothing
guards the QML naming/syntax tests that were written specifically to
catch device-only breakage.

## What to do next

One structural thing first, because it decides how everything else gets
written: the shim's "flat set of fire-and-forget methods + one global
model" shape is what causes the shared-model bugs and the full-refetch
storms. Move to per-chat model instances created on demand, with
incremental updates driven by event payloads instead of blanket
re-fetches. Doing that before piling features on is much cheaper than
retrofitting.

Then, in order of what unblocks actual use:

1. **Onboarding that matches the product** -- chatmail account creation
   and invite/QR links. See `ONBOARDING.md`; this outranks everything
   else because today a new user cannot get an account at all.
2. **Contacts + new chat + groups**, batch message fetch, error surfacing
   in the UI, `markseen_msgs`. The difference between "not a messenger"
   and "a messenger".
3. **Conversation UX** -- sender names, timestamps, day markers,
   attachments, quotes.
4. **Chat list** -- unread badges, times, context actions, account
   switcher.
5. **Platform** -- notifications, background/suspend, cover actions,
   sailjail, translations, CI.
