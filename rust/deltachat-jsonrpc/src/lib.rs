//! Transport for a `deltachat-rpc-server` subprocess over its JSON-RPC 2.0
//! stdio interface.
//!
//! Knows nothing of Delta Chat's protocol, accounts, chats or messages: it
//! spawns the server, frames and correlates requests, and drains the event
//! stream. See `docs/PROJECT.md` for why that boundary is strict.

mod client;
mod error;
mod events;
mod protocol;

pub use client::RpcClient;
pub use error::{RpcError, SpawnError};
pub use events::{spawn_event_loop, CoreEvent, EventLoopHandle};
pub use protocol::ErrorObject;
