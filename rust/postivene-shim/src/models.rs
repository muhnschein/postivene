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
    pub chat_id: u32,
    pub name: QString,
    /// Last-message preview text (`summaryText2` upstream).
    pub preview: QString,
    pub unread_count: u32,
}

pub type ChatListModel = SimpleListModel<ChatListItem>;

/// One row of a message list, as surfaced by `get_message`/
/// `get_message_list_items`. `is_outgoing` is derived from `from_id`
/// upstream (contact id `1` is the well-known `DC_CONTACT_ID_SELF`).
#[derive(Default, Clone, qmetaobject::SimpleListItem)]
pub struct MessageListItem {
    pub message_id: u32,
    pub text: QString,
    pub is_outgoing: bool,
    /// Unix timestamp, seconds.
    pub timestamp: i64,
}

pub type MessageListModel = SimpleListModel<MessageListItem>;
