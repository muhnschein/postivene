# Gap analysis

What stands between Postivene and a usable Delta Chat client. Companion to
`MILESTONES.md`, which tracks what is built; this tracks what is missing.

Written against `5308322`, updated as items close. What only a phone can
answer is in `DEVICE-CHECKS.md`.

## Starting a conversation  *(done)*

`ContactList` lists contacts and opens a chat three ways: tapping a known
contact or creating a group. `NewChatPage` and `NewGroupPage` sit behind
"New Chat" on the chat list.

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

What is still missing here: avatars on bubbles, voice messages and audio
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
rather than showing it as an ordinary chat. `ProfilesPage` puts the
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
added. The address form is gone entirely: it could only ever produce a
chat that cannot be encrypted, and this client does not set out to
support plaintext conversations.

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
directory) in `harbour-postivene.desktop`; `Camera` waits for QR scanning.
`qsTr()` throughout with no `.ts` catalogs.
Placeholder icons.

## Onboarding: remaining paths

Camera QR scanning, add-as-second-device, and restore-from-backup. The link
form of every invite payload already works, so the camera is polish.

## Order of work

1. **A background service, and suspend handling.** Messages arrive only
   while the app is open and awake, which is the one thing standing
   between this and a client someone could actually rely on. Notifications
   exist now but can only fire while the app is running, so they inherit
   the same limit. Restarting `deltachat-rpc-server` after it dies belongs
   with this rather than after it: a supervised service that dies and
   stays dead is worse than one that was never there, because nobody is
   watching the screen to see the banner.
2. Camera QR scanning, and showing one's own invite as a QR image.
3. **Loading a translation.** `translations/postivene.ts` now exists and
   `ci/packaging-lint.sh` fails if it drifts from the `qsTr()` calls, so
   the catalog is real and reviewable. Nothing loads it yet: that needs a
   `QTranslator`, which qmetaobject 0.2.10 does not bind, so it means a
   `cpp!` block and an `unsafe` exception to a workspace lint the tree
   otherwise holds to -- plus C++ build machinery in a second crate, in a
   build environment `SDK-BUILD.md` already documents as fragile. Worth
   deciding deliberately rather than in passing.
4. Group member management after creation, contact profile pages, and
   blocking a contact outside a request.
5. Avatars on message bubbles, voice messages, reactions, drafts, an
   unread divider, paging for long histories, and sending attachments.

## Vocabulary: profile, not account

The reference clients say "profile", and so does everything a reader
sees: the page, its title, its rows, the pulley entry that opens it.

The word stops at the qmetaobject bridge. `account_id`, `account_list`,
`accounts_refreshed` and their neighbours keep the core's own name,
because that is what the wire says -- the JSON-RPC methods are literally
`add_account`, `get_all_accounts`, `remove_account`. Renaming our side
while the protocol says otherwise would buy a nicer identifier and cost a
translation layer at every call site.

Two visible strings deliberately keep the word: "Log in to an email
account" and "no account with us" both mean an account somewhere else,
which is exactly the distinction the rename is drawing.
