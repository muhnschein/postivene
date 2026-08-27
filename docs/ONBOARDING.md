# How Delta Chat onboards a user

Read out of `deltachat/deltachat-android` @ `080e735`, plus the OpenRPC
surface of the pinned `deltachat-rpc-server` v2.53.0.

## The main path

A new user sees no address and no password. They tap **Create New Profile**,
type a display name, and tap **Agree & Create Profile**. A chatmail server
mints the account; the address and credentials are generated.

One call does it (`InstantOnboardingActivity.java:594-600`, after
`set_config` of `displayname`):

```
add_transport_from_qr(account_id, "dcaccount:nine.testrun.org")
```

`ConfigureProgress` events (0..1000) drive the progress dialog; **Cancel**
calls `stop_ongoing_process`.

## The screens

**Welcome** (`WelcomeActivity`) — "Secure Decentralized Chat", two buttons:
*Create New Profile*, and *I Already Have a Profile* offering **Add as
Second Device** (scan a QR from an existing install) and **Restore from
Backup**. No credential field anywhere.

**Create profile** (`InstantOnboardingActivity`) — avatar, display name, one
button, a privacy-policy link for the chosen host (default
`nine.testrun.org`). An overflow menu offers *List Chatmail Servers*
(`https://chatmail.at/relays`), *Log in to your email account*
(`EditRelayActivity`), and *Scan QR code*.

**Landing** — the chat list, seeded with a "Device Messages" chat whose
welcome text tells the user to exchange QR codes or share an invite link.

## The other ways in

| Payload | `check_qr` kind | Effect |
|---|---|---|
| `dcaccount:<host>` | `account` | Same flow, different server |
| `dclogin:...` | `login` | Button becomes *Log in*; credentials from the link |
| `https://i.delta.chat/...` | `askVerifyContact` | Secure-join runs after the profile is created |
| Group invite | `askVerifyGroup` | Same, for a group |
| Backup transfer QR | — | Streams the profile from another install |

`dcaccount:` and `dclogin:` URIs open onboarding directly.

## Accounts have transports

`configure` is deprecated upstream as of 2025-02 (`Rpc.java:233-241`). An
account holds one or more **transports** — email relays managed with
`add_transport` / `add_or_update_transport` / `list_transports` /
`delete_transport`, each an `EnteredLoginParam` whose only required fields
are `addr` and `password`.

| Deprecated | Current |
|---|---|
| `set_config(addr)` + `set_config(mail_pw)` + `configure` + `start_io` | `add_or_update_transport(account_id, {addr, password})` |
| — | `add_transport_from_qr(account_id, "dcaccount:<host>")` |
| — | `list_transports` for a settings screen |

`add_transport_from_qr` stops and restarts IO around the change;
`set_config_from_qr` does not.

## Starting a conversation is invite-first

- **QR Code** in the main menu: show your own invite
  (`get_chat_securejoin_qr_code(account_id, null)`) or scan one, which runs
  `secure_join`.
- **Invite Friends**: shares that invite as an `https://i.delta.chat/...`
  URL.
- **New Chat**: a contact list led by actions — *New Group*, *New
  Unencrypted Group*, *New Broadcast*, *Scan QR Invite* — then contacts.
  Picking one calls `create_chat_by_contact_id`.
- **New Contact** (typing an address) produces an *unencrypted* chat. It is
  the fallback for mailing someone who does not use Delta Chat.

## Verified against the pinned binary

v2.53.0 advertises 177 methods, including everything above. No version bump
needed:

```sh
scripts/fetch-rpc-server.sh
vendor/deltachat-rpc-server/x86_64/deltachat-rpc-server --openrpc \
  | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["methods"]))'
```
