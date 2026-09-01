use qmetaobject::listmodel::SimpleListModel;
use qmetaobject::QString;

/// One chat-list row from `get_chatlist_items_by_entries`. Carries only the
/// fields the UI uses; upstream's has ~20 more.
// Each flag is a role QML reads by name, so they cannot be folded into
// one field.
#[allow(clippy::struct_excessive_bools)]
#[derive(Default, Clone, PartialEq, qmetaobject::SimpleListItem)]
pub struct ChatListItem {
    /// The core's chat id.
    pub chat_id: u32,
    /// Display name of the chat.
    pub name: QString,
    /// Last-message preview (`summaryText2` upstream).
    pub preview: QString,
    /// Who wrote the last message (`summaryText1`), for a group's preview.
    pub preview_sender: QString,
    /// Fresh (unseen) message count, for the badge.
    pub unread_count: u32,
    /// When the last message landed. Unix seconds; the core sends
    /// milliseconds.
    pub last_updated: i64,
    /// `DC_STATE_*` of the last message, for a tick on one we sent.
    pub summary_state: u32,
    /// `isEncrypted` upstream. Unencrypted chats get a mail icon.
    pub is_encrypted: bool,
    /// Kept at the top of the list.
    pub is_pinned: bool,
    /// No notifications, and the badge is muted with it.
    pub is_muted: bool,
    /// A chat from someone not yet accepted.
    pub is_contact_request: bool,
    /// The core's per-chat colour, `#rrggbb`, for the avatar.
    pub color: QString,
    /// Path to the chat's picture, empty when it has none.
    pub avatar_path: QString,
}

/// Chat-list model bound to a `SilicaListView` from QML.
pub type ChatListModel = SimpleListModel<ChatListItem>;

/// One message row from `get_message_list_items` plus `get_message`.
// Each flag is a role QML reads by name, so they cannot be folded into
// one field.
#[allow(clippy::struct_excessive_bools)]
#[derive(Default, Clone, qmetaobject::SimpleListItem)]
pub struct MessageListItem {
    /// The core's message id.
    pub message_id: u32,
    /// Whether this row's content has been fetched yet.
    ///
    /// The model holds one row per message in the chat, because that is what
    /// makes the first message row 0 and keeps it there. Only the rows near
    /// the reader are filled in; the rest stand as placeholders of their own
    /// height until they come into view.
    pub loaded: bool,
    /// Message body text.
    pub text: QString,
    /// True when this account sent it.
    pub is_outgoing: bool,
    /// Unix timestamp, seconds.
    pub timestamp: i64,
    /// Days since the epoch in the viewer's timezone, for the day
    /// separators. A section groups by it, so it need not know the
    /// timezone itself.
    pub day_number: i64,
    /// `showPadlock` upstream: correctly encrypted and signed. A mail icon
    /// marks the false case.
    pub show_padlock: bool,
    /// `DC_STATE_*`: outgoing 20 pending, 24 failed, 26 delivered, 28 read;
    /// incoming 10 fresh, 13 noticed, 16 seen.
    pub state: u32,
    /// Who sent it: `overrideSenderName` if set, else the contact's
    /// display name.
    pub sender_name: QString,
    /// The core's per-contact colour, `#rrggbb`.
    pub sender_color: QString,
    /// A core-generated notice ("... joined the group"), not a message
    /// anyone typed.
    pub is_info: bool,
    /// `isForwarded` upstream: a copy of a message from somewhere else
    /// rather than one written here. The core sets it on the sender's own
    /// copy too, which is why a forward made from this device is marked.
    pub is_forwarded: bool,
    /// The quoted message's text, empty when nothing is quoted.
    pub quote_text: QString,
    /// Who wrote the quoted message.
    pub quote_author: QString,
    /// Absolute path to the attachment in the core's blob dir.
    pub file_path: QString,
    /// The attachment's name as it should be shown.
    pub file_name: QString,
    /// `viewType` upstream: Text, Image, Gif, Sticker, Audio, Voice,
    /// Video, File, Call, Webxdc, Vcard. What the conversation renders the
    /// attachment as; the core decides it from the file itself.
    pub view_type: QString,
    /// Attachment pixel size, 0 when not an image.
    pub image_width: i32,
    /// Attachment pixel size, 0 when not an image.
    pub image_height: i32,
    /// The attachment's MIME type, empty when the core has none. What a
    /// generic file row names itself by when it has nothing better.
    pub file_mime: QString,
    /// The attachment's size in bytes, 0 when unknown.
    pub file_bytes: f64,
    /// A shared contact's name, empty when this is not a vCard. The core
    /// parses the card; nothing here reads vCard syntax.
    pub vcard_name: QString,
    /// A shared contact's address.
    pub vcard_addr: QString,
    /// A shared contact's colour, `#rrggbb`.
    pub vcard_color: QString,
}

/// Conversation model bound to a `SilicaListView` from QML.
pub type MessageListModel = SimpleListModel<MessageListItem>;

/// One account from `get_all_accounts`.
#[derive(Default, Clone, qmetaobject::SimpleListItem)]
pub struct AccountItem {
    /// The core's account id.
    pub account_id: u32,
    /// Profile display name, empty when unset.
    pub display_name: QString,
    /// The account's email address, empty when unconfigured.
    pub addr: QString,
    /// Whether this account has a usable transport.
    pub is_configured: bool,
}

/// Account model, for the account switcher.
pub type AccountListModel = SimpleListModel<AccountItem>;

/// One contact from `get_contacts`.
#[derive(Default, Clone, PartialEq, qmetaobject::SimpleListItem)]
pub struct ContactItem {
    /// The core's contact id.
    pub contact_id: u32,
    /// Name to show: the contact's own, else their address.
    pub display_name: QString,
    /// Email address.
    pub address: QString,
    /// Verified through a secure-join.
    pub is_verified: bool,
    /// Reachable with encryption. An address contact is not.
    pub is_key_contact: bool,
    /// The core's per-contact colour, `#rrggbb`, for the avatar.
    pub color: QString,
    /// Path to the contact's picture, empty when they have none.
    pub avatar_path: QString,
}

/// Contact model, for pickers and the contact list.
pub type ContactListModel = SimpleListModel<ContactItem>;

/// One row of a search, whatever kind of thing it found.
///
/// The three kinds live in one model so a single list can show them under
/// counted headings the way the reference clients do -- QML's `section`
/// groups a flat model, and there is no way to give one view three.
#[derive(Default, Clone, qmetaobject::SimpleListItem)]
pub struct SearchItem {
    /// `chat`, `contact` or `message`. QML sections on this, and the
    /// delegate picks what to draw from it.
    pub kind: QString,
    /// The chat to open. A message row carries the chat it is in.
    pub chat_id: u32,
    /// Contact rows only; the chat with them may not exist yet.
    pub contact_id: u32,
    /// Message rows only, so a hit can be pointed at later.
    pub message_id: u32,
    /// Chat name, contact name, or the name of the chat a message is in.
    pub title: QString,
    /// The last message, the contact's address, or the matching text.
    pub subtitle: QString,
    /// Unix seconds. 0 where the row has no time worth showing.
    pub timestamp: i64,
    /// The core's colour for the chat or contact, `#rrggbb`.
    pub color: QString,
    /// Picture for the chat or contact, empty when there is none.
    pub avatar_path: QString,
}

/// Search model, for the grouped results list.
pub type SearchListModel = SimpleListModel<SearchItem>;
