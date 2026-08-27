//! Qt/QML integration layer over `deltachat-jsonrpc`: exposes
//! [`DeltaChatCore`] as a `QObject` and chat/message lists as
//! `QAbstractListModel`s (via `qmetaobject`'s `SimpleListModel`) for use
//! from Silica QML.
//!
//! Requires Qt5 dev headers to build (`qtbase5-dev`, `qtdeclarative5-dev`
//! on Debian/Ubuntu-family hosts, or the Sailfish SDK's Qt5 for target
//! builds).

mod core;
mod models;
mod runtime;

pub use crate::core::DeltaChatCore;
pub use models::{ChatListItem, ChatListModel, MessageListItem, MessageListModel};
