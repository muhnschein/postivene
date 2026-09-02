use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use deltachat_jsonrpc::{spawn_event_loop, CoreEvent, RpcClient};
use qmetaobject::*;

use crate::json;
use crate::models::{AccountItem, AccountListModel};
use crate::runtime::CoreRuntime;

/// The one live connection to the spawned server, shared with the models
/// QML instantiates per chat.
///
/// One per process in practice because the app makes one `DeltaChatCore`.
/// [`DeltaChatCore::start`]'s guard is per object, not global, so a second
/// instance would spawn a second server and take this over -- nothing
/// enforces the singleton, and nothing needs to yet.
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
/// `add_transport_from_qr` takes. See `docs/PROJECT.md`.
pub const DEFAULT_PROVIDER_QR: &str = "dcaccount:nine.testrun.org";

/// Where the RPM installs the server: beside the app, and not on `PATH`.
pub const BUNDLED_SERVER: &str = "/usr/libexec/harbour-postivene/deltachat-rpc-server";

/// How long the app waits for the server to go at exit.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Which server binary to run: `--rpc-server <path>` or
/// `--rpc-server=<path>` from `args`, else `POSTIVENE_RPC_SERVER` from
/// `env`, else [`BUNDLED_SERVER`].
///
/// Never a `PATH` lookup. The server is handed the mail password and holds
/// the keys, so it has to be the one this package installed or the one a
/// developer named -- not whichever binary of that name is first on a
/// search path. A bundled server that is missing fails at spawn with a
/// message saying which file, which is the right failure.
///
/// Behind a flag rather than taking `argv[1]`: Sailfish launches
/// `silica-qt5` apps through the invoker, which passes arguments of its
/// own, and a bare positional turned any of them into "the server binary".
#[must_use]
pub fn server_path<I>(args: I, env: Option<String>) -> String
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--rpc-server" {
            if let Some(path) = args.next() {
                return path;
            }
        } else if let Some(path) = arg.strip_prefix("--rpc-server=") {
            return path.to_string();
        }
    }
    env.filter(|path| !path.is_empty())
        .unwrap_or_else(|| BUNDLED_SERVER.to_string())
}

/// Stop the server, once the Qt event loop has returned.
///
/// Without this the child is only killed when the last `RpcClient` drops,
/// and the one held for the models never does: statics are not dropped at
/// exit. The server went anyway, on its stdin closing, but that was its
/// courtesy rather than this app's doing. Bounded, so a server that will
/// not die cannot hold the app open.
pub fn shutdown() {
    let Some((rpc, runtime)) = CONNECTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
    else {
        return;
    };
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    runtime.spawn(async move {
        let _ = rpc.shutdown().await;
        let _ = done_tx.send(());
    });
    let _ = done_rx.recv_timeout(SHUTDOWN_TIMEOUT);
}

/// The last few lines the server wrote to stderr, as one message, or
/// nothing when it said nothing. The one clue to why it went that anyone
/// will ever see.
fn last_words(tail: &[String]) -> Option<String> {
    const KEEP: usize = 5;
    if tail.is_empty() {
        return None;
    }
    let start = tail.len().saturating_sub(KEEP);
    Some(format!(
        "the core's last output was: {}",
        tail[start..].join(" | ")
    ))
}

/// How long to wait before the first restart, and the ceiling the wait
/// doubles towards. The first is short because the overwhelmingly likely
/// cause is the phone reclaiming memory, and respawning then works
/// immediately; the ceiling is what keeps a server that cannot start from
/// spinning.
const RESTART_DELAY_MIN: Duration = Duration::from_secs(1);
const RESTART_DELAY_MAX: Duration = Duration::from_secs(30);

/// A server that stayed up this long counts as having worked. The next
/// failure then starts backing off from the minimum again instead of from
/// wherever the last crash loop left the delay.
const HEALTHY_UPTIME: Duration = Duration::from_secs(60);

/// How many restarts without a healthy run in between before giving up and
/// saying so. With the backoff above that is a little over three minutes of
/// trying -- long enough to outlast anything transient, short enough that a
/// binary which will never start does not keep the phone awake.
const RESTART_LIMIT: u32 = 12;

