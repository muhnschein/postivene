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
- Group chats: sender names and colours, and the invite flow for one.
- Quoted messages, and attachments other than images.
- Orientation changes, and the cover.
- Long histories: the list has no paging yet.
- Everything in `GAP-ANALYSIS.md` under Platform integration.
