//! Exercises `RpcClient` against `fake-rpc-test-server` (see
//! `src/bin/fake_rpc_test_server.rs`) instead of a real
//! `deltachat-rpc-server` binary, since this crate has no dependency on the
//! Delta Chat core itself -- it only needs *some* process speaking
//! newline-delimited JSON-RPC 2.0 on stdio to prove the transport works.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use deltachat_jsonrpc::{spawn_event_loop, RpcClient, RpcError};
use serde_json::json;

fn fake_server_path() -> &'static str {
    env!("CARGO_BIN_EXE_fake-rpc-test-server")
}

#[tokio::test]
async fn health_check_round_trip() {
    let client = RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
        .await
        .expect("spawn fake server");

    let info: HashMap<String, String> = client
        .call_unit("get_system_info")
        .await
        .expect("get_system_info call");

    assert_eq!(info.get("name").map(String::as_str), Some("fake-rpc-test-server"));
    assert_eq!(info.get("version").map(String::as_str), Some("0.0.0-test"));

    client.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn echoes_arbitrary_json_params() {
    let client = RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
        .await
        .expect("spawn fake server");

    let payload = json!({"nested": {"a": [1, 2, 3]}, "ok": true});
    let echoed: serde_json::Value = client
        .call("echo", payload.clone())
        .await
        .expect("echo call");

    assert_eq!(echoed, payload);
    client.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_errors_are_reported_as_rpc_error() {
    let client = RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
        .await
        .expect("spawn fake server");

    let result: Result<serde_json::Value, RpcError> = client.call_unit("fail").await;
    match result {
        Err(RpcError::Remote(err)) => {
            assert_eq!(err.code, -32000);
            assert_eq!(err.message, "boom");
        }
        other => panic!("expected RpcError::Remote, got {other:?}"),
    }

    client.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn unknown_method_reports_method_not_found() {
    let client = RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
        .await
        .expect("spawn fake server");

    let result: Result<serde_json::Value, RpcError> = client.call_unit("does_not_exist").await;
    match result {
        Err(RpcError::Remote(err)) => assert_eq!(err.code, -32601),
        other => panic!("expected RpcError::Remote(-32601), got {other:?}"),
    }

    client.shutdown().await.expect("shutdown");
}

/// Two concurrent calls must be correlated by id independently of send
/// order: a fast call issued *after* a slow one should still complete
/// first, proving requests aren't serialized behind each other.
#[tokio::test]
async fn concurrent_calls_are_correlated_independently_of_order() {
    let client = Arc::new(
        RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
            .await
            .expect("spawn fake server"),
    );

    let slow_client = client.clone();
    let slow_handle = tokio::spawn(async move {
        let start = Instant::now();
        let result: String = slow_client
            .call("slow", (300u64,))
            .await
            .expect("slow call");
        assert_eq!(result, "slow-done");
        start.elapsed()
    });

    // Give the slow request time to be in flight before firing the fast one.
    tokio::time::sleep(Duration::from_millis(30)).await;

    let echo_start = Instant::now();
    let _: serde_json::Value = client.call("echo", json!("hi")).await.expect("echo call");
    let echo_elapsed = echo_start.elapsed();

    assert!(
        echo_elapsed < Duration::from_millis(250),
        "echo took {echo_elapsed:?}, expected it to complete well before the slow call"
    );

    let slow_elapsed = slow_handle.await.expect("slow task");
    assert!(slow_elapsed >= Duration::from_millis(300));

    client.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn event_loop_streams_batches_in_order() {
    let client = Arc::new(
        RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
            .await
            .expect("spawn fake server"),
    );

    let (mut events, handle) = spawn_event_loop(client.clone());

    let first = events.recv().await.expect("first event");
    assert_eq!(first.context_id, 1);
    assert_eq!(first.event["msg"], json!("batch 1"));

    let second = events.recv().await.expect("second event");
    assert_eq!(second.event["msg"], json!("batch 2"));

    // The fake server now simulates an indefinite long-poll with no new
    // events; stopping the handle must not hang the test.
    handle.stop();
    client.shutdown().await.expect("shutdown");
}
