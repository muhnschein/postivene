use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use deltachat_jsonrpc::{spawn_event_loop, CoreEvent, RpcClient};
use qmetaobject::*;

use crate::models::{
    AccountItem, AccountListModel, ChatListItem, ChatListModel, MessageListItem, MessageListModel,
};
use crate::runtime::CoreRuntime;

/// The chatmail server a new profile is created on unless the user picks
/// another one. Expressed the way the core wants it -- a `dcaccount:` QR
/// payload -- because that is the single input `add_transport_from_qr`
/// takes, whether it came from this default, a scanned code, or a pasted
/// invite link. Matches the reference client's `DEFAULT_CHATMAIL_HOST`;
/// see `docs/ONBOARDING.md`.
pub const DEFAULT_PROVIDER_QR: &str = "dcaccount:nine.testrun.org";

/// `DeltaChatCore` is the one `QObject` that owns the connection to a spawned
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
    /// Emitted whenever [`DeltaChatCore::status`] changes.
    pub status_changed: qt_signal!(),

    /// The core's `get_system_info` answer, as raw JSON. Empty until
    /// [`DeltaChatCore::check_health`] has completed once.
    pub system_info: qt_property!(QString; NOTIFY system_info_changed),
    /// Emitted when [`DeltaChatCore::system_info`] is refreshed.
    pub system_info_changed: qt_signal!(),

    /// Raw core event, forwarded as-is: `kind` is the event's `"kind"`
    /// tag (e.g. `"IncomingMsg"`, `"ConfigureProgress"`) and `payload_json`
    /// is the full event object serialized as JSON, so QML can pick out
    /// whatever fields a given event kind carries without this crate
    /// needing a hand-typed struct for every one of the ~40 event kinds.
    pub core_event: qt_signal!(context_id: u32, kind: QString, payload_json: QString),

    /// A new, still unconfigured account was created.
    pub account_added: qt_signal!(account_id: u32),
    /// An account-scoped call (create, list, resume) failed.
    pub account_error: qt_signal!(message: QString),

    /// Configuration of `account_id` finished, successfully or not.
    pub configure_done: qt_signal!(account_id: u32, success: bool, error: QString),

    /// A message was accepted by the core and appended to
    /// [`DeltaChatCore::message_list`].
    pub message_sent: qt_signal!(account_id: u32, chat_id: u32, message_id: u32),
    /// Sending a message failed; nothing was appended.
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

    /// Backing model for a chat list `SilicaListView`. Repopulated by
    /// `refresh_chat_list`.
    pub chat_list: qt_property!(RefCell<ChatListModel>; CONST),

    /// Backing model for a conversation `SilicaListView`. Repopulated by
    /// `open_chat`, appended to by a successful `send_text`.
    pub message_list: qt_property!(RefCell<MessageListModel>; CONST),

    /// Repopulate `chatList` from the core; result observable via
    /// `chatList`'s own change notifications, errors via
    /// `chatListError`.
    pub refresh_chat_list: qt_method!(fn(&mut self, account_id: u32)),

    /// Repopulate `messageList` with `chat_id`'s messages; errors via
    /// `messageListError`.
    pub open_chat: qt_method!(fn(&mut self, account_id: u32, chat_id: u32)),

    /// Repopulating [`DeltaChatCore::chat_list`] failed; the model still
    /// holds whatever it held before.
    pub chat_list_error: qt_signal!(message: QString),
    /// Repopulating [`DeltaChatCore::message_list`] failed; the model
    /// still holds whatever it held before.
    pub message_list_error: qt_signal!(message: QString),

    /// All accounts known to the core. Repopulated by `refresh_accounts`.
    pub account_list: qt_property!(RefCell<AccountListModel>; CONST),

    /// Repopulate `accountList`; completion via `accountsRefreshed`
    /// (errors via `accountError`). QML uses this at startup to decide
    /// between the setup wizard and the chat list: if
    /// `configured_count > 0`, `first_configured_id` is the account to
    /// resume (call `start_account_io` and go straight to chats).
    pub refresh_accounts: qt_method!(fn(&mut self)),
    /// [`DeltaChatCore::account_list`] was repopulated. `configured_count`
    /// is how many accounts are usable, and `first_configured_id` is the
    /// one to resume (0 when there is none).
    pub accounts_refreshed: qt_signal!(configured_count: u32, first_configured_id: u32),

    /// Resume IO for an already-configured account (app start / account
    /// switch); result via `ioStarted`.
    pub start_account_io: qt_method!(fn(&mut self, account_id: u32)),
    /// Result of resuming IO for an already-configured account.
    pub io_started: qt_signal!(account_id: u32, success: bool, error: QString),

    /// The `dcaccount:` payload for the default chatmail server, so QML
    /// does not have to hardcode a hostname.
    pub default_provider_qr: qt_method!(fn(&mut self) -> QString),

    /// Create a profile on a chatmail server: set `display_name` on a
    /// fresh (or reused unconfigured) account, then hand `provider_qr` to
    /// the core, which mints an address and credentials there. Result via
    /// `profile_created`/`profile_error`, progress via
    /// `configure_progress`.
    pub create_profile: qt_method!(fn(&mut self, display_name: QString, provider_qr: QString)),

    /// The classic path: configure an existing mailbox as this profile's
    /// transport. Same result signals as `create_profile`.
    pub create_profile_with_email:
        qt_method!(fn(&mut self, display_name: QString, addr: QString, password: QString)),

    /// A profile is ready to use; `account_id` has a working transport.
    pub profile_created: qt_signal!(account_id: u32),
    /// Creating a profile failed. The message is the core's own.
    pub profile_error: qt_signal!(message: QString),

    /// Configuration progress for `account_id`, as the core reports it:
    /// 0 means failure, 1..=999 is permille, 1000 means done.
    pub configure_progress: qt_signal!(account_id: u32, permille: u32),

    /// Abort a running configure. Takes no account id because onboarding
    /// has none to give: the account being configured is the one
    /// unconfigured account, and the shim finds it. The core answers with a
    /// final `ConfigureProgress` of 0.
    pub cancel_ongoing: qt_method!(fn(&mut self)),

    /// Classify a pasted invite or login link during onboarding, when
    /// there is no account id to pass yet. Resolves (or creates) the
    /// profile account first, then answers on `qr_checked`/`qr_error` like
    /// `check_qr`.
    pub check_invite: qt_method!(fn(&mut self, qr_content: QString)),

    /// List the email transports configured on an account, as raw JSON
    /// (an array of upstream `EnteredLoginParam`). Result via
    /// `transports_listed`.
    pub list_transports: qt_method!(fn(&mut self, account_id: u32)),
    /// The account's transports, as a raw JSON array.
    pub transports_listed: qt_signal!(account_id: u32, transports_json: QString),

    /// Parse/classify a QR code's payload via the core. Result via
    /// `qrChecked` with the upstream `Qr` object as raw JSON: `kind` is
    /// camelCase (e.g. "account", "askVerifyContact", "login"), while the
    /// payload's *fields* are `snake_case` (upstream's serde `rename_all` sits
    /// at the enum level, exactly like `MessageListItem`'s).
    pub check_qr: qt_method!(fn(&mut self, account_id: u32, qr_content: QString)),
    /// A QR/invite payload was classified by the core.
    pub qr_checked: qt_signal!(account_id: u32, kind: QString, payload_json: QString),
    /// Classifying a QR/invite payload failed.
    pub qr_error: qt_signal!(message: QString),

    rpc: Option<Arc<RpcClient>>,
    runtime: Option<CoreRuntime>,
}

