//! Wire types for the JSON-RPC 2.0 messages exchanged with
//! `deltachat-rpc-server` over newline-delimited stdio (confirmed against
//! upstream `deltachat-rpc-server/src/main.rs`: it reads `stdin` line by
//! line and writes each response with `println!`, i.e. JSON Lines, not
//! `Content-Length`-framed like LSP).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub(crate) struct RequestEnvelope<'a> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'a str,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResponseEnvelope {
    #[allow(dead_code)]
    pub jsonrpc: Option<String>,
    pub id: Option<u64>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<ErrorObject>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ErrorObject {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}
