use crate::protocol::ErrorObject;

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error("failed to spawn deltachat-rpc-server: {0}")]
    Io(#[from] std::io::Error),
    #[error("spawned child has no stdin/stdout pipe (this is a bug)")]
    MissingPipe,
}

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// The server returned a JSON-RPC error object for this call.
    #[error("deltachat-rpc-server returned an error ({}): {}", .0.code, .0.message)]
    Remote(ErrorObject),

    /// The result payload didn't match the type the caller asked for.
    #[error("failed to decode result for method {method}: {source}")]
    Decode {
        method: String,
        #[source]
        source: serde_json::Error,
    },

    /// The transport (child process stdin/stdout, or the reader/writer
    /// tasks that own them) is gone -- almost always because the
    /// `deltachat-rpc-server` process exited or was killed.
    #[error("deltachat-rpc-server transport closed")]
    TransportClosed,

    #[error("failed to serialize params for method {method}: {source}")]
    Encode {
        method: String,
        #[source]
        source: serde_json::Error,
    },
}
