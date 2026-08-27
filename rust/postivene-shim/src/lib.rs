//! Qt/QML integration layer over `deltachat-jsonrpc`: exposes
//! [`DeltaChatCore`] as a `QObject` and chat/message lists as
//! `QAbstractListModel`s (via `qmetaobject`'s `SimpleListModel`) for use
//! from Silica QML.
//!
//! Requires Qt5 dev headers to build (`qtbase5-dev`, `qtdeclarative5-dev`
//! on Debian/Ubuntu-family hosts, or the Sailfish SDK's Qt5 for target
//! builds).

// Three exceptions to the workspace lint set, all forced by qmetaobject's
// derive macros rather than by code written here:
//
// * `wildcard_imports`: `qmetaobject`'s `qt_property!`/`qt_method!`/
//   `qt_signal!` macros expand to references to a long and unstable list of
//   items from that crate; upstream's own examples use the glob, and
//   spelling the list out would break on every qmetaobject release.
// * `useless_transmute` and `type_complexity`: emitted inside
//   `#[derive(QObject)]`'s generated `QObject` impl and its
//   `static_metacall` dispatcher.
// * `needless_pass_by_value`: `qt_method!` declarations must match the
//   generated dispatcher's by-value `QString` parameters.
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
