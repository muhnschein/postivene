# How Delta Chat actually onboards a user

Postivene's `SetupPage.qml` asks for an email address and a password. That
is not how anyone gets into Delta Chat in 2026, and it is not how the
reference client presents itself. This document records the real user
journey, read out of the official Android client, so the Sailfish UI can
be rebuilt against it rather than against an assumption.

Reference: `deltachat/deltachat-android` at `080e735` (read-only clone),
plus the OpenRPC surface of the `deltachat-rpc-server` **v2.53.0** binary
this repo already pins (`scripts/fetch-rpc-server.sh`).

## The short version

A new user never sees an email address or a password. They tap **Create
New Profile**, type a display name, optionally pick an avatar, and tap
**Agree & Create Profile**. The app asks a *chatmail server* to mint an
account, and the address and credentials are generated for them. There is
no "sign up" form, no server picker in the main path, no confirmation
mail.

The client-side implementation of that is a single JSON-RPC call:

```
add_transport_from_qr(account_id, "dcaccount:nine.testrun.org")
```

(`InstantOnboardingActivity.java:594-600`, after `set_config` of
`displayname` and the self avatar.) Everything else on that screen is
profile decoration.

## The first-run flow, screen by screen

### 1. Welcome (`WelcomeActivity`, `welcome_activity.xml`)

Title: *"Secure Decentralized Chat"*. Two buttons, nothing else:

- **Create New Profile** (primary) -> instant onboarding.
- **I Already Have a Profile** (secondary) -> a dialog with exactly two
  options: **Add as Second Device** (scan a QR shown by an existing
  install; transfers the profile over the local network) and **Restore
  from Backup** (pick a backup file).

Note what is *absent*: any field for an email address, and any mention of
IMAP, SMTP, or a password.

### 2. Create profile (`InstantOnboardingActivity`)

Fields: avatar (optional), display name (required). One button, *"Agree &
Create Profile"*, above a link to the chosen server's privacy policy.
The server defaults to `nine.testrun.org`
(`DEFAULT_CHATMAIL_HOST`), expressed internally as the QR payload
`dcaccount:nine.testrun.org`.

An overflow dialog ("Use Other Server") offers three escapes:

- **List Chatmail Servers** -> opens `https://chatmail.at/relays` in a
  browser, from which the user comes back with a `dcaccount:` link.
- **Log in to your email account** -> `EditRelayActivity`, the classic
  IMAP/SMTP form. This is where Postivene's *entire* current setup screen
  lives in the reference client: two levels deep, behind "other options",
  as the escape hatch for people who insist on their own mailbox.
- **Scan QR code** -> camera scanner for `dcaccount:` / `dclogin:` /
  invite codes.

Progress: the core emits `ConfigureProgress` events (0..1000); the dialog
shows `progress / 10` as a percentage and offers **Cancel**, which calls
`stop_ongoing_process`.

### 3. Landing

On success the app goes straight to the chat list and seeds the "Device
Messages" chat. That chat's welcome text is the product's own statement
of what a new user should do next:

> Get in contact!
> 🙌 Tap "QR code" on the main screen of both devices. Choose "Scan QR
> Code" on one device, and point it at the other.
> 🌍 If not in the same room, scan via video call or share an invite link
> from "Scan QR code".

Not "type your friend's email address".

## The other ways in

All of them are link- or QR-shaped, and all of them land in the same
instant-onboarding screen with a different provider or intent:

| Payload | `check_qr` kind | What the screen becomes |
|---|---|---|
| `dcaccount:<host>` / `DCACCOUNT:` | `account` | Same flow, different chatmail server |
| `dclogin:...` | `login` | Button becomes *"Log in"*; credentials come from the link |
| Contact invite (`https://i.delta.chat/...`) | `askVerifyContact` | Shows "you will be connected with <name>"; after the profile is created, secure-join runs automatically |
| Group invite | `askVerifyGroup` | Same, for a group |
| Backup transfer QR | -- | "Add as Second Device": streams the profile from another install |

`InstantOnboardingActivity` also registers for `dcaccount:` and
`dclogin:` URI intents, so tapping such a link anywhere on the device
opens onboarding directly (`handleIntent()`).

## The data model has moved on: accounts have *transports*

Postivene models an account as "one address + one password, set via
`set_config` then `configure`". The core's own API deprecated that:

