//! Transport for talking to a `deltachat-rpc-server` subprocess over its
//! JSON-RPC 2.0 stdio interface.
//!
//! This crate deliberately knows nothing about Delta Chat's protocol,
//! accounts, chats, or messages -- it only spawns the server, frames
//! newline-delimited JSON-RPC requests/responses, correlates them by id,
//! and offers a way to drain the core's event stream. All of that logic
//! lives in the upstream core; see `docs/SCOPE.md` §3 in the repository
//! root for why that boundary is kept strict.

mod client;
mod error;
mod events;
mod protocol;

pub use client::RpcClient;
pub use error::{RpcError, SpawnError};
pub use events::{spawn_event_loop, CoreEvent, EventLoopHandle};
pub use protocol::ErrorObject;
