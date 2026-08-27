use qmetaobject::listmodel::SimpleListModel;
use qmetaobject::QString;

/// One chat-list row from `get_chatlist_items_by_entries`. Carries only the
/// fields the UI uses; upstream's has ~20 more.
#[derive(Default, Clone, PartialEq, qmetaobject::SimpleListItem)]
pub struct ChatListItem {
    /// The core's chat id.
    pub chat_id: u32,
    /// Display name of the chat.
    pub name: QString,
    /// Last-message preview (`summaryText2` upstream).
    pub preview: QString,
    /// Fresh (unseen) message count, for the badge.
    pub unread_count: u32,
    /// `isEncrypted` upstream. Unencrypted chats get a mail icon.
    pub is_encrypted: bool,
}

/// Chat-list model bound to a `SilicaListView` from QML.
pub type ChatListModel = SimpleListModel<ChatListItem>;

/// One message row from `get_message_list_items` plus `get_message`.
#[derive(Default, Clone, qmetaobject::SimpleListItem)]
pub struct MessageListItem {
    /// The core's message id.
    pub message_id: u32,
    /// Message body text.
    pub text: QString,
    /// True when this account sent it.
    pub is_outgoing: bool,
    /// Unix timestamp, seconds.
    pub timestamp: i64,
    /// `showPadlock` upstream: correctly encrypted and signed. A mail icon
    /// marks the false case.
    pub show_padlock: bool,
    /// `DC_STATE_*`: outgoing 20 pending, 24 failed, 26 delivered, 28 read;
    /// incoming 10 fresh, 13 noticed, 16 seen.
    pub state: u32,
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
