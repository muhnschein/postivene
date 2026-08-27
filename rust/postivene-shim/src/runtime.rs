//! The tokio runtime, kept off the Qt main thread.
//!
//! Building a tokio runtime is not a passive operation: `Builder::build`
//! finishes with `Handle::enter()`, and dropping a `Runtime` re-enters the
//! runtime context, so both touch tokio's thread-local context. On the
//! Sailfish aarch64 build that thread-local turns out not to be usable on
//! the Qt main thread -- `Runtime::new()` panics with
//! `already borrowed: BorrowMutError` inside `Builder::build`, before any
//! of our own code gets to run:
//!
//! ```text
//! tokio::runtime::context::current::Context::set_current
//! tokio::runtime::builder::Builder::build
//! tokio::runtime::runtime::Runtime::new
//! postivene_shim::core::DeltaChatCore::start
//! <postivene_shim::core::DeltaChatCore as qmetaobject::QObject>::static_metacall
//! ```
//!
//! (Reproduced on a Jolla phone; the same `Runtime::new()` in a non-Qt
//! binary built from the same toolchain works, so it is specific to doing
//! it on the Qt main thread of this build.)
//!
//! `CoreRuntime` therefore creates the runtime on a thread of its own and
//! hands back only a `Handle`. `Handle::spawn` never touches the runtime
//! context, so the Qt thread can keep spawning work exactly as before, and
//! the `Runtime` itself is both created and dropped on its own thread.

use std::future::Future;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::{Handle, Runtime};
use tokio::task::JoinHandle;

/// How long `CoreRuntime::new` waits for its thread to hand back a handle.
/// This is a thread spawn plus a runtime build, i.e. milliseconds; the
/// timeout only exists so a wedged thread cannot hang the UI forever.
const HANDLE_TIMEOUT: Duration = Duration::from_secs(5);

/// A handle to a tokio runtime living on its own thread. Cloning is cheap
/// and shares the one runtime; when the last clone goes away, the owning
/// thread is told to shut down and drops the runtime there.
#[derive(Clone)]
pub struct CoreRuntime(Arc<RuntimeThread>);

struct RuntimeThread {
    handle: Handle,
    /// Dropping this sender is what tells the runtime thread to finish.
    _shutdown: mpsc::Sender<()>,
}

impl CoreRuntime {
    pub fn new() -> Result<Self, String> {
        let (handle_tx, handle_rx) = mpsc::channel::<Result<Handle, String>>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        std::thread::Builder::new()
            .name("postivene-tokio".to_string())
            .spawn(move || {
                let runtime = match Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(err) => {
                        let _ = handle_tx.send(Err(err.to_string()));
                        return;
                    }
                };
                if handle_tx.send(Ok(runtime.handle().clone())).is_err() {
                    return;
                }
                // Park until every `CoreRuntime` clone is gone (the send
                // half is dropped, so `recv` returns `Err`). The runtime is
                // then dropped here, on this thread -- never on the Qt one.
                let _ = shutdown_rx.recv();
            })
            .map_err(|err| format!("could not start runtime thread: {err}"))?;

        match handle_rx.recv_timeout(HANDLE_TIMEOUT) {
            Ok(Ok(handle)) => Ok(Self(Arc::new(RuntimeThread {
                handle,
                _shutdown: shutdown_tx,
            }))),
            Ok(Err(err)) => Err(err),
            Err(err) => Err(format!("runtime thread did not start: {err}")),
        }
    }

    /// Spawn onto the runtime. Deliberately goes through `Handle`, which
    /// only enqueues the task -- see the module docs for why the Qt thread
    /// must not enter the runtime context.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.0.handle.spawn(future)
    }
}