> **`configure`** -- *"Deprecated as of 2025-02; use
> `add_transport_from_qr()` or `add_or_update_transport()` instead."*
> (`Rpc.java:233-241`, generated from the core's OpenRPC spec.)

The current model is:

- An **account** (a "profile") holds identity, contacts, chats, keys.
- It has one or more **transports** -- email relays -- managed with
  `add_transport` / `add_or_update_transport` / `list_transports` /
  `delete_transport`, each described by an `EnteredLoginParam`
  (`addr`, `password`, and optional `imapServer`/`imapPort`/
  `imapSecurity`/`imapUser`/`smtpServer`/... which autoconfigure when
  left null).
- `add_transport_from_qr` is the same thing driven from a QR payload,
  and it stops and restarts IO around the change, which the deprecated
  `set_config_from_qr` does not.

So the correct Postivene equivalents are:

| Today (deprecated) | Should be |
|---|---|
| `set_config(addr)` + `set_config(mail_pw)` + `configure` + `start_io` | `add_or_update_transport(account_id, {addr, password})` |
| -- | `add_transport_from_qr(account_id, "dcaccount:nine.testrun.org")` for instant onboarding |
| -- | `list_transports` for a settings screen |

## Starting a conversation is also QR-first

The second thing the current UI misreads. In the reference client, the
chat list's entry points are:

- **QR Code** (main menu): a two-tab screen -- *Show* your own invite QR
  (`get_chat_securejoin_qr_code(account_id, null)`) and *Scan* someone
  else's. Scanning runs `secure_join`.
- **Invite Friends**: shares the same invite as a URL --
  *"Contact me on Delta Chat:\n<https://i.delta.chat/...>"*.
- **New Chat** (FAB / menu): a contact list whose first rows are actions,
  not contacts -- *New Group*, *New Unencrypted Group*, *New Broadcast*,
  *Scan QR Invite* -- followed by known contacts. Picking one calls
  `create_chat_by_contact_id`.
- **New Contact** (typing an email address) exists, but it produces an
  *unencrypted* "address contact" chat. It is the fallback for emailing
  someone who does not use Delta Chat, not the way you add a friend.

## What Postivene should build

1. **Replace `SetupPage.qml` with a welcome page**: "Create New Profile"
   / "I Already Have a Profile". No credential fields on the first
   screen.
2. **A profile page**: display name (+ avatar later), one "Agree & Create
   Profile" button, privacy-policy link for the selected host, and an
   "Other options" menu (paste/scan an invite, other server, classic
   email login).
3. **Shim methods**: `add_transport_from_qr`, `add_or_update_transport`,
   `list_transports`, `stop_ongoing_process`, plus surfacing
   `ConfigureProgress` (percent + cancel) instead of an indefinite
   BusyIndicator.
4. **An invite path that does not need a camera.** Sailfish QR scanning
   needs a scanner component and camera permission; the *link* form of
   every one of these payloads is plain text. Accepting a pasted
   `dcaccount:` / `dclogin:` / `https://i.delta.chat/...` string covers
   the whole surface with a TextField, and should be built first --
   camera scanning is an enhancement, not a prerequisite.
5. **Register the URL schemes** (`dcaccount:`, `dclogin:`,
   `https://i.delta.chat`) in `postivene.desktop` so invite links tapped
   elsewhere on the device open the app.
6. **Keep classic email login**, but move it where upstream keeps it:
   behind "other options", implemented as `add_or_update_transport`.

## Verified against the pinned binary

Everything above is implementable with the `deltachat-rpc-server`
v2.53.0 binary this repo already pins -- no version bump needed. Its
OpenRPC spec advertises **177 methods** (Postivene currently wires 13),
including `add_transport`, `add_or_update_transport`,
`add_transport_from_qr`, `list_transports`, `check_qr`, `secure_join`,
`get_chat_securejoin_qr_code`, `create_contact`,
`create_chat_by_contact_id`, `create_group_chat`, `get_messages`,
`markseen_msgs`, `accept_chat`, `block_chat`, `background_fetch` and
`wait_next_msgs`.

Reproduce:

```sh
scripts/fetch-rpc-server.sh
vendor/deltachat-rpc-server/x86_64/deltachat-rpc-server --openrpc \
  | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["methods"]))'
```
