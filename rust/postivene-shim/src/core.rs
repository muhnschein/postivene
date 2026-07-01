use std::sync::Arc;

use deltachat_jsonrpc::{spawn_event_loop, CoreEvent, RpcClient};
use qmetaobject::*;
use tokio::runtime::Runtime;

/// `DeltaChatCore` is the one QObject that owns the connection to a spawned
/// `deltachat-rpc-server`: it starts the process, keeps the tokio runtime
/// that drives it alive, forwards the core's event stream to QML as a
/// signal, and exposes a handful of fire-and-forget methods (each paired
/// with a result signal) for the operations the minimal UI needs.
///
/// This mirrors the scope's architecture note: "core events run off the
/// main thread and are marshalled to the Qt main thread via queued
/// signals" -- every async completion here goes through
/// [`qmetaobject::queued_callback`] so it lands back on the Qt thread
/// before touching any `qt_property`/`qt_signal`.
///
/// It deliberately exposes only a thin slice of the core's ~100 JSON-RPC
/// methods (enough for account bootstrap + single-conversation send/health
/// check); more methods should be added the same way as the UI grows,
/// rather than trying to cover the whole API up front.
#[derive(QObject, Default)]
pub struct DeltaChatCore {
    base: qt_base_class!(trait QObject),

    /// One of: "idle", "starting", "ready", or `"error: ..."`.
    pub status: qt_property!(QString; NOTIFY status_changed),
    pub status_changed: qt_signal!(),

    pub system_info: qt_property!(QString; NOTIFY system_info_changed),
    pub system_info_changed: qt_signal!(),

    /// Raw core event, forwarded as-is: `kind` is the event's `"kind"`
    /// tag (e.g. `"IncomingMsg"`, `"ConfigureProgress"`) and `payload_json`
    /// is the full event object serialized as JSON, so QML can pick out
    /// whatever fields a given event kind carries without this crate
    /// needing a hand-typed struct for every one of the ~40 event kinds.
    pub core_event: qt_signal!(context_id: u32, kind: QString, payload_json: QString),

    pub account_added: qt_signal!(account_id: u32),
    pub account_error: qt_signal!(message: QString),

    pub configure_done: qt_signal!(account_id: u32, success: bool, error: QString),

    pub message_sent: qt_signal!(account_id: u32, chat_id: u32, message_id: u32),
    pub send_error: qt_signal!(message: QString),

    /// Spawn `rpc_server_path` (typically the bundled
    /// `deltachat-rpc-server` binary) and start draining its event stream.
    /// No-op if already started.
    pub start: qt_method!(fn(&mut self, rpc_server_path: QString)),

    /// Trigger a `get_system_info` round trip; result lands in
    /// `systemInfo`/`status`.
    pub check_health: qt_method!(fn(&mut self)),

    /// Create a new (unconfigured) account; result via `accountAdded`/
    /// `accountError`.
    pub add_account: qt_method!(fn(&mut self)),

    /// Set `addr`/`mail_pw` and run `configure`; result via
    /// `configureDone`. Progress is observable through `coreEvent`
    /// (`kind == "ConfigureProgress"`).
    pub configure_account:
        qt_method!(fn(&mut self, account_id: u32, addr: QString, password: QString)),

    /// Send a plain-text message; result via `messageSent`/`sendError`.
    pub send_text: qt_method!(fn(&mut self, account_id: u32, chat_id: u32, text: QString)),

    rpc: Option<Arc<RpcClient>>,
    runtime: Option<Arc<Runtime>>,
}

