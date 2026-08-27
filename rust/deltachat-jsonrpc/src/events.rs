use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::client::RpcClient;

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

/// Handle to the polling task. Drop or [`EventLoopHandle::stop`] to cancel.
pub struct EventLoopHandle {
    task: JoinHandle<()>,
}

impl EventLoopHandle {
    /// Cancel the polling task.
    pub fn stop(self) {
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
