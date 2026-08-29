use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::client::RpcClient;
use crate::error::RpcError;

/// One event as delivered by `get_next_event_batch`.
///
/// `event` stays a raw [`serde_json::Value`]: hand-typing the core's several
/// dozen event variants would be the protocol reimplementation
/// `docs/SCOPE.md` §3 rules out. Callers match on `event["kind"]`.
#[derive(Debug, Clone, Deserialize)]
pub struct CoreEvent {
    /// Which account (the core calls it a "context") the event belongs to.
    #[serde(rename = "contextId")]
    pub context_id: u32,
    /// The event object itself, tagged by its `kind` field.
    pub event: serde_json::Value,
}

/// How many failures that are not a closed transport the loop will take
/// before giving up. A single unreadable batch is not the core going away.
const FAILURE_TOLERANCE: usize = 5;

/// Handle to the polling task. Cancels on drop, as well as through
/// [`EventLoopHandle::stop`].
pub struct EventLoopHandle {
    task: JoinHandle<()>,
}

impl EventLoopHandle {
    /// Cancel the polling task.
    pub fn stop(self) {
        self.task.abort();
    }
}

impl Drop for EventLoopHandle {
    fn drop(&mut self) {
        // Dropping a `JoinHandle` on its own detaches the task, which would
        // leave it polling a server nobody is listening to.
        self.task.abort();
    }
}

/// Poll `get_next_event_batch` in a loop, pushing each event to the returned
/// channel until the transport closes or the handle is stopped.
///
/// The core has no unsolicited notifications: the call blocks server-side
/// until an event is available, so this is an ordinary RPC call repeated.
pub fn spawn_event_loop(
    client: Arc<RpcClient>,
) -> (mpsc::UnboundedReceiver<CoreEvent>, EventLoopHandle) {
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut failures = 0_usize;
        loop {
            match client
                .call_polling::<Vec<CoreEvent>>("get_next_event_batch")
                .await
            {
                Ok(batch) => {
                    failures = 0;
                    for event in batch {
                        if tx.send(event).is_err() {
                            return;
                        }
                    }
                }
                // The server is gone; nothing to poll.
                Err(RpcError::TransportClosed) => return,
                // Anything else is one bad answer, not a dead core: an
                // event shape we could not read, or a call the core
                // refused. Ending here would stop every event for the life
                // of the process, and the shim reads the stream ending as
                // the server having died -- so the app would say the core
                // was gone while it was still running.
                Err(_) => {
                    failures += 1;
                    if failures >= FAILURE_TOLERANCE {
                        return;
                    }
                }
            }
        }
    });
    (rx, EventLoopHandle { task })
}
