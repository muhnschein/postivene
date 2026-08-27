# Gap analysis

What stands between Postivene and a usable Delta Chat client. Companion to
`MILESTONES.md`, which tracks what is built; this tracks what is missing.

Written against `5308322`, updated as items close.

## Starting a conversation  *(done)*

`ContactList` lists contacts and opens a chat three ways: tapping a known
contact, adding an address, or creating a group. `NewChatPage`,
`NewContactPage` and `NewGroupPage` sit behind "New Chat" on the chat list.

What is still missing here: group member management after creation
(`get_chat_contacts`, removing members, renaming), contact profile pages,
and blocking.

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

The shared models are gone. `ChatMessages` and `ChatList` are
QML-instantiable, so a page owns its model; both load in one batch call and
apply events rather than rebuilding.

What is left here:

- `io_started` and `qr_error` have no QML listeners. A failure vanishes
  silently.
- Nothing notices when `deltachat-rpc-server` dies: `status` stays
  `"ready"` and nothing restarts it.
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

1. Error surfacing in the UI, and `markseen_msgs` -- read receipts never go
   out today.
2. Conversation UX: sender names, timestamps, day markers, attachments,
   quotes.
3. Chat list: unread badges, times, context actions, account switcher.
4. Platform: notifications, background and suspend, cover actions,
   sailjail, translations.
