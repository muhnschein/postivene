use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use deltachat_jsonrpc::{spawn_event_loop, CoreEvent, RpcClient};
use qmetaobject::*;

use crate::models::{AccountItem, AccountListModel};
use crate::runtime::CoreRuntime;

/// The one live connection to the spawned server, shared with the models
/// QML instantiates per chat. There is exactly one per process:
/// [`DeltaChatCore::start`] refuses a second.
static CONNECTION: Mutex<Option<(Arc<RpcClient>, CoreRuntime)>> = Mutex::new(None);

/// The transport and runtime, once [`DeltaChatCore::start`] has completed.
pub(crate) fn connection() -> Option<(Arc<RpcClient>, CoreRuntime)> {
    CONNECTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn set_connection(value: Option<(Arc<RpcClient>, CoreRuntime)>) {
    *CONNECTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = value;
}

/// Default chatmail server, as the `dcaccount:` payload
/// `add_transport_from_qr` takes. See `docs/ONBOARDING.md`.
pub const DEFAULT_PROVIDER_QR: &str = "dcaccount:nine.testrun.org";

/// The one `QObject` owning the connection to a spawned
/// `deltachat-rpc-server`.
///
/// Methods are fire-and-forget, each paired with a result signal. Every
/// async completion goes through [`qmetaobject::queued_callback`] so it
/// lands on the Qt thread before touching a `qt_property` or `qt_signal`.
///
/// Only a slice of the core's ~100 JSON-RPC methods is exposed; add more
/// the same way as the UI needs them.
#[derive(QObject, Default)]
pub struct DeltaChatCore {
    base: qt_base_class!(trait QObject),

    /// One of: "idle", "starting", "ready", "stopped" (the server died),
    /// or `"error: ..."`.
    pub status: qt_property!(QString; NOTIFY status_changed),
    /// Emitted whenever [`DeltaChatCore::status`] changes.
    pub status_changed: qt_signal!(),

    /// The core's `get_system_info` answer, as raw JSON. Empty until
    /// [`DeltaChatCore::check_health`] has completed once.
    pub system_info: qt_property!(QString; NOTIFY system_info_changed),
    /// Emitted when [`DeltaChatCore::system_info`] is refreshed.
    pub system_info_changed: qt_signal!(),

    /// Raw core event: `kind` is the event's tag, `payload_json` the whole
    /// event as JSON. Untyped so QML can read any of the ~40 event kinds.
    pub core_event: qt_signal!(context_id: u32, kind: QString, payload_json: QString),

    /// A new, still unconfigured account was created.
    pub account_added: qt_signal!(account_id: u32),
    /// An account-scoped call (create, list, resume) failed.
    pub account_error: qt_signal!(message: QString),

    /// Configuration of `account_id` finished, successfully or not.
    pub configure_done: qt_signal!(account_id: u32, success: bool, error: QString),

    /// Spawn `rpc_server_path` and drain its event stream. No-op if
    /// already started.
    pub start: qt_method!(fn(&mut self, rpc_server_path: QString)),

    /// `get_system_info` round trip; result lands in `system_info`.
    pub check_health: qt_method!(fn(&mut self)),

    /// Create a new unconfigured account.
    pub add_account: qt_method!(fn(&mut self)),

    /// Legacy path: `set_config` + `configure`. Prefer
    /// `create_profile_with_email` (`docs/ONBOARDING.md`).
    pub configure_account:
        qt_method!(fn(&mut self, account_id: u32, addr: QString, password: QString)),

    /// All accounts known to the core.
    pub account_list: qt_property!(RefCell<AccountListModel>; CONST),

    /// Repopulate `account_list`. QML uses the `accounts_refreshed` result
    /// at startup to choose between onboarding and resuming an account.
    pub refresh_accounts: qt_method!(fn(&mut self)),
    /// [`DeltaChatCore::account_list`] was repopulated. `configured_count`
    /// is how many accounts are usable, and `first_configured_id` is the
    /// one to resume (0 when there is none).
    pub accounts_refreshed: qt_signal!(configured_count: u32, first_configured_id: u32),

    /// Resume IO for an already-configured account.
    pub start_account_io: qt_method!(fn(&mut self, account_id: u32)),
    /// Result of resuming IO for an already-configured account.
    pub io_started: qt_signal!(account_id: u32, success: bool, error: QString),

    /// The default chatmail server's `dcaccount:` payload.
    pub default_provider_qr: qt_method!(fn(&mut self) -> QString),

    /// Create a profile on a chatmail server: the core mints the address
    /// and credentials. Result via `profile_created`/`profile_error`.
    pub create_profile: qt_method!(fn(&mut self, display_name: QString, provider_qr: QString)),

    /// Configure an existing mailbox as this profile's transport.
    pub create_profile_with_email:
        qt_method!(fn(&mut self, display_name: QString, addr: QString, password: QString)),

    /// A profile is ready to use; `account_id` has a working transport.
    pub profile_created: qt_signal!(account_id: u32),
    /// Creating a profile failed. The message is the core's own.
    pub profile_error: qt_signal!(message: QString),

    /// Configuration progress for `account_id`, as the core reports it:
    /// 0 means failure, 1..=999 is permille, 1000 means done.
    pub configure_progress: qt_signal!(account_id: u32, permille: u32),

    /// Abort a running configure. Takes no account id: onboarding has none
    /// to give, so the unconfigured account is found here.
    pub cancel_ongoing: qt_method!(fn(&mut self)),

    /// `check_qr` for onboarding, where there is no account id to pass
    /// yet: resolves the profile account first.
    pub check_invite: qt_method!(fn(&mut self, qr_content: QString)),

    /// The account's email transports, as a JSON array of upstream
    /// `EnteredLoginParam`.
    pub list_transports: qt_method!(fn(&mut self, account_id: u32)),
    /// The account's transports, as a raw JSON array.
    pub transports_listed: qt_signal!(account_id: u32, transports_json: QString),

    /// Classify a QR payload. `qr_checked` carries the upstream `Qr`
    /// object as JSON: `kind` is camelCase, its fields `snake_case`.
    pub check_qr: qt_method!(fn(&mut self, account_id: u32, qr_content: QString)),
    /// A QR/invite payload was classified by the core.
    pub qr_checked: qt_signal!(account_id: u32, kind: QString, payload_json: QString),
    /// Classifying a QR/invite payload failed.
    pub qr_error: qt_signal!(message: QString),

    /// The core reported a failure of its own -- an `Error` event, which
    /// carries a message meant for the user. Typed so a page need not
    /// parse the event payload.
    pub core_error: qt_signal!(message: QString),

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

        // Built on its own thread; see `crate::runtime`.
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
                        this_mut.rpc = Some(rpc.clone());
                        this_mut.status = QString::from("ready");
                        set_connection(this_mut.runtime.clone().map(|rt| (rpc, rt)));
                    }
                    this.borrow().status_changed();
                    Self::forward_events(
                        ptr.clone(),
                        this.borrow().rpc.clone(),
                        this.borrow().runtime.clone(),
                    );
                }
                Err(err) => {
                    // Drop the runtime so a later `start()` is not blocked
                    // by the already-started guard.
                    let mut this_mut = this.borrow_mut();
                    this_mut.runtime = None;
                    set_connection(None);
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

    /// Where the core keeps account state: `POSTIVENE_ACCOUNTS_DIR`, else
    /// the XDG data dir. Without it the core would use `./accounts`,
    /// relative to whatever directory the app was launched from.
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

    /// Forward the core's event stream to `core_event` via queued
    /// callbacks, for as long as the transport lives.
    ///
    /// The setup and the loop share one `runtime.spawn` because
    /// `spawn_event_loop`'s internal `tokio::spawn` needs ambient runtime
    /// context.
    fn forward_events(
        ptr: QPointer<Self>,
        rpc: Option<Arc<RpcClient>>,
        runtime: Option<CoreRuntime>,
    ) {
        let (Some(rpc), Some(runtime)) = (rpc, runtime) else {
            return;
        };
        // The stream ends when the server dies. Nothing restarts it yet,
        // but `status` has to stop claiming the core is there.
        let stopped_ptr = ptr.clone();
        let stopped = queued_callback(move |()| {
            let Some(this) = stopped_ptr.as_pinned() else {
                return;
            };
            set_connection(None);
            {
                let mut this_mut = this.borrow_mut();
                this_mut.rpc = None;
                // Dropped for the same reason the failed-spawn path drops
                // it: `start` refuses to run while a runtime is here, so
                // leaving one behind makes a later restart a silent no-op.
                this_mut.runtime = None;
                this_mut.status = QString::from("stopped");
            }
            this.borrow().status_changed();
        });
        let emit = queued_callback(move |event: CoreEvent| {
            if let Some(this) = ptr.as_pinned() {
                let kind = event
                    .event
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Unknown")
                    .to_string();
                let payload = serde_json::to_string(&event.event).unwrap_or_default();
                // A typed signal too, so a progress bar need not parse
                // JSON. Both fire for these events.
                if kind == "Error" {
                    let text = event
                        .event
                        .get("msg")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("the core reported an error");
                    this.borrow().core_error(text.into());
                }
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
            stopped(());
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
                    // Saturating: "very many" is the right answer for a
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
            // Upstream `Account`: `kind` is "Configured"/"Unconfigured",
            // fields camelCase.
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
                // The core asks the server for an account, stores the
                // credentials, and restarts IO. Not `configure`, which
                // upstream deprecated (docs/ONBOARDING.md).
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
                // `addr` and `password` only; the rest of
                // EnteredLoginParam autoconfigures (docs/ONBOARDING.md).
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
            // The configuring account is the unconfigured one. Never
            // creates one, unlike `profile_account`.
            let Ok(Some(account_id)) = Self::find_unconfigured(&rpc).await else {
                return;
            };
            // Fire and forget: the UI reacts to the core's final
            // ConfigureProgress(0), not to this call's return.
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

    /// The account a new profile is built on: an existing unconfigured one
    /// if there is one, else a fresh one. Reuse keeps a failed signup from
    /// stranding an account per retry.
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

    /// Set the display name, before the transport call so it is in place
    /// when the core announces the account.
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

    /// The transport and runtime, once [`DeltaChatCore::start`] completed.
    /// Callers report `None` on their own error signal.
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
