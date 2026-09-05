//! Error types for the transport: failures spawning the server process,
//! and failures of an individual RPC call.
//!
//! Written out rather than derived: two enums and a dozen lines of
//! `Display` are not worth a proc-macro crate and its own in the tree.

use std::fmt;

use crate::protocol::ErrorObject;

/// Why `deltachat-rpc-server` could not be started.
#[derive(Debug)]
pub enum SpawnError {
    /// The process could not be executed at all (missing binary, wrong
    /// permissions, ...).
    Io(std::io::Error),
    /// The child was spawned but one of the stdio pipes we asked for is
    /// missing, which should be impossible.
    MissingPipe,
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to spawn deltachat-rpc-server: {err}"),
            Self::MissingPipe => {
                f.write_str("spawned child has no stdin/stdout pipe (this is a bug)")
            }
        }
    }
}

impl std::error::Error for SpawnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::MissingPipe => None,
        }
    }
}

impl From<std::io::Error> for SpawnError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Why a single RPC call did not produce a result.
#[derive(Debug)]
pub enum RpcError {
    /// The server returned a JSON-RPC error object for this call.
    Remote(ErrorObject),

    /// The result payload didn't match the type the caller asked for.
    Decode {
        /// The method whose result could not be decoded.
        method: String,
        /// The underlying serde error.
        source: serde_json::Error,
    },

    /// The transport (child process stdin/stdout, or the reader/writer
    /// tasks that own them) is gone -- almost always because the
    /// `deltachat-rpc-server` process exited or was killed.
    TransportClosed,

    /// The server took the request and never answered it.
    Timeout {
        /// The method that went unanswered.
        method: String,
        /// How long the call waited.
        seconds: u64,
    },

    /// The caller's params could not be serialized to JSON.
    Encode {
        /// The method whose params could not be serialized.
        method: String,
        /// The underlying serde error.
        source: serde_json::Error,
    },
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Remote(error) => write!(
                f,
                "deltachat-rpc-server returned an error ({}): {}",
                error.code, error.message
            ),
            Self::Decode { method, source } => {
                write!(f, "failed to decode result for method {method}: {source}")
            }
            Self::TransportClosed => f.write_str("deltachat-rpc-server transport closed"),
            Self::Timeout { method, seconds } => write!(
                f,
                "deltachat-rpc-server did not answer {method} within {seconds}s"
            ),
            Self::Encode { method, source } => {
                write!(
                    f,
                    "failed to serialize params for method {method}: {source}"
                )
            }
        }
    }
}

impl std::error::Error for RpcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode { source, .. } | Self::Encode { source, .. } => Some(source),
            Self::Remote(_) | Self::TransportClosed | Self::Timeout { .. } => None,
        }
    }
}
