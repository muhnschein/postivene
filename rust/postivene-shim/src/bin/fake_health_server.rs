//! Minimal JSON-RPC double for `postivene-shim`'s own smoke test: answers
//! `get_system_info` only. Separate from
//! `deltachat-jsonrpc/src/bin/fake_rpc_test_server.rs` (that one is not
//! reachable from this crate's `CARGO_BIN_EXE_*` env vars, which Cargo only
//! sets for binaries within the same package).

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let response = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"name": "fake-health-server", "version": "0.0.0-test"},
        });
        let _ = stdout.write_all(response.to_string().as_bytes()).await;
        let _ = stdout.write_all(b"\n").await;
        let _ = stdout.flush().await;
    }
}