impl DeltaChatCore {
    pub fn start(&mut self, rpc_server_path: QString) {
        if self.runtime.is_some() {
            return;
        }

        let runtime = match Runtime::new() {
            Ok(rt) => Arc::new(rt),
            Err(err) => {
                self.status = format!("error: failed to start async runtime: {err}").into();
                self.status_changed();
                return;
            }
        };

        self.status = QString::from("starting");
        self.status_changed();

        let path = rpc_server_path.to_string();
        let rt_for_spawn = runtime.clone();
        self.runtime = Some(runtime);

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let started = queued_callback(move |result: Result<Arc<RpcClient>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(rpc) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.rpc = Some(rpc);
                        this_mut.status = QString::from("ready");
                    }
                    this.borrow().status_changed();
                    Self::forward_events(
                        ptr.clone(),
                        this.borrow().rpc.clone(),
                        this.borrow().runtime.clone(),
                    );
                }
                Err(err) => {
                    this.borrow_mut().status = format!("error: {err}").into();
                    this.borrow().status_changed();
                }
            }
        });

        rt_for_spawn.spawn(async move {
            let result = RpcClient::spawn(path, Vec::<&str>::new())
                .await
                .map(Arc::new)
                .map_err(|err| err.to_string());
            started(result);
        });
    }

    /// Start draining the core's event stream and forwarding each event to
    /// the `coreEvent` signal via a queued callback. Runs for as long as
    /// the transport lives; `spawn_event_loop` itself must be called from
    /// *within* a task already running on `runtime` (its internal
    /// `tokio::spawn` needs ambient runtime context), which is why this is
    /// one `runtime.spawn` wrapping both the event-loop setup and the
    /// forwarding loop, rather than calling `spawn_event_loop` directly
    /// from the Qt thread.
    fn forward_events(
        ptr: QPointer<Self>,
        rpc: Option<Arc<RpcClient>>,
        runtime: Option<Arc<Runtime>>,
    ) {
        let (Some(rpc), Some(runtime)) = (rpc, runtime) else {
            return;
        };
        let emit = queued_callback(move |event: CoreEvent| {
            if let Some(this) = ptr.as_pinned() {
                let kind = event
                    .event
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let payload = serde_json::to_string(&event.event).unwrap_or_default();
                this.borrow()
                    .core_event(event.context_id, kind.into(), payload.into());
            }
        });
        runtime.spawn(async move {
            let (mut events, _handle) = spawn_event_loop(rpc);
            while let Some(event) = events.recv().await {
                emit(event);
            }
        });
    }

    pub fn check_health(&mut self) {
        let Some(rpc) = self.rpc.clone() else {
            self.status = QString::from("error: not started");
            self.status_changed();
            return;
        };
        let Some(runtime) = self.runtime.clone() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<String, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(info) => {
                    this.borrow_mut().system_info = info.into();
                    this.borrow().system_info_changed();
                }
                Err(err) => {
                    this.borrow_mut().status = format!("error: {err}").into();
                    this.borrow().status_changed();
                }
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call_unit::<std::collections::BTreeMap<String, String>>("get_system_info")
                .await
                .map(|info| serde_json::to_string(&info).unwrap_or_default())
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    pub fn add_account(&mut self) {
        let Some(rpc) = self.rpc.clone() else {
            self.account_error(QString::from("not started"));
            return;
        };
        let Some(runtime) = self.runtime.clone() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<u32, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(account_id) => this.borrow().account_added(account_id),
                Err(err) => this.borrow().account_error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call_unit::<u32>("add_account")
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    pub fn configure_account(&mut self, account_id: u32, addr: QString, password: QString) {
        let Some(rpc) = self.rpc.clone() else {
            self.configure_done(account_id, false, QString::from("not started"));
            return;
        };
        let Some(runtime) = self.runtime.clone() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: (u32, Result<(), String>)| {
            let Some(this) = ptr.as_pinned() else { return };
            let (account_id, result) = result;
            match result {
                Ok(()) => this
                    .borrow()
                    .configure_done(account_id, true, QString::default()),
                Err(err) => this.borrow().configure_done(account_id, false, err.into()),
            }
        });

        let addr = addr.to_string();
        let password = password.to_string();
        runtime.spawn(async move {
            let result: Result<(), String> = async {
                rpc.call::<_, ()>("set_config", (account_id, "addr", Some(addr)))
                    .await
                    .map_err(|err| err.to_string())?;
                rpc.call::<_, ()>("set_config", (account_id, "mail_pw", Some(password)))
                    .await
                    .map_err(|err| err.to_string())?;
                rpc.call::<_, ()>("configure", (account_id,))
                    .await
                    .map_err(|err| err.to_string())?;
                rpc.call::<_, ()>("start_io", (account_id,))
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(())
            }
            .await;
            done((account_id, result));
        });
    }

    pub fn send_text(&mut self, account_id: u32, chat_id: u32, text: QString) {
        let Some(rpc) = self.rpc.clone() else {
            self.send_error(QString::from("not started"));
            return;
        };
        let Some(runtime) = self.runtime.clone() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: (u32, u32, Result<u32, String>)| {
            let Some(this) = ptr.as_pinned() else { return };
            let (account_id, chat_id, result) = result;
            match result {
                Ok(message_id) => this.borrow().message_sent(account_id, chat_id, message_id),
                Err(err) => this.borrow().send_error(err.into()),
            }
        });

        let text = text.to_string();
        runtime.spawn(async move {
            // `misc_send_msg` params, positional: account_id, chat_id, text,
            // file, filename, location, quoted_message_id.
            let result = rpc
                .call::<_, (u32, serde_json::Value)>(
                    "misc_send_msg",
                    (
                        account_id,
                        chat_id,
                        Some(text),
                        Option::<String>::None,
                        Option::<String>::None,
                        Option::<(f64, f64)>::None,
                        Option::<u32>::None,
                    ),
                )
                .await
                .map(|(message_id, _message)| message_id)
                .map_err(|err| err.to_string());
            done((account_id, chat_id, result));
        });
    }
}
