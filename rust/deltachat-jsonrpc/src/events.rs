use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::client::RpcClient;

/// One event as delivered by `get_next_event_batch`.
///
/// `event` is left as a raw [`serde_json::Value`] rather than a hand-typed
/// enum of every `EventType` variant upstream defines (there are several
/// dozen, see `deltachat-jsonrpc/src/api/types/events.rs` in `chatmail/core`
/// -- tagged on the `"kind"` field). Duplicating that whole schema by hand
/// here would be exactly the kind of protocol-logic reimplementation the
/// project scope rules out; callers that need typed access to specific
/// event kinds should match on `event["kind"]` for the events they actually
/// care about, or this crate can grow generated bindings from the core's
/// `--openrpc` output later if that becomes worthwhile.
#[derive(Debug, Clone, Deserialize)]
pub struct CoreEvent {
    /// Which account (the core calls it a "context") the event belongs to.
    #[serde(rename = "contextId")]
    pub context_id: u32,
    /// The event object itself, tagged by its `kind` field.
    pub event: serde_json::Value,
}

/// Handle to a background task that long-polls `get_next_event_batch` and
/// forwards results to a channel. Drop or call [`EventLoopHandle::stop`] to
/// cancel it.
pub struct EventLoopHandle {
    task: JoinHandle<()>,
}

impl EventLoopHandle {
    /// Cancel the polling task.
    pub fn stop(self) {
        self.task.abort();
    }
}

/// Start a background task that repeatedly calls `get_next_event_batch` and
/// pushes each event to the returned channel, in order, until the transport
/// closes or the handle is stopped.
///
/// The core doesn't push unsolicited notifications for events; clients are
/// expected to poll `get_next_event`/`get_next_event_batch` in a loop (the
/// call blocks server-side until an event is available), so this is just an
/// ordinary RPC call issued repeatedly, not a special transport mode.
pub fn spawn_event_loop(
    client: Arc<RpcClient>,
) -> (mpsc::UnboundedReceiver<CoreEvent>, EventLoopHandle) {
    let (tx, rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            match client
                .call_unit::<Vec<CoreEvent>>("get_next_event_batch")
                .await
            {
                Ok(batch) => {
                    for event in batch {
                        if tx.send(event).is_err() {
                            return;
                        }
                    }
                }
                Err(_) => return,
            }
        }
    });
    (rx, EventLoopHandle { task })
}
