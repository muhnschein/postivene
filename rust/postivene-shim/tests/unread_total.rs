//! What the cover counts.
//!
//! The total has to include muted chats: muting silences the
//! announcement, not the arithmetic, and the per-chat badge behaves the
//! same way. It also has to survive a count that would overflow rather
//! than wrapping to a small, plausible-looking number.

use postivene_shim::{ChatList, ChatListItem};

fn chat(chat_id: u32, unread: u32, muted: bool) -> ChatListItem {
    ChatListItem {
        chat_id,
        unread_count: unread,
        is_muted: muted,
        ..ChatListItem::default()
    }
}

#[test]
fn the_total_counts_every_chat_including_muted_ones() {
    let list = ChatList::default();
    assert_eq!(list.unread_total(), 0, "an empty list is not silent");

    {
        let mut rows = list.rows.borrow_mut();
        rows.push(chat(1, 3, false));
        rows.push(chat(2, 0, false));
        // Muted, and still waiting to be read.
        rows.push(chat(3, 4, true));
    }

    assert_eq!(
        list.unread_total(),
        7,
        "the cover total dropped a muted chat's unread messages"
    );
}

#[test]
fn an_absurd_count_saturates_rather_than_wrapping() {
    let list = ChatList::default();
    {
        let mut rows = list.rows.borrow_mut();
        rows.push(chat(1, u32::MAX, false));
        rows.push(chat(2, 5, false));
    }
    assert_eq!(
        list.unread_total(),
        u32::MAX,
        "a total past u32 wrapped, so a huge backlog would show as a small \
         number on the cover"
    );
}
