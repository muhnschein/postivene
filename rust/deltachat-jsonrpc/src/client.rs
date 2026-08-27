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

/// Lock a mutex, recovering the guard if a previous holder panicked.
///
/// These mutexes guard plain collections, which a panic leaves structurally
/// intact. Killing the transport over a fault that happened elsewhere is
/// the worse outcome.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A running `deltachat-rpc-server` subprocess, addressable via JSON-RPC 2.0
/// over its stdio.
///
/// Transport only: correlation, framing, process lifecycle. Method names and
/// payloads pass through uninterpreted.
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
    ///
    /// # Errors
    ///
    /// Fails if the process cannot be executed, or if the spawned child
    /// does not expose the stdio pipes we asked for.
    pub async fn spawn<S, I, A>(program: S, args: I) -> Result<Self, SpawnError>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
    {
        Self::spawn_with_env(program, args, std::iter::empty::<(&OsStr, &OsStr)>()).await
    }

    /// Like [`RpcClient::spawn`], with extra environment variables on the
    /// child. `DC_ACCOUNTS_PATH` is the one that matters: without it the
    /// server stores account state under `./accounts`, relative to its
    /// working directory.
    ///
    /// # Errors
    ///
    /// Same as [`RpcClient::spawn`].
    // Stays `async` though nothing awaits: `Command::spawn` needs an active
    // reactor, and an `async fn` can only run inside one.
    #[allow(clippy::unused_async)]
    pub async fn spawn_with_env<S, I, A, E, K, V>(
        program: S,
        args: I,
        envs: E,
    ) -> Result<Self, SpawnError>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let mut child = Command::new(program)
            .args(args)
            .envs(envs)
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
            // Ends on end-of-stdout or an unreadable line: the server is
            // gone either way.
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                // Skip anything that is not a JSON-RPC response rather than
                // tearing down a live transport.
                let Ok(envelope) = serde_json::from_str::<ResponseEnvelope>(&line) else {
                    continue;
                };
                let Some(id) = envelope.id else {
                    // The core delivers events by polling, not by
                    // notification, so this should not happen.
                    continue;
                };
                let outcome = match (envelope.result, envelope.error) {
                    (_, Some(err)) => Err(RpcError::Remote(err)),
                    (Some(result), None) => Ok(result),
                    (None, None) => Ok(serde_json::Value::Null),
                };
                if let Some(sender) = lock(&reader_pending).remove(&id) {
                    let _ = sender.send(outcome);
                }
            }
            // Transport is gone: wake up anyone still waiting rather than
            // leaving them hanging forever.
            for (_, sender) in lock(&reader_pending).drain() {
                let _ = sender.send(Err(RpcError::TransportClosed));
            }
        });

        let stderr_capture = stderr_tail.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut tail = lock(&stderr_capture);
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

    /// Call an RPC method, serializing `params` and deserializing the
    /// result as `R`.
    ///
    /// Method names and shapes come from the core's API (`--openrpc`); this
    /// crate neither hardcodes nor validates them.
    ///
    /// # Errors
    ///
    /// Returns [`RpcError::Encode`] if `params` cannot be serialized,
    /// [`RpcError::Remote`] if the server answered with an error object,
    /// [`RpcError::Decode`] if the result does not fit `R`, and
    /// [`RpcError::TransportClosed`] if the server went away.
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
    ///
    /// # Errors
    ///
    /// Same as [`RpcClient::call`].
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
        lock(&self.pending).insert(id, tx);

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
            lock(&self.pending).remove(&id);
            return Err(RpcError::TransportClosed);
        }

        rx.await.unwrap_or(Err(RpcError::TransportClosed))
    }

    /// The last `STDERR_TAIL_CAPACITY` lines of the server's stderr, oldest
    /// first. Diagnostics for a failed call or an unexpected exit.
    pub fn stderr_tail(&self) -> Vec<String> {
        lock(&self.stderr_tail).clone()
    }

    /// Terminate the child process and wait for the reader/writer tasks to
    /// finish. Safe to call even if the process has already exited.
    ///
    /// # Errors
    ///
    /// Propagates the I/O error from waiting on the child, if any.
    pub async fn shutdown(&self) -> std::io::Result<()> {
        let child = lock(&self.child).take();
        if let Some(mut child) = child {
            let _ = child.start_kill();
            child.wait().await?;
        }
        let reader_task = lock(&self.reader_task).take();
        if let Some(task) = reader_task {
            let _ = task.await;
        }
        let writer_task = lock(&self.writer_task).take();
        if let Some(task) = writer_task {
            task.abort();
        }
        Ok(())
    }
}
