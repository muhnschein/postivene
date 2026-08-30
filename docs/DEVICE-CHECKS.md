# Device checks

What only a phone can answer. `make check` covers everything else.

Walked in full on a device, 2026-08-29.

## Confirmed

- Sending: a message lands once, gets its delivery tick.
- Chat list pulley: "New chat".
- Conversation view: bubbles, times, delivery marks, day separators,
  image attachment sizing.
- Read receipts reach the other client.
- Killing `deltachat-rpc-server` puts the banner on screen, and it stays.
- **Onboarding pulley menu** (`CreateProfilePage`): email login and invite
  link.
- Reopening a chat lands on the newest message, and a message arriving
  while you are reading history leaves you where you are.
- Chat list rows: badge, time, avatar, and the context menu's five
  actions. Deleting asks first.
- The message context menu: reply (the quote shows above the field and in
  the sent message), copy, delete, and Send again on a failed message.
- The jump-to-newest button: one circle, readable at half transparency,
  and its badge when messages arrive out of sight.
- Scrolling up into the history on the first try, and messages arriving
  while up there keeping their unread badge in the chat list until the
  reader comes back down.
- The reply bar wrapping a long quote to three lines, and copying a
  message saying so.
- Group chats: sender names and colours, and the invite flow for one.
- Quoted messages, and attachments other than images.
- Orientation changes, and the cover.

## Needs re-checking

Changed since the list above was walked, and only a phone can settle it:

- **A chat's unread badge clears when you open it.** The walk above
  confirmed the badge *appears*; it did not catch that it never went
  away. `reading_history` was read once, when the load returned, and the
  reader is not looking then -- the page is still transitioning in. Now
  marked when the reader actually starts looking. Check a chat with
  several unread messages, opened from the list, and the cover's total
  alongside it.
- **Grouped search.** Typing in the chat list now searches chats,
  contacts and messages at once and shows them under counted headings.
  Only a phone has an account with enough in it to say whether the
  grouping reads well, whether the fifty-message cap is the right place
  to cut, and whether a message result landing at the bottom of its chat
  rather than on the hit is tolerable.
- **The profile picture picker.** The settings page opens Silica's image
  picker, and `Permissions=Pictures` was added to the desktop file for
  it. Under confinement without that permission the picker shows an empty
  gallery and the file it returns cannot be opened -- so this is the one
  check that says whether the sandbox grant is right. Pick a picture,
  leave the page, come back: it should still be there, at a path inside
  the account's blob directory rather than the one that was picked.
- **Opening a busy chat.** The fetch now waits for the page to finish
  arriving. Open the longest conversation on the device from the chat
  list and watch the transition rather than the result.
- - **The accounts directory after sailjail.** Confinement is new, and the
  directory moved inside the grant to suit it. A profile from an earlier
  build has to be moved by hand first; see `GAP-ANALYSIS.md`.

## Open question: is the sandbox actually on

Not checked yet, and worth knowing. The `[X-Sailjail]` section was added
in the sailjail commit, and the accounts directory moved inside the grant
to survive it -- but a device that upgraded straight into it kept working
*without* the directory being moved by hand. The likely explanation is
that `adopt_legacy_accounts` ran, which it can only do when the app can
still read outside its grant. That would mean confinement is not in force.

To settle it:

```sh
# Did the profile move on its own?
ls -d ~/.local/share/postivene/accounts \
      ~/.local/share/postivene/postivene/accounts 2>&1
```

If the old path is gone and the new one exists, the migration ran -- and
the question is then why the sandbox did not prevent it. Revisit before
claiming the app is confined.

## Not yet checkable

Nothing above is outstanding. What is left here waits on features that do
not exist yet rather than on a phone:

- Long histories. The list has no paging (`GAP-ANALYSIS.md`), so there is
  nothing to walk into.
- Everything in `GAP-ANALYSIS.md` under Platform integration --
  notifications, background service, suspend handling. None of it is
  built, so none of it can be confirmed or denied on a device.
