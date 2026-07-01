//! Stand-in for `deltachat-rpc-server` used only by this crate's own tests.
//!
//! Speaks the same wire format (newline-delimited JSON-RPC 2.0 on
//! stdin/stdout) and handles requests concurrently (each line is dispatched
//! to its own task), so the test suite can exercise request/response
//! correlation, out-of-order concurrent replies, error propagation, and the
//! `get_next_event_batch` long-poll pattern without needing a real Delta
//! Chat core binary.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
    let batch_counter = Arc::new(Mutex::new(0u32));

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let stdout = stdout.clone();
        let batch_counter = batch_counter.clone();
        tokio::spawn(async move {
            let request: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => return,
            };
            let id = request.get("id").cloned();
            let method = request
                .get("method")
                .and_then(|m| m.as_str())
                .unwrap_or_default()
                .to_string();
            let params = request.get("params").cloned().unwrap_or(Value::Null);

            let response = match method.as_str() {
                "get_system_info" => ok(id, json!({"name": "fake-rpc-test-server", "version": "0.0.0-test"})),
                "echo" => ok(id, params),
                "add" => {
                    let nums = params.as_array().cloned().unwrap_or_default();
                    let sum: f64 = nums.iter().filter_map(Value::as_f64).sum();
                    ok(id, json!(sum))
                }
                "fail" => err(id, -32000, "boom"),
                "slow" => {
                    let ms = params
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                    ok(id, json!("slow-done"))
                }
                "get_next_event_batch" => {
                    let mut count = batch_counter.lock().await;
                    *count += 1;
                    if *count <= 2 {
                        let n = *count;
                        ok(
                            id,
                            json!([{
                                "contextId": 1,
                                "event": {"kind": "Info", "msg": format!("batch {n}")},
                            }]),
                        )
                    } else {
                        // No more events: emulate the real long-poll
                        // behavior of blocking until something new
                        // happens, which in this test double is "never".
                        drop(count);
                        std::future::pending::<()>().await;
                        unreachable!("pending future never resolves")
                    }
                }
                _ => err(id, -32601, "method not found"),
            };

            if let Some(response) = response {
                let mut out = stdout.lock().await;
                let _ = out.write_all(response.to_string().as_bytes()).await;
                let _ = out.write_all(b"\n").await;
                let _ = out.flush().await;
            }
        });
    }
}

fn ok(id: Option<Value>, result: Value) -> Option<Value> {
    id.map(|id| json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn err(id: Option<Value>, code: i64, message: &str) -> Option<Value> {
    id.map(|id| json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}))
}
