# Gap analysis

What stands between Postivene and a usable Delta Chat client. Companion to
`MILESTONES.md`, which tracks what is built; this tracks what is missing.

Written against `5308322`, updated as items close.

## Cannot start a conversation

The shim wires 13 of the core's ~100 JSON-RPC methods. Missing:
`get_contacts`, `create_contact`, `create_chat_by_contact_id`,
`create_group_chat`, `get_chat_contacts`, `add_contact_to_chat`. No UI
creates a chat, so only conversations that arrive on their own are
reachable. This is the largest gap.

## The conversation view

One `Label` per message (`ConversationPage.qml`). No sender name, so group
chats are unreadable. No timestamps, though the model carries them. No day
markers, avatars, bubbles, images, files, voice, quotes, reactions, message
actions, drafts, unread divider, or paging.

## The chat list

`unread_count` is populated and never rendered. No last-message time,
avatars, pinned/archived/muted, search, context menu, or account switcher
(the `account_list` model has no UI).

## Defects in what exists

- `open_chat` does 1 + N round trips over the whole history, and re-runs it
  on every incoming or delivery event. Upstream has a batch `get_messages`.
- Every refresh is `reset_data`: scroll position lost, whole list redrawn.
- One shared `message_list` and `chat_list` for the app. A second
  conversation resets the first page's model underneath it, and `send_text`
  appends to whichever list is loaded.
- `send_error`, `chat_list_error`, `message_list_error`, `io_started` and
  `qr_error` have no QML listeners. A failed send vanishes silently.
- A dead `deltachat-rpc-server` leaves `status` at `"ready"`; nothing
  restarts it.
- Only `marknoticed_chat`, never `markseen_msgs`: read receipts never go
  out and messages are never marked seen on IMAP or other devices.

## Platform integration

No notifications, background service, or suspend handling, so messages
arrive only while the app is open and awake. The cover is a static label.
No sailjail permissions. `qsTr()` throughout with no `.ts` catalogs.
Placeholder icons.

## Onboarding: remaining paths

Camera QR scanning, add-as-second-device, and restore-from-backup. The link
form of every invite payload already works, so the camera is polish.

## Order of work

1. Restructure the shim: per-chat models created on demand, incremental
   updates from event payloads instead of blanket re-fetches. This causes
   the shared-model bugs and the refetch storms; doing it first is cheaper
   than retrofitting.
2. Contacts, new chat, groups; batch message fetch; error surfacing;
   `markseen_msgs`.
3. Conversation UX: sender names, timestamps, day markers, attachments,
   quotes.
4. Chat list: unread badges, times, context actions, account switcher.
5. Platform: notifications, background and suspend, cover actions,
   sailjail, translations.
