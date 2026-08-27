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

mod core;
mod models;
mod runtime;

pub use crate::core::DeltaChatCore;
pub use models::{ChatListItem, ChatListModel, MessageListItem, MessageListModel};
