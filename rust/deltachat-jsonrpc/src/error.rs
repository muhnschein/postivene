//! Error types for the transport: failures spawning the server process,
//! and failures of an individual RPC call.

use crate::protocol::ErrorObject;

/// Why `deltachat-rpc-server` could not be started.
#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    /// The process could not be executed at all (missing binary, wrong
    /// permissions, ...).
    #[error("failed to spawn deltachat-rpc-server: {0}")]
    Io(#[from] std::io::Error),
    /// The child was spawned but one of the stdio pipes we asked for is
    /// missing, which should be impossible.
    #[error("spawned child has no stdin/stdout pipe (this is a bug)")]
    MissingPipe,
}

/// Why a single RPC call did not produce a result.
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// The server returned a JSON-RPC error object for this call.
    #[error("deltachat-rpc-server returned an error ({}): {}", .0.code, .0.message)]
    Remote(ErrorObject),

    /// The result payload didn't match the type the caller asked for.
    #[error("failed to decode result for method {method}: {source}")]
    Decode {
        /// The method whose result could not be decoded.
        method: String,
        /// The underlying serde error.
        #[source]
        source: serde_json::Error,
    },

    /// The transport (child process stdin/stdout, or the reader/writer
    /// tasks that own them) is gone -- almost always because the
    /// `deltachat-rpc-server` process exited or was killed.
    #[error("deltachat-rpc-server transport closed")]
    TransportClosed,

    /// The server took the request and never answered it.
    #[error("deltachat-rpc-server did not answer {method} within {seconds}s")]
    Timeout {
        /// The method that went unanswered.
        method: String,
        /// How long the call waited.
        seconds: u64,
    },

    /// The caller's params could not be serialized to JSON.
    #[error("failed to serialize params for method {method}: {source}")]
    Encode {
        /// The method whose params could not be serialized.
        method: String,
        /// The underlying serde error.
        #[source]
        source: serde_json::Error,
    },
}
