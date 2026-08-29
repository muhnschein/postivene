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
the reader is at the bottom. When they are not, a button says how many
have arrived and takes them there. A row's context menu replies (the send
carries the quote), copies, deletes, and offers a failed message again.

What is still missing here: forwarding, avatars, voice messages and audio
playback, reactions, drafts, an unread divider, paging for long histories,
and sending attachments.

## The chat list

Rows carry an unread badge, the last message's time (a clock today, a
weekday this week, a date beyond that), who wrote it or how far ours got,
an avatar, and marks for unencrypted, pinned and muted chats
(`components/ChatListDelegate.qml`). The context menu marks read, pins,
mutes, archives and deletes.

What is still missing here:

- **A real avatar renders square.** The disc behind it is round and the
  generated initial sits on it correctly, but an `Image` does not take its
  parent's corner radius, so a chat with a picture reads as a rectangle
  among circles. Rounding it wants `OpacityMask` from QtGraphicalEffects,
  or a shader -- `clip` only cuts to the bounding box.
- An account switcher (the `account_list` model has no UI), search, a way
  back to archived chats, and contact requests -- they show as ordinary
  chats rather than asking to be accepted.

## Starting a conversation: the pages behind "New chat"

`NewChatPage` and `NewGroupPage` predate the chat list's rebuild and look
it: contacts are a name and an address on a fixed-height row, with the
page's actions as buttons stacked in the header.

What is still missing here:

- Both pages should carry the chat list's row: a round avatar in the
  contact's own colour, and its spacing.
- Their actions belong in a pulley menu, as on the chat list. No context
  menu on the rows -- picking a contact is the only thing to do with one.
- "New Contact" should open the invite page rather than the
  address-and-name form. Adding an address alone produces a chat that
  cannot be encrypted; an invite is how a contact is actually added. The
  entry keeps the name, since that is what the reader is trying to do.

## Defects in what exists

The shared models are gone. `ChatMessages` and `ChatList` are
QML-instantiable, so a page owns its model; both load in one batch call and
apply events rather than rebuilding.

Failures reach the user: every page shows them in a shared `Banner`,
the core's own `Error` events arrive as a typed `core_error` signal, and the
server dying flips `status` to `"stopped"` rather than leaving it claiming
`"ready"`. Opening a chat calls `markseen_msgs`, so read receipts go out.

What is left here:

- Nothing restarts `deltachat-rpc-server` after it dies; the app says so
  and asks for a restart.

## Platform integration

A message landing in a chat the reader is not looking at raises a
notification (`Nemo.Notifications`), and walking into that chat takes
it down; a muted chat is never announced. There is still no background
service or suspend handling, so none of that happens unless the app is
running -- messages arrive only while it is open and awake, which
remains the thing that stops this being usable as one's actual client.
The cover is a static label.
The app declares a sailjail sandbox (`Internet`, and its own data
directory) in `postivene.desktop`; `Camera` waits for QR scanning.
`qsTr()` throughout with no `.ts` catalogs.
Placeholder icons.

## Onboarding: remaining paths

Camera QR scanning, add-as-second-device, and restore-from-backup. The link
form of every invite payload already works, so the camera is polish.

## Order of work

1. Platform: notifications, background and suspend, cover actions,
   sailjail, translations, and camera QR scanning. Messages arrive only
   while the app is open and awake, which is what stops this being usable
   as one's actual client.
2. The pages behind "New chat", brought up to the chat list's own look,
   and the round avatar everywhere.
3. Accounts: the switcher, and the rest of the chat list -- search,
   archived chats, contact requests.
4. Forwarding, which needs a chat picker the app does not have yet.
