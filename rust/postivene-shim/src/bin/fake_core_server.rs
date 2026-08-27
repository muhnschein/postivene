//! A `deltachat-rpc-server` double that *records* what it was asked.
//!
//! The onboarding tests care less about what comes back than about what
//! goes out: which JSON-RPC methods a UI action issues, in what order, with
//! what parameters. That is the contract this repository keeps with the
//! core, and it is exactly what a refactor breaks silently -- calling the
//! deprecated `configure` again, dropping the display name, sending the
//! login parameters in the wrong shape.
//!
//! So every request is appended as one JSON line to the file named by
//! `POSTIVENE_FAKE_JOURNAL`, and tests assert against that journal.
//! `get_next_event_batch` is left out of it: the client polls it in a loop,
//! and its noise would bury the sequence being checked.
//!
//! Behaviour is keyed on the *input*, never on an environment switch, so a
//! single test process can drive both a success and a failure:
//! a QR payload or address containing `fail` is rejected the way the real
//! core rejects an unreachable server.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// One account, as `get_all_accounts` reports it.
struct Account {
    id: u32,
    configured: bool,
}

#[derive(Default)]
struct State {
    accounts: Vec<Account>,
    /// Events waiting to be handed out by `get_next_event_batch`.
    events: VecDeque<Value>,
}

impl State {
    fn account_list(&self) -> Value {
        Value::Array(
            self.accounts
                .iter()
                .map(|account| {
                    json!({
                        "id": account.id,
                        "kind": if account.configured { "Configured" } else { "Unconfigured" },
                        "displayName": "",
                        "addr": "",
                    })
                })
                .collect(),
        )
    }

    /// Mark an account configured and queue the progress events the real
    /// core emits while doing so: some permille steps, then 1000 for done.
    fn configure(&mut self, account_id: u32) {
        for account in &mut self.accounts {
            if account.id == account_id {
                account.configured = true;
            }
        }
        for progress in [300_u32, 1000] {
            self.events.push_back(json!({
                "contextId": account_id,
                "event": {"kind": "ConfigureProgress", "progress": progress, "comment": null},
            }));
        }
    }
}

fn journal(method: &str, params: &Value) {
    let Ok(path) = std::env::var("POSTIVENE_FAKE_JOURNAL") else {
        return;
    };
    // Polling noise would bury the call sequence a test is reading.
    if method == "get_next_event_batch" {
        return;
    }
    let line = json!({"method": method, "params": params}).to_string();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// True for the inputs that stand in for "the server cannot be reached".
fn should_fail(value: &str) -> bool {
    value.contains("fail")
}

#[tokio::main]
async fn main() {
    let state = Arc::new(Mutex::new(State::default()));
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let state = state.clone();
        let stdout = stdout.clone();
        tokio::spawn(async move {
            let Ok(request) = serde_json::from_str::<Value>(&line) else {
                return;
            };
            let Some(id) = request.get("id").cloned() else {
                return;
            };
            let method = request
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            journal(&method, &params);

            let positional = |index: usize| -> Value {
                params
                    .as_array()
                    .and_then(|array| array.get(index))
                    .cloned()
                    .unwrap_or(Value::Null)
            };
            let account_id = || -> u32 {
                positional(0)
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or_default()
            };

            let response = match method.as_str() {
                "get_system_info" => ok(&id, &json!({"name": "fake-core-server"})),
                "get_all_accounts" => {
                    let list = state.lock().await.account_list();
                    ok(&id, &list)
                }
                "add_account" => {
                    let mut state = state.lock().await;
                    let next = u32::try_from(state.accounts.len()).unwrap_or(0) + 1;
                    state.accounts.push(Account {
                        id: next,
                        configured: false,
                    });
                    ok(&id, &json!(next))
                }
                "set_config" | "start_io" | "stop_ongoing_process" => ok(&id, &Value::Null),
                "add_transport_from_qr" => {
                    let qr = positional(1).as_str().unwrap_or_default().to_string();
                    if should_fail(&qr) {
                        err(&id, "cannot resolve chatmail server")
                    } else {
                        state.lock().await.configure(account_id());
                        ok(&id, &Value::Null)
                    }
                }
                "add_or_update_transport" => {
                    let param = positional(1);
                    let addr = param
                        .get("addr")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let has_password = param
                        .get("password")
                        .and_then(Value::as_str)
                        .is_some_and(|password| !password.is_empty());
                    if addr.is_empty() || !has_password {
                        // The real core answers a malformed EnteredLoginParam
                        // with an invalid-params error, and so does this: a
                        // test that sends the wrong shape should fail here
                        // rather than pass and break on a phone.
                        err(&id, "invalid params: addr and password are required")
                    } else if should_fail(addr) {
                        err(&id, "could not connect to server")
                    } else {
                        state.lock().await.configure(account_id());
                        ok(&id, &Value::Null)
                    }
                }
                "list_transports" => ok(&id, &json!([{"addr": "someone@example.org"}])),
                "get_next_event_batch" => {
                    // Hand out whatever is queued; when nothing is, block
                    // forever, which is what the real long poll does.
                    loop {
                        let queued: Vec<Value> = state.lock().await.events.drain(..).collect();
                        if !queued.is_empty() {
                            break ok(&id, &Value::Array(queued));
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    }
                }
                _ => err(&id, "method not found"),
            };

            let mut out = stdout.lock().await;
            let _ = out.write_all(response.to_string().as_bytes()).await;
            let _ = out.write_all(b"\n").await;
            let _ = out.flush().await;
        });
    }
}

fn ok(id: &Value, result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn err(id: &Value, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": message}})
}