impl DeltaChatCore {
    /// Spawn the server and begin draining its event stream. No-op if
    /// already started. See the `start` declaration above.
    pub fn start(&mut self, rpc_server_path: QString) {
        if self.runtime.is_some() {
            return;
        }

        // Built on its own thread -- see `crate::runtime` for why the Qt
        // main thread must not build (or drop) the runtime itself.
        let runtime = match CoreRuntime::new() {
            Ok(runtime) => runtime,
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
                    // Drop the runtime so a later `start()` (e.g. after the
                    // user fixes the server path) isn't blocked by the
                    // is-already-started guard.
                    let mut this_mut = this.borrow_mut();
                    this_mut.runtime = None;
                    this_mut.status = format!("error: {err}").into();
                    drop(this_mut);
                    this.borrow().status_changed();
                }
            }
        });

        rt_for_spawn.spawn(async move {
            let result = async {
                let accounts_dir = Self::accounts_dir()?;
                RpcClient::spawn_with_env(
                    path,
                    Vec::<&str>::new(),
                    [("DC_ACCOUNTS_PATH", accounts_dir)],
                )
                .await
                .map(Arc::new)
                .map_err(|err| err.to_string())
            }
            .await;
            started(result);
        });
    }

    /// Where the core keeps all account state. Without this,
    /// `deltachat-rpc-server` defaults to `./accounts` relative to whatever
    /// working directory the app happened to be launched from -- account
    /// data would land in a different place per launch context.
    /// `POSTIVENE_ACCOUNTS_DIR` overrides; otherwise the XDG data dir.
    fn accounts_dir() -> Result<String, String> {
        let dir = if let Ok(dir) = std::env::var("POSTIVENE_ACCOUNTS_DIR") {
            std::path::PathBuf::from(dir)
        } else {
            let base = std::env::var("XDG_DATA_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|_| {
                    std::env::var("HOME")
                        .map(|home| std::path::PathBuf::from(home).join(".local/share"))
                })
                .map_err(|_| "neither XDG_DATA_HOME nor HOME is set".to_string())?;
            base.join("postivene/accounts")
        };
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("cannot create accounts dir {}: {err}", dir.display()))?;
        Ok(dir.to_string_lossy().into_owned())
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
        runtime: Option<CoreRuntime>,
    ) {
        let (Some(rpc), Some(runtime)) = (rpc, runtime) else {
            return;
        };
        let emit = queued_callback(move |event: CoreEvent| {
            if let Some(this) = ptr.as_pinned() {
                let kind = event
                    .event
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Unknown")
                    .to_string();
                let payload = serde_json::to_string(&event.event).unwrap_or_default();
                // Configuration progress gets a typed signal as well as the
                // raw event: it drives a progress bar during onboarding, and
                // a UI should not have to parse JSON to move one. Both fire
                // for these events -- listeners of `core_event` still see it.
                if kind == "ConfigureProgress" {
                    if let Some(permille) = event
                        .event
                        .get("progress")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                    {
                        this.borrow().configure_progress(event.context_id, permille);
                    }
                }
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

    /// Run a `get_system_info` round trip into
    /// [`DeltaChatCore::system_info`].
    pub fn check_health(&mut self) {
        let Some((rpc, runtime)) = self.connection() else {
            self.status = QString::from("error: not started");
            self.status_changed();
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

    /// Create a new, unconfigured account.
    pub fn add_account(&mut self) {
        let Some((rpc, runtime)) = self.connection() else {
            self.account_error(QString::from("not started"));
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

    /// Configure `account_id` from an address and password.
    pub fn configure_account(&mut self, account_id: u32, addr: QString, password: QString) {
        let Some((rpc, runtime)) = self.connection() else {
            self.configure_done(account_id, false, QString::from("not started"));
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

    /// Send a plain-text message to `chat_id`.
    pub fn send_text(&mut self, account_id: u32, chat_id: u32, text: QString) {
        let Some((rpc, runtime)) = self.connection() else {
            self.send_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(
            move |result: (u32, u32, Result<(u32, String, i64, bool, u32), String>)| {
                let Some(this) = ptr.as_pinned() else { return };
                let (account_id, chat_id, result) = result;
                match result {
                    Ok((message_id, text, timestamp, show_padlock, state)) => {
                        this.borrow_mut()
                            .message_list
                            .borrow_mut()
                            .push(MessageListItem {
                                message_id,
                                text: text.into(),
                                is_outgoing: true,
                                timestamp,
                                show_padlock,
                                state,
                            });
                        this.borrow().message_sent(account_id, chat_id, message_id);
                    }
                    Err(err) => this.borrow().send_error(err.into()),
                }
            },
        );

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
                .map(|(message_id, message)| {
                    let text = message
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let timestamp = message
                        .get("timestamp")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0);
                    let show_padlock = message
                        .get("showPadlock")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    // The core's DC_STATE_* constants are small; anything
                    // that does not fit is not a state we know, so it reads
                    // as "no state" rather than silently wrapping.
                    let state = message
                        .get("state")
                        .and_then(serde_json::Value::as_u64)
                        .and_then(|v| u32::try_from(v).ok())
                        .unwrap_or(0);
                    (message_id, text, timestamp, show_padlock, state)
                })
                .map_err(|err| err.to_string());
            done((account_id, chat_id, result));
        });
    }

    /// Repopulate [`DeltaChatCore::chat_list`] from the core.
    pub fn refresh_chat_list(&mut self, account_id: u32) {
        let Some((rpc, runtime)) = self.connection() else {
            self.chat_list_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<ChatListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(items) => this.borrow_mut().chat_list.borrow_mut().reset_data(items),
                Err(err) => this.borrow().chat_list_error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = Self::fetch_chat_list(&rpc, account_id)
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    async fn fetch_chat_list(
        rpc: &RpcClient,
        account_id: u32,
    ) -> Result<Vec<ChatListItem>, deltachat_jsonrpc::RpcError> {
        let entries: Vec<u32> = rpc
            .call(
                "get_chatlist_entries",
                (
                    account_id,
                    Option::<u32>::None,
                    Option::<String>::None,
                    Option::<u32>::None,
                ),
            )
            .await?;
        let items: HashMap<u32, serde_json::Value> = rpc
            .call(
                "get_chatlist_items_by_entries",
                (account_id, entries.clone()),
            )
            .await?;

        let mut result = Vec::with_capacity(entries.len());
        for chat_id in entries {
            let Some(item) = items.get(&chat_id) else {
                continue;
            };
            if item.get("kind").and_then(serde_json::Value::as_str) != Some("ChatListItem") {
                continue;
            }
            result.push(ChatListItem {
                chat_id,
                name: item
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                preview: item
                    .get("summaryText2")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                unread_count: item
                    .get("freshMessageCounter")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|v| u32::try_from(v).ok())
                    .unwrap_or(0),
                is_encrypted: item
                    .get("isEncrypted")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            });
        }
        Ok(result)
    }

    /// Repopulate [`DeltaChatCore::message_list`] with `chat_id`'s
    /// messages and clear the chat's fresh-message badge.
    pub fn open_chat(&mut self, account_id: u32, chat_id: u32) {
        let Some((rpc, runtime)) = self.connection() else {
            self.message_list_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(items) => this
                    .borrow_mut()
                    .message_list
                    .borrow_mut()
                    .reset_data(items),
                Err(err) => this.borrow().message_list_error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = Self::fetch_messages(&rpc, account_id, chat_id)
                .await
                .map_err(|err| err.to_string());
            let fetched_ok = result.is_ok();
            done(result);
            if fetched_ok {
                // Opening a chat means the user has seen it: clear its
                // "fresh" badge. The core answers with a MsgsNoticed
                // event, which the chat list listens to for refreshing
                // its unread counts -- so no extra UI plumbing here.
                let _ = rpc
                    .call::<_, ()>("marknoticed_chat", (account_id, chat_id))
                    .await;
            }
        });
    }

    async fn fetch_messages(
        rpc: &RpcClient,
        account_id: u32,
        chat_id: u32,
    ) -> Result<Vec<MessageListItem>, deltachat_jsonrpc::RpcError> {
        let items: Vec<serde_json::Value> = rpc
            .call(
                "get_message_list_items",
                (account_id, chat_id, false, false),
            )
            .await?;

        let mut result = Vec::with_capacity(items.len());
        for item in items {
            if item.get("kind").and_then(serde_json::Value::as_str) != Some("message") {
                continue;
            }
            // Upstream's JsonrpcMessageListItem has serde's `rename_all`
            // only at the *enum* level, which renames variants ("message",
            // "dayMarker") but not their fields -- so this field really is
            // snake_case `msg_id` on the wire, unlike MessageObject's
            // camelCase fields. Verified by serializing the exact upstream
            // attribute shape. `msgId` is also accepted in case upstream
            // ever switches to `rename_all_fields`.
            let Some(message_id) = item
                .get("msg_id")
                .or_else(|| item.get("msgId"))
                .and_then(serde_json::Value::as_u64)
            else {
                continue;
            };
            // A message id that does not fit u32 is not one the core's own
            // API can address, so such a row is skipped rather than wrapped
            // into a different message's id.
            let Ok(message_id) = u32::try_from(message_id) else {
                continue;
            };
            let message: serde_json::Value =
                rpc.call("get_message", (account_id, message_id)).await?;
            result.push(MessageListItem {
                message_id,
                text: message
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .into(),
                // Contact id 1 is the well-known DC_CONTACT_ID_SELF.
                is_outgoing: message.get("fromId").and_then(serde_json::Value::as_u64) == Some(1),
                timestamp: message
                    .get("timestamp")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0),
                show_padlock: message
                    .get("showPadlock")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                state: message
                    .get("state")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|v| u32::try_from(v).ok())
                    .unwrap_or(0),
            });
        }
        Ok(result)
    }

    /// Repopulate [`DeltaChatCore::account_list`] from the core.
    pub fn refresh_accounts(&mut self) {
        let Some((rpc, runtime)) = self.connection() else {
            self.account_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<AccountItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(items) => {
                    // Saturating rather than wrapping: an account count
                    // that overflows u32 is impossible, and if it ever
                    // happened "very many" is the right answer for a
                    // has-any-configured-account check.
                    let configured_count =
                        u32::try_from(items.iter().filter(|item| item.is_configured).count())
                            .unwrap_or(u32::MAX);
                    let first_configured_id = items
                        .iter()
                        .find(|item| item.is_configured)
                        .map_or(0, |item| item.account_id);
                    this.borrow_mut()
                        .account_list
                        .borrow_mut()
                        .reset_data(items);
                    this.borrow()
                        .accounts_refreshed(configured_count, first_configured_id);
                }
                Err(err) => this.borrow().account_error(err.into()),
            }
        });

        runtime.spawn(async move {
            // Upstream `Account`: tagged `kind` ("Configured"/
            // "Unconfigured", verbatim variant names), camelCase fields
            // (per-variant rename_all).
            let result = rpc
                .call_unit::<Vec<serde_json::Value>>("get_all_accounts")
                .await
                .map(|accounts| {
                    accounts
                        .iter()
                        .filter_map(|account| {
                            let account_id = u32::try_from(account.get("id")?.as_u64()?).ok()?;
                            let is_configured =
                                account.get("kind").and_then(serde_json::Value::as_str)
                                    == Some("Configured");
                            Some(AccountItem {
                                account_id,
                                display_name: account
                                    .get("displayName")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .into(),
                                addr: account
                                    .get("addr")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .into(),
                                is_configured,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Resume IO for an already-configured account.
    pub fn start_account_io(&mut self, account_id: u32) {
        let Some((rpc, runtime)) = self.connection() else {
            self.io_started(account_id, false, QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: (u32, Result<(), String>)| {
            let Some(this) = ptr.as_pinned() else { return };
            let (account_id, result) = result;
            match result {
                Ok(()) => this
                    .borrow()
                    .io_started(account_id, true, QString::default()),
                Err(err) => this.borrow().io_started(account_id, false, err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("start_io", (account_id,))
                .await
                .map_err(|err| err.to_string());
            done((account_id, result));
        });
    }

    /// The `dcaccount:` payload for the default chatmail server.
    pub fn default_provider_qr(&mut self) -> QString {
        QString::from(DEFAULT_PROVIDER_QR)
    }

    /// Create a profile on a chatmail server from a `dcaccount:`/`dclogin:`
    /// payload.
    pub fn create_profile(&mut self, display_name: QString, provider_qr: QString) {
        let Some((rpc, runtime)) = self.connection() else {
            self.profile_error(QString::from("not started"));
            return;
        };
        let done = self.profile_callback();

        let display_name = display_name.to_string();
        let provider_qr = provider_qr.to_string();
        runtime.spawn(async move {
            let result = async {
                let account_id = Self::profile_account(&rpc).await?;
                Self::set_display_name(&rpc, account_id, display_name).await?;
                // One call does the whole thing: the core asks the chatmail
                // server for an account, stores the credentials it hands
                // back, and restarts IO around the change. `configure` is
                // deliberately not involved -- upstream deprecated it in
                // 2025-02 in favour of exactly this (docs/ONBOARDING.md).
                rpc.call::<_, ()>("add_transport_from_qr", (account_id, provider_qr))
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(account_id)
            }
            .await;
            done(result);
        });
    }

    /// Create a profile backed by an existing mailbox.
    pub fn create_profile_with_email(
        &mut self,
        display_name: QString,
        addr: QString,
        password: QString,
    ) {
        let Some((rpc, runtime)) = self.connection() else {
            self.profile_error(QString::from("not started"));
            return;
        };
        let done = self.profile_callback();

        let display_name = display_name.to_string();
        let addr = addr.to_string();
        let password = password.to_string();
        runtime.spawn(async move {
            let result = async {
                let account_id = Self::profile_account(&rpc).await?;
                Self::set_display_name(&rpc, account_id, display_name).await?;
                // `addr` and `password` only: every other field of upstream's
                // EnteredLoginParam is optional and autoconfigured. Sending
                // the pair through add_or_update_transport, rather than
                // set_config + configure, is what makes this the supported
                // path and gives the account a *transport* rather than a
                // single credential pair (docs/ONBOARDING.md).
                let param = serde_json::json!({ "addr": addr, "password": password });
                rpc.call::<_, ()>("add_or_update_transport", (account_id, param))
                    .await
                    .map_err(|err| err.to_string())?;
                Ok(account_id)
            }
            .await;
            done(result);
        });
    }

    /// Abort a running configure.
    pub fn cancel_ongoing(&mut self) {
        let Some((rpc, runtime)) = self.connection() else {
            return;
        };
        runtime.spawn(async move {
            // The account configuring is the unconfigured one; if there
            // isn't one, there is nothing to stop. Deliberately does not
            // fall back to creating an account the way `profile_account`
            // does -- cancelling must never make one.
            let Ok(Some(account_id)) = Self::find_unconfigured(&rpc).await else {
                return;
            };
            // Fire and forget: the outcome the UI reacts to is the final
            // ConfigureProgress(0) the core emits in response, not this
            // call's own return.
            let _ = rpc
                .call::<_, ()>("stop_ongoing_process", (account_id,))
                .await;
        });
    }

    /// Classify a pasted invite or login link during onboarding.
    pub fn check_invite(&mut self, qr_content: QString) {
        let Some((rpc, runtime)) = self.connection() else {
            self.qr_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: (u32, Result<serde_json::Value, String>)| {
            let Some(this) = ptr.as_pinned() else { return };
            let (account_id, result) = result;
            match result {
                Ok(qr) => {
                    let kind = qr
                        .get("kind")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown")
                        .to_string();
                    let payload = serde_json::to_string(&qr).unwrap_or_default();
                    this.borrow()
                        .qr_checked(account_id, kind.into(), payload.into());
                }
                Err(err) => this.borrow().qr_error(err.into()),
            }
        });

        let qr_content = qr_content.to_string();
        runtime.spawn(async move {
            let outcome = async {
                let account_id = Self::profile_account(&rpc).await?;
                let qr = rpc
                    .call::<_, serde_json::Value>("check_qr", (account_id, qr_content))
                    .await
                    .map_err(|err| err.to_string())?;
                Ok::<_, String>((account_id, qr))
            }
            .await;
            match outcome {
                Ok((account_id, qr)) => done((account_id, Ok(qr))),
                Err(err) => done((0, Err(err))),
            }
        });
    }

    /// List the account's email transports.
    pub fn list_transports(&mut self, account_id: u32) {
        let Some((rpc, runtime)) = self.connection() else {
            self.profile_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: (u32, Result<String, String>)| {
            let Some(this) = ptr.as_pinned() else { return };
            let (account_id, result) = result;
            match result {
                Ok(json) => this.borrow().transports_listed(account_id, json.into()),
                Err(err) => this.borrow().profile_error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, serde_json::Value>("list_transports", (account_id,))
                .await
                .map(|value| value.to_string())
                .map_err(|err| err.to_string());
            done((account_id, result));
        });
    }

    /// The shared completion path of both `create_profile*` methods.
    fn profile_callback(&self) -> impl Fn(Result<u32, String>) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |result: Result<u32, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(account_id) => this.borrow().profile_created(account_id),
                Err(err) => this.borrow().profile_error(err.into()),
            }
        })
    }

    /// The account a new profile is built on: an existing *unconfigured*
    /// one if a previous attempt left one behind, otherwise a fresh one.
    /// Reusing it is what keeps a failed signup from leaving an orphan
    /// account behind on every retry.
    async fn profile_account(rpc: &RpcClient) -> Result<u32, String> {
        if let Some(account_id) = Self::find_unconfigured(rpc).await? {
            return Ok(account_id);
        }
        rpc.call_unit::<u32>("add_account")
            .await
            .map_err(|err| err.to_string())
    }

    /// The first account the core reports as `Unconfigured`, if any.
    async fn find_unconfigured(rpc: &RpcClient) -> Result<Option<u32>, String> {
        let accounts: Vec<serde_json::Value> = rpc
            .call_unit("get_all_accounts")
            .await
            .map_err(|err| err.to_string())?;
        Ok(accounts.iter().find_map(|account| {
            if account.get("kind").and_then(serde_json::Value::as_str) != Some("Unconfigured") {
                return None;
            }
            account
                .get("id")
                .and_then(serde_json::Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())
        }))
    }

    /// Set the profile's display name. Sent before the transport call, so
    /// the name is already in place when the core announces the account.
    async fn set_display_name(
        rpc: &RpcClient,
        account_id: u32,
        display_name: String,
    ) -> Result<(), String> {
        rpc.call::<_, ()>(
            "set_config",
            (account_id, "displayname", Some(display_name)),
        )
        .await
        .map_err(|err| err.to_string())
    }

    /// The transport and the runtime, once [`DeltaChatCore::start`] has
    /// completed. Callers report the `None` case themselves, because each
    /// has its own error signal to report it on.
    fn connection(&self) -> Option<(Arc<RpcClient>, CoreRuntime)> {
        Some((self.rpc.clone()?, self.runtime.clone()?))
    }

    /// Classify a QR/invite payload via the core.
    pub fn check_qr(&mut self, account_id: u32, qr_content: QString) {
        let Some((rpc, runtime)) = self.connection() else {
            self.qr_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: (u32, Result<serde_json::Value, String>)| {
            let Some(this) = ptr.as_pinned() else { return };
            let (account_id, result) = result;
            match result {
                Ok(qr) => {
                    let kind = qr
                        .get("kind")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown")
                        .to_string();
                    let payload = serde_json::to_string(&qr).unwrap_or_default();
                    this.borrow()
                        .qr_checked(account_id, kind.into(), payload.into());
                }
                Err(err) => this.borrow().qr_error(err.into()),
            }
        });

        let qr_content = qr_content.to_string();
        runtime.spawn(async move {
            let result = rpc
                .call::<_, serde_json::Value>("check_qr", (account_id, qr_content))
                .await
                .map_err(|err| err.to_string());
            done((account_id, result));
        });
    }
}
