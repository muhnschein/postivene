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

The chat list searches (the core matches, so a search finds chats the
model has never loaded), reaches the archived list as its own page, and
offers a contact request the only two answers it has -- accept or block --
rather than showing it as an ordinary chat. `AccountsPage` puts the
`account_list` model on screen, and appears in the pulley only where there
is more than one account to choose between.

What is still missing here: group member management after creation,
contact profile pages, and blocking a contact outside a request.

## Starting a conversation: the pages behind "New chat"  *(done)*

`NewChatPage` and `NewGroupPage` carry the chat list's row --
`components/ContactRow.qml` over the same `components/Avatar.qml` the chat
list uses, so the two cannot drift -- and their actions sit in a pulley
menu. Rows have no context menu: picking a contact is the only thing to do
with one.

"New Contact" opens the invite page, since an address alone produces a
chat that cannot be encrypted and an invite is how a contact is actually
added. The address form lives on behind that page: the core can also mail
someone who does not use Delta Chat at all, and that needs an address.

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
The cover shows the unread total across every chat and the core's
state, with a cover action back to the chat list.
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
