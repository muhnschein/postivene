use qmetaobject::listmodel::SimpleListModel;
use qmetaobject::QString;

/// One row of a chat list, as surfaced by the core's
/// `get_chatlist_items_by_entries` JSON-RPC method (see
/// `ChatListItemFetchResult::ChatListItem` in `chatmail/core`). Only the
/// fields a minimal chat list UI needs are carried over; the full
/// `ChatListItemFetchResult` has ~20 fields (archived/pinned/muted flags,
/// encryption state, etc.) that can be added here as the UI grows, rather
/// than mirrored wholesale up front.
#[derive(Default, Clone, qmetaobject::SimpleListItem)]
pub struct ChatListItem {
    /// The core's chat id.
    pub chat_id: u32,
    /// Display name of the chat.
    pub name: QString,
    /// Last-message preview text (`summaryText2` upstream).
    pub preview: QString,
    /// Fresh (unseen) message count, for the badge.
    pub unread_count: u32,
    /// Whether all messages/contacts in this chat are encrypted
    /// (`isEncrypted` upstream). Unencrypted chats should be marked with a
    /// mail icon per upstream UI guidance.
    pub is_encrypted: bool,
}

/// Chat-list model bound to a `SilicaListView` from QML.
pub type ChatListModel = SimpleListModel<ChatListItem>;

/// One row of a message list, as surfaced by `get_message`/
/// `get_message_list_items`. `is_outgoing` is derived from `from_id`
/// upstream (contact id `1` is the well-known `DC_CONTACT_ID_SELF`).
#[derive(Default, Clone, qmetaobject::SimpleListItem)]
pub struct MessageListItem {
    /// The core's message id.
    pub message_id: u32,
    /// Message body text.
    pub text: QString,
    /// True when this device's account is the sender.
    pub is_outgoing: bool,
    /// Unix timestamp, seconds.
    pub timestamp: i64,
    /// `showPadlock` upstream: true if correctly encrypted & signed.
    /// Upstream guidance: show a small email icon when this is *false*,
    /// nothing when true.
    pub show_padlock: bool,
    /// Upstream `state` (the classic `DC_STATE_*` constants, verified
    /// against `chatmail/core` `src/message.rs` for v2.53.0): 20 = out
    /// pending, 24 = out failed, 26 = out delivered, 28 = out read
    /// (MDN received); incoming: 10 fresh, 13 noticed, 16 seen.
    pub state: u32,
}

/// Conversation model bound to a `SilicaListView` from QML.
pub type MessageListModel = SimpleListModel<MessageListItem>;

/// One account, as surfaced by `get_all_accounts` (upstream `Account`:
/// tagged `kind` = "Configured"/"Unconfigured", camelCase fields).
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
