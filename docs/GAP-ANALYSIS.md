# Gap analysis

What stands between Postivene and a usable Delta Chat client. Companion to
`MILESTONES.md`, which tracks what is built; this tracks what is missing.

Written against `5308322`, updated as items close. What only a phone can
answer is in `DEVICE-CHECKS.md`.

## Starting a conversation  *(done)*

`ContactList` lists contacts and opens a chat three ways: tapping a known
contact, adding an address, or creating a group. `NewChatPage`,
`NewContactPage` and `NewGroupPage` sit behind "New Chat" on the chat list.

Invites work in both directions: a pasted `https://i.delta.chat/...` link
is classified by the core and followed with `secure_join`, and the
account's own invite link can be copied out. This is the normal way a Delta
Chat contact is added -- an address alone cannot be encrypted to.

What is still missing here: reading an invite off the camera (a scanner is
platform work, below), showing one's own invite as a QR image, group member
management after creation, contact profile pages, and blocking.

## The conversation view

Messages are bubbles (`components/MessageDelegate.qml`, loaded and measured
on its own by `tests/qml_conversation.rs`): sender name and colour in
groups, time and delivery mark, quoted message, image previews, named
attachments that open in the system's handler, and core notices set apart.
Day separators come from a section over the local day, which the model
counts from an offset QML hands it.

The view opens on the newest message and follows arrivals, but only while
the reader is at the bottom.

What is still missing here:

- **A message context menu**, the way the chat list has one: reply, quote,
  forward, copy, delete, and resend for a failed message.
- **Saying that a message arrived while the reader is scrolled up.** Not
  being yanked down is right; not knowing there is something new is not.
  A "jump to newest" button over the bottom of the list is the usual
  answer, and it is worth having whether or not notifications land first.
- Avatars, voice messages and audio playback, reactions, drafts, an unread
  divider, paging for long histories, and sending attachments.

## The chat list

Rows carry an unread badge, the last message's time (a clock today, a
weekday this week, a date beyond that), who wrote it or how far ours got,
an avatar, and marks for unencrypted, pinned and muted chats
(`components/ChatListDelegate.qml`). The context menu marks read, pins,
mutes, archives and deletes.

What is still missing here: an account switcher (the `account_list` model
has no UI), search, a way back to archived chats, and contact requests --
they show as ordinary chats rather than asking to be accepted.

## Defects in what exists

The shared models are gone. `ChatMessages` and `ChatList` are
QML-instantiable, so a page owns its model; both load in one batch call and
apply events rather than rebuilding.

Failures reach the user: every page shows them in a shared `ErrorBanner`,
the core's own `Error` events arrive as a typed `core_error` signal, and the
server dying flips `status` to `"stopped"` rather than leaving it claiming
`"ready"`. Opening a chat calls `markseen_msgs`, so read receipts go out.

What is left here:

- Nothing restarts `deltachat-rpc-server` after it dies; the app says so
  and asks for a restart.

## Platform integration

No notifications, background service, or suspend handling, so messages
arrive only while the app is open and awake. The cover is a static label.
No sailjail permissions. `qsTr()` throughout with no `.ts` catalogs.
Placeholder icons.

## Onboarding: remaining paths

Camera QR scanning, add-as-second-device, and restore-from-backup. The link
form of every invite payload already works, so the camera is polish.

## Order of work

1. Platform: notifications, background and suspend, cover actions,
   sailjail, translations, and camera QR scanning. Messages arrive only
   while the app is open and awake, which is what stops this being usable
   as one's actual client.
2. Message actions: a context menu, and a way to know a message arrived
   while reading further up.
3. Accounts: the switcher, and the rest of the chat list -- search,
   archived chats, contact requests.
