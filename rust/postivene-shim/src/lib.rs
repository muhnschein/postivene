//! Qt/QML integration layer over `deltachat-jsonrpc`: [`DeltaChatCore`] as a
//! `QObject`, chat and message lists as `QAbstractListModel`s.
//!
//! Needs Qt5 dev headers to build.

// All four are forced by qmetaobject's derive macros, not by code here:
// the macros expand to a wide, unstable set of items from that crate
// (`wildcard_imports`), generate the `QObject` impl and dispatcher
// (`useless_transmute`, `type_complexity`), and require by-value `QString`
// parameters (`needless_pass_by_value`).
#![allow(
    clippy::wildcard_imports,
    clippy::useless_transmute,
    clippy::type_complexity,
    clippy::needless_pass_by_value
)]

mod chat;
mod chatlist;
mod core;
mod models;
mod runtime;

pub use crate::chat::ChatMessages;
pub use crate::chatlist::ChatList;
pub use crate::core::DeltaChatCore;

/// Register the shim's QML-instantiable types. The app and the tests share
/// this so a page cannot work in one and not the other.
pub fn register_qml_types() {
    // Nul-terminated literals; `c"..."` would be neater but arrived in Rust
    // 1.77, past the 1.75 floor Sailfish sets.
    let (Ok(uri), Ok(messages), Ok(list)) = (
        std::ffi::CStr::from_bytes_with_nul(b"Postivene\0"),
        std::ffi::CStr::from_bytes_with_nul(b"ChatMessages\0"),
        std::ffi::CStr::from_bytes_with_nul(b"ChatList\0"),
    ) else {
        return;
    };
    qmetaobject::qml_register_type::<ChatMessages>(uri, 1, 0, messages);
    qmetaobject::qml_register_type::<ChatList>(uri, 1, 0, list);
}
pub use models::{ChatListItem, ChatListModel, MessageListItem, MessageListModel};