/// How long to wait before restart number `attempt`.
fn restart_delay(attempt: u32) -> Duration {
    RESTART_DELAY_MIN
        .saturating_mul(1_u32.checked_shl(attempt).unwrap_or(u32::MAX))
        .min(RESTART_DELAY_MAX)
}

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

    /// Spawn `rpc_server_path` and drain its event stream. No-op if
    /// already started.
    pub start: qt_method!(fn(&mut self, rpc_server_path: QString)),

    /// `get_system_info` round trip; result lands in `system_info`.
    pub check_health: qt_method!(fn(&mut self)),

    /// Create a new unconfigured account.
    pub add_account: qt_method!(fn(&mut self)),
    /// Delete a profile and everything in it, on this device.
    ///
    /// `remove_account` is the core's name for it -- verified against the
    /// pinned binary, which has no `delete_account`. There is no undo.
    pub remove_account: qt_method!(fn(&mut self, account_id: u32)),

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

    /// The server binary `start` was given, kept so a restart need not be
    /// told again.
    rpc_server_path: String,
    /// Accounts whose IO was resumed, replayed after a restart. A fresh
    /// server has the account files but no IO running, so without this the
    /// app comes back able to read history and unable to receive.
    io_accounts: BTreeSet<u32>,
    /// Restarts since the last healthy run; drives the backoff and the
    /// give-up limit.
    restart_attempt: u32,
    /// When the current server was spawned, for [`HEALTHY_UPTIME`].
    server_started_at: Option<Instant>,
    /// True once a spawn has succeeded. Until then a failure is the app
    /// failing to start, which is reported and left alone; after it, a
    /// failure is something to retry.
    supervising: bool,
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

        let path = rpc_server_path.to_string();
        self.rpc_server_path.clone_from(&path);
        self.runtime = Some(runtime.clone());
        self.restart_attempt = 0;
        self.supervising = false;

        self.status = QString::from("starting");
        self.status_changed();

        Self::spawn_server(QPointer::from(&*self), path, runtime);
    }

    /// Spawn the server and wire the result up. Shared by `start` and every
    /// restart.
    ///
    /// An associated function taking what it needs, rather than a method:
    /// the object must not be borrowed while any of this runs. `start` is
    /// called from QML, which holds a mutable borrow for the duration, and
    /// `status_changed` is handled in QML by code that calls straight back
    /// in here. So every mutation below is scoped to a callback, and every
    /// signal is emitted with no borrow held.
    fn spawn_server(ptr: QPointer<Self>, path: String, runtime: CoreRuntime) {
        let started_ptr = ptr.clone();
        let retry_path = path.clone();
        let started = queued_callback(move |result: Result<Arc<RpcClient>, String>| {
            let Some(this) = started_ptr.as_pinned() else {
                return;
            };
            match result {
                Ok(rpc) => {
                    let (runtime, accounts) = {
                        let mut this_mut = this.borrow_mut();
                        this_mut.rpc = Some(rpc.clone());
                        this_mut.status = QString::from("ready");
                        this_mut.server_started_at = Some(Instant::now());
                        this_mut.supervising = true;
                        set_connection(this_mut.runtime.clone().map(|rt| (rpc.clone(), rt)));
                        (this_mut.runtime.clone(), this_mut.io_accounts.clone())
                    };
                    // Draining first: IO resumed before anything is reading
                    // the stream would deliver its events to nobody.
                    if let Some(runtime) = runtime {
                        Self::forward_events(started_ptr.clone(), rpc, runtime, retry_path.clone());
                    }
                    for account_id in accounts {
                        Self::resume_io(started_ptr.clone(), account_id);
                    }
                    // Last, because a handler may call back in here.
                    this.borrow().status_changed();
                }
                Err(err) => {
                    // Only a server that worked once is worth retrying; a
                    // first spawn that fails is reported and left alone,
                    // which is what the first screen reads.
                    if this.borrow().supervising {
                        Self::schedule_restart(
                            started_ptr.clone(),
                            retry_path.clone(),
                            Some(format!("could not restart the core: {err}")),
                        );
                        return;
                    }
                    // Never started: this is the app failing to start.
                    {
                        // Dropped so a later `start()` is not blocked by the
                        // already-started guard.
                        let mut this_mut = this.borrow_mut();
                        this_mut.runtime = None;
                        set_connection(None);
                        this_mut.status = format!("error: {err}").into();
                    }
                    this.borrow().status_changed();
                }
            }
        });

        runtime.spawn(async move {
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

    /// The server is gone: put the app into `reconnecting` and spawn
    /// another one after a backoff, or give up and say `stopped`.
    ///
    /// `failure` is what to report if this round gives up: the spawn error
    /// when a restart attempt is what failed, the server's last words when
    /// a running server exited, and `None` when it left in silence.
    fn schedule_restart(ptr: QPointer<Self>, path: String, failure: Option<String>) {
        let Some(this) = ptr.as_pinned() else { return };
        set_connection(None);

        let next = {
            let mut this_mut = this.borrow_mut();
            this_mut.rpc = None;
            // Nothing to restart: either `start` was never called, or a
            // previous round already gave up and dropped the runtime.
            let Some(runtime) = this_mut.runtime.clone() else {
                return;
            };
            // A server that stayed up long enough to be useful resets the
            // backoff, so an app running for a week does not treat its
            // second ever restart as if it were in a crash loop.
            if this_mut
                .server_started_at
                .is_some_and(|at| at.elapsed() >= HEALTHY_UPTIME)
            {
                this_mut.restart_attempt = 0;
            }
            this_mut.server_started_at = None;

            if this_mut.restart_attempt >= RESTART_LIMIT {
                // Same reason as the failed first spawn: leaving the runtime
                // in place would make a later `start()` a silent no-op.
                this_mut.runtime = None;
                // "stopped" and not an `error:` status: this is the state
                // the pages have a message for, and the reason -- when
                // there is one -- goes out on `core_error` instead, which
                // is where a detail the reader cannot act on belongs.
                this_mut.status = QString::from("stopped");
                None
            } else {
                let delay = restart_delay(this_mut.restart_attempt);
                this_mut.restart_attempt += 1;
                this_mut.status = QString::from("reconnecting");
                Some((delay, runtime))
            }
        };
        this.borrow().status_changed();

        let Some((delay, runtime)) = next else {
            if let Some(detail) = failure {
                this.borrow().core_error(detail.into());
            }
            return;
        };
        let spawn_runtime = runtime.clone();
        let retry = queued_callback(move |()| {
            Self::spawn_server(ptr.clone(), path.clone(), spawn_runtime.clone());
        });
        runtime.spawn(async move {
            tokio::time::sleep(delay).await;
            retry(());
        });
    }

    /// Resume IO for one account after a restart, without touching the
    /// `io_started` signal: nothing asked for this, and a page that reacted
    /// to it would be reacting to a reconnection it never requested.
    fn resume_io(ptr: QPointer<Self>, account_id: u32) {
        let Some(this) = ptr.as_pinned() else { return };
        let Some((rpc, runtime)) = this.borrow().connection() else {
            return;
        };
        let failed = queued_callback(move |err: String| {
            if let Some(this) = ptr.as_pinned() {
                this.borrow().core_error(err.into());
            }
        });
        runtime.spawn(async move {
            if let Err(err) = rpc.call::<_, ()>("start_io", (account_id,)).await {
                failed(err.to_string());
            }
        });
    }

    /// Where the core keeps account state: `POSTIVENE_ACCOUNTS_DIR`, else
    /// inside the directory sailjail grants the app. Without it the core
    /// would use `./accounts`, relative to whatever directory the app was
    /// launched from.
    ///
    /// The nesting is not a typo. Sailjail grants write access to
    /// `~/.local/share/<OrganizationName>/<ApplicationName>`, and
    /// postivene.desktop declares both as `postivene`, so the account
    /// directory has to sit under *both* to be writable once confined.
    /// The old `postivene/accounts` was a sibling of that grant, not a
    /// child, and would have become unwritable the moment the
    /// `[X-Sailjail]` section took effect.
    ///
    /// # Errors
    ///
    /// If neither `XDG_DATA_HOME` nor `HOME` is set, so there is nowhere
    /// to put the directory, or if it cannot be created.
    pub fn accounts_dir() -> Result<String, String> {
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
            let dir = base.join("postivene/postivene/accounts");
            Self::adopt_legacy_accounts(&base.join("postivene/accounts"), &dir);
            dir
        };
        // Private to this user: the directory holds the keys, the mail
        // password and every message. The mode is asked for at creation
        // and set again afterwards, because a directory that already
        // exists -- adopted from before the sandbox, or made by an older
        // build with the umask default -- keeps whatever it was given.
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&dir)
            .map_err(|err| format!("cannot create accounts dir {}: {err}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|err| format!("cannot restrict accounts dir {}: {err}", dir.display()))?;
        }
        Ok(dir.to_string_lossy().into_owned())
    }

    /// Move a profile left at the pre-sandbox location, once.
    ///
    /// Best effort by necessity: a confined app cannot see the old
    /// directory at all, since it is outside the grant. This helps the
    /// run that happens before confinement takes effect, and does nothing
    /// otherwise -- a profile stranded by an upgrade straight into the
    /// sandbox has to be moved by hand, or pointed at with
    /// `POSTIVENE_ACCOUNTS_DIR`. Never overwrites a profile that is
    /// already in the new place.
    fn adopt_legacy_accounts(legacy: &std::path::Path, wanted: &std::path::Path) {
        if wanted.exists() || !legacy.is_dir() {
            return;
        }
        let Some(parent) = wanted.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_ok() {
            // A failure here leaves the old directory untouched, which is
            // the right way to fail: the account is still there to move.
            let _ = std::fs::rename(legacy, wanted);
        }
    }

    /// Forward the core's event stream to `core_event` via queued
    /// callbacks, for as long as the transport lives.
    ///
    /// The setup and the loop share one `runtime.spawn` because
    /// `spawn_event_loop`'s internal `tokio::spawn` needs ambient runtime
    /// context.
    fn forward_events(
        ptr: QPointer<Self>,
        rpc: Arc<RpcClient>,
        runtime: CoreRuntime,
        path: String,
    ) {
        // The stream ends when the server dies -- killed for memory,
        // crashed, whatever. That is the one failure the app cannot see
        // for itself: every model goes quiet and the UI still looks fine.
        // So it is also where the next server gets started.
        let stopped_ptr = ptr.clone();
        let stopped = queued_callback(move |tail: Vec<String>| {
            Self::schedule_restart(stopped_ptr.clone(), path.clone(), last_words(&tail));
        });
        let emit = queued_callback(move |event: CoreEvent| {
            if let Some(this) = ptr.as_pinned() {
                let kind = match json::str_at(&event.event, "kind") {
                    "" => "Unknown".to_string(),
                    kind => kind.to_string(),
                };
                let payload = serde_json::to_string(&event.event).unwrap_or_default();
                // A typed signal too, so a progress bar need not parse
                // JSON. Both fire for these events.
                if kind == "Error" {
                    let text = match json::str_at(&event.event, "msg") {
                        "" => "the core reported an error",
                        text => text,
                    };
                    this.borrow().core_error(text.into());
                }
                if kind == "ConfigureProgress" {
                    if let Some(permille) = json::u32_opt(&event.event, "progress") {
                        this.borrow().configure_progress(event.context_id, permille);
                    }
                }
                this.borrow()
                    .core_event(event.context_id, kind.into(), payload.into());
            }
        });
        runtime.spawn(async move {
            let (mut events, _handle) = spawn_event_loop(rpc.clone());
            while let Some(event) = events.recv().await {
                emit(event);
            }
            // The stream ends only when the transport does (see
            // `spawn_event_loop`), so this is the server gone -- and what
            // it wrote to stderr on the way out is the only clue why.
            stopped(rpc.stderr_tail());
        });
    }

    /// Run a `get_system_info` round trip into
    /// [`DeltaChatCore::system_info`].
    pub fn check_health(&mut self) {
        let Some((rpc, runtime)) = self.connection() else {
            self.core_error(QString::from("not started"));
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
                // Reported on its own signal rather than written into
                // `status`: that is the app's answer to "is the core
                // there", and a health check that times out is not the
                // same as a core that has gone away -- but every button
                // enabled on `status === "ready"` would go dead.
                Err(err) => this.borrow().core_error(err.into()),
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

    /// Delete a profile and everything in it.
    ///
    /// Refreshes afterwards rather than trusting the caller to: the list
    /// this leaves behind is what decides whether the app still has a
    /// profile to show, and `accounts_refreshed` is how that is learnt.
    pub fn remove_account(&mut self, account_id: u32) {
        let Some((rpc, runtime)) = self.connection() else {
            self.account_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => this.borrow_mut().refresh_accounts(),
                Err(err) => this.borrow().account_error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("remove_account", (account_id,))
                .await
                .map_err(|err| err.to_string());
            done(result);
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
                            Some(AccountItem {
                                account_id: json::u32_opt(account, "id")?,
                                display_name: json::text(account, "displayName"),
                                addr: json::text(account, "addr"),
                                is_configured: json::str_at(account, "kind") == "Configured",
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
        // Remembered before the call, not after it succeeds: a restart has
        // to resume whatever the app asked for, and an attempt that failed
        // against a dying server is exactly what the next one should redo.
        self.io_accounts.insert(account_id);
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
                // upstream deprecated (docs/PROJECT.md).
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
                // EnteredLoginParam autoconfigures (docs/PROJECT.md).
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

    /// The completion path of `check_qr`: the classification, as the
    /// core gave it, or the error.
    fn qr_callback(&self) -> impl Fn((u32, Result<serde_json::Value, String>)) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |result: (u32, Result<serde_json::Value, String>)| {
            let Some(this) = ptr.as_pinned() else { return };
            let (account_id, result) = result;
            match result {
                Ok(qr) => {
                    let kind = match json::str_at(&qr, "kind") {
                        "" => "unknown".to_string(),
                        kind => kind.to_string(),
                    };
                    let payload = serde_json::to_string(&qr).unwrap_or_default();
                    this.borrow()
                        .qr_checked(account_id, kind.into(), payload.into());
                }
                Err(err) => this.borrow().qr_error(err.into()),
            }
        })
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
            if json::str_at(account, "kind") != "Unconfigured" {
                return None;
            }
            json::u32_opt(account, "id")
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

        let done = self.qr_callback();

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

#[cfg(test)]
mod tests {
    use super::{
        last_words, restart_delay, server_path, BUNDLED_SERVER, RESTART_DELAY_MAX,
        RESTART_DELAY_MIN, RESTART_LIMIT,
    };

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn the_server_is_the_flag_then_the_environment_then_the_bundle() {
        assert_eq!(
            server_path(args(&["--rpc-server", "/x/server"]), Some("/env".into())),
            "/x/server"
        );
        assert_eq!(
            server_path(args(&["--rpc-server=/y/server"]), None),
            "/y/server"
        );
        // The invoker's own arguments are not a server.
        assert_eq!(
            server_path(
                args(&["-prestart", "--type=silica-qt5"]),
                Some("/env".into())
            ),
            "/env"
        );
        assert_eq!(server_path(args(&[]), None), BUNDLED_SERVER);
    }

    #[test]
    fn the_server_is_never_looked_up_on_path() {
        // A bare name would go to PATH, and whatever answered there would
        // be handed the mail password. Nothing here produces one, an
        // empty environment variable included.
        for env in [None, Some(String::new())] {
            let path = server_path(args(&["--rpc-server"]), env);
            assert!(path.starts_with('/'), "{path:?} would be looked up on PATH");
        }
    }

    #[test]
    fn last_words_keep_the_tail_and_say_nothing_for_silence() {
        assert_eq!(last_words(&[]), None);
        let lines: Vec<String> = (1..=7).map(|n| format!("line {n}")).collect();
        let words = last_words(&lines).unwrap_or_default();
        assert!(words.ends_with("line 3 | line 4 | line 5 | line 6 | line 7"));
        assert!(!words.contains("line 2"));
    }

    #[test]
    fn the_backoff_doubles_and_then_stops_doubling() {
        assert_eq!(restart_delay(0), RESTART_DELAY_MIN);
        assert_eq!(restart_delay(1), RESTART_DELAY_MIN * 2);
        assert_eq!(restart_delay(2), RESTART_DELAY_MIN * 4);
        assert_eq!(restart_delay(5), RESTART_DELAY_MAX);
        // Every later attempt waits the ceiling, and the shift that would
        // overflow a u32 does not panic.
        assert_eq!(restart_delay(RESTART_LIMIT), RESTART_DELAY_MAX);
        assert_eq!(restart_delay(u32::MAX), RESTART_DELAY_MAX);
    }

    #[test]
    fn giving_up_takes_longer_than_anything_transient() {
        let total: std::time::Duration = (0..RESTART_LIMIT).map(restart_delay).sum();
        assert!(
            total >= std::time::Duration::from_secs(180),
            "the app gives up after {total:?}, which is not long enough to \
             outlast a phone that is briefly out of memory"
        );
    }
}
