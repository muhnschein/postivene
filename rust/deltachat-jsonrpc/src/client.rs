use std::collections::HashMap;
use std::ffi::OsStr;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::error::{RpcError, SpawnError};
use crate::protocol::{RequestEnvelope, ResponseEnvelope};

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, RpcError>>>>>;

/// A running `deltachat-rpc-server` subprocess, addressable via JSON-RPC 2.0
/// calls over its stdio.
///
/// This type owns *transport only*: request/response correlation, framing,
/// and process lifecycle. It has no knowledge of any particular RPC method
/// -- callers pass method names and JSON params/results through
/// [`RpcClient::call`]. All Delta Chat protocol semantics live in the
/// upstream core; this crate never interprets them.
pub struct RpcClient {
    stdin_tx: mpsc::UnboundedSender<String>,
    pending: PendingMap,
    next_id: AtomicU64,
    child: Mutex<Option<Child>>,
    reader_task: Mutex<Option<JoinHandle<()>>>,
    writer_task: Mutex<Option<JoinHandle<()>>>,
    /// Lines the server wrote to stderr, most recent last, capped to a
    /// small ring so a misbehaving server can't grow this unbounded.
    stderr_tail: Arc<Mutex<Vec<String>>>,
}

const STDERR_TAIL_CAPACITY: usize = 200;

impl RpcClient {
    /// Spawn `program` (typically the path to a bundled
    /// `deltachat-rpc-server` binary) with `args` and set up the
    /// request/response machinery over its stdio.
    pub async fn spawn<S, I, A>(program: S, args: I) -> Result<Self, SpawnError>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
    {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child.stdin.take().ok_or(SpawnError::MissingPipe)?;
        let stdout = child.stdout.take().ok_or(SpawnError::MissingPipe)?;
        let stderr = child.stderr.take().ok_or(SpawnError::MissingPipe)?;

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));

        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();
        let writer_task = tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(line) = stdin_rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.write_all(b"\n").await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        let reader_pending = pending.clone();
        let reader_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<ResponseEnvelope>(&line) {
                            Ok(envelope) => {
                                let Some(id) = envelope.id else {
                                    // A notification with no id. The core's
                                    // JSON-RPC API delivers events via the
                                    // polling `get_next_event`/
                                    // `get_next_event_batch` methods rather
                                    // than unsolicited notifications, so we
                                    // don't expect these in practice; ignore
                                    // rather than crash if the server ever
                                    // sends one.
                                    continue;
                                };
                                let outcome = match (envelope.result, envelope.error) {
                                    (_, Some(err)) => Err(RpcError::Remote(err)),
                                    (Some(result), None) => Ok(result),
                                    (None, None) => Ok(serde_json::Value::Null),
                                };
                                if let Some(sender) =
                                    reader_pending.lock().unwrap().remove(&id)
                                {
                                    let _ = sender.send(outcome);
                                }
                            }
                            Err(_) => continue,
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            // Transport is gone: wake up anyone still waiting rather than
            // leaving them hanging forever.
            for (_, sender) in reader_pending.lock().unwrap().drain() {
                let _ = sender.send(Err(RpcError::TransportClosed));
            }
        });

        let stderr_capture = stderr_tail.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut tail = stderr_capture.lock().unwrap();
                tail.push(line);
                if tail.len() > STDERR_TAIL_CAPACITY {
                    let excess = tail.len() - STDERR_TAIL_CAPACITY;
                    tail.drain(0..excess);
                }
            }
        });

        Ok(Self {
            stdin_tx,
            pending,
            next_id: AtomicU64::new(1),
            child: Mutex::new(Some(child)),
            reader_task: Mutex::new(Some(reader_task)),
            writer_task: Mutex::new(Some(writer_task)),
            stderr_tail,
        })
    }

    /// Call an RPC method, serializing `params` as the JSON-RPC params array
    /// (or object) and deserializing the result as `R`.
    ///
    /// Method names and param/result shapes are defined entirely by the
    /// core's JSON-RPC API (see the `--openrpc` output of
    /// `deltachat-rpc-server`); this crate doesn't hardcode or validate
    /// them.
    pub async fn call<P, R>(&self, method: &str, params: P) -> Result<R, RpcError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let value = self.call_raw(method, &params).await?;
        serde_json::from_value(value).map_err(|source| RpcError::Decode {
            method: method.to_string(),
            source,
        })
    }

    /// Call a method that takes no params.
    pub async fn call_unit<R>(&self, method: &str) -> Result<R, RpcError>
    where
        R: DeserializeOwned,
    {
        self.call(method, ()).await
    }

    async fn call_raw(
        &self,
        method: &str,
        params: &impl Serialize,
    ) -> Result<serde_json::Value, RpcError> {
        let params = serde_json::to_value(params).map_err(|source| RpcError::Encode {
            method: method.to_string(),
            source,
        })?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);

        let request = RequestEnvelope {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };
        let line = serde_json::to_string(&request).map_err(|source| RpcError::Encode {
            method: method.to_string(),
            source,
        })?;

        if self.stdin_tx.send(line).is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(RpcError::TransportClosed);
        }

        rx.await.unwrap_or(Err(RpcError::TransportClosed))
    }

    /// The last [`STDERR_TAIL_CAPACITY`] lines the server has written to
    /// stderr, oldest first. Useful for surfacing diagnostics when a call
    /// fails or the process exits unexpectedly.
    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr_tail.lock().unwrap().clone()
    }

    /// Terminate the child process and wait for the reader/writer tasks to
    /// finish. Safe to call even if the process has already exited.
    pub async fn shutdown(&self) -> std::io::Result<()> {
        let child = self.child.lock().unwrap().take();
        if let Some(mut child) = child {
            let _ = child.start_kill();
            child.wait().await?;
        }
        let reader_task = self.reader_task.lock().unwrap().take();
        if let Some(task) = reader_task {
            let _ = task.await;
        }
        let writer_task = self.writer_task.lock().unwrap().take();
        if let Some(task) = writer_task {
            task.abort();
        }
        Ok(())
    }
}
