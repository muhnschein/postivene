# Device checks

What only a phone can answer. `make check` covers everything else.

## Confirmed

- Sending: a message lands once, gets its delivery tick.
- Chat list pulley: "New chat".
- Conversation view: bubbles, times, delivery marks, day separators,
  image attachment sizing.
- Read receipts reach the other client.
- Killing `deltachat-rpc-server` puts the banner on screen, and it stays.

## Pending

- **Onboarding pulley menu** (`CreateProfilePage`): email login and invite
  link. Only reachable before an account exists, so it needs a fresh
  install or a second profile.
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
- Long histories: the list has no paging yet.
- Everything in `GAP-ANALYSIS.md` under Platform integration.
