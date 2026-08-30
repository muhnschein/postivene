//! The account's own profile: the name others see, the line under it, and
//! the picture.
//!
//! All three are core config keys rather than a record of their own, which
//! is why this is a small object over `get_config`/`set_config` rather
//! than a model.

use qmetaobject::*;

use crate::core::connection;

/// One account's profile, loaded and saved.
///
/// ```qml
/// Profile { id: profile; account_id: page.accountId }
/// TextField { text: profile.display_name }
/// ```
#[derive(QObject, Default)]
pub struct Profile {
    base: qt_base_class!(trait QObject),

    /// Whose profile this is. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Emitted when the account changes.
    pub account_changed: qt_signal!(),

    /// The name everyone written to sees. `displayname` to the core.
    pub display_name: qt_property!(QString; NOTIFY loaded_changed),
    /// The line under it, which the core appends to outgoing mail.
    /// `selfstatus` to the core.
    pub status: qt_property!(QString; NOTIFY loaded_changed),
    /// Absolute path to the picture inside the core's blob directory,
    /// empty when there is none.
    pub avatar_path: qt_property!(QString; NOTIFY loaded_changed),
    /// The address this profile sends from. Not editable: changing it is
    /// setting up a different transport, not renaming this one.
    pub address: qt_property!(QString; NOTIFY loaded_changed),

    /// Whether the other end is told when a message has been read.
    /// `mdns_enabled` to the core, which defaults it on.
    pub read_receipts: qt_property!(bool; NOTIFY loaded_changed),

    /// True once the profile has been read from the core. Until then the
    /// fields are empty because nothing has been loaded, not because the
    /// profile is blank -- and a save then would wipe it.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted when any of the fields, or `loaded`, changes.
    pub loaded_changed: qt_signal!(),

    /// Something failed. The message is the core's own.
    pub error: qt_signal!(message: QString),
    /// The profile was stored.
    pub saved: qt_signal!(),

    /// Read the profile from the core.
    pub reload: qt_method!(fn(&mut self)),
    /// Store the name and the status.
    pub save: qt_method!(fn(&mut self, display_name: QString, status: QString)),
    /// Use the image at `path` as the picture. The core copies it into its
    /// own blob directory, so the file picked can go away afterwards.
    pub set_picture: qt_method!(fn(&mut self, path: QString)),
    /// Remove the picture.
    pub clear_picture: qt_method!(fn(&mut self)),
    /// Turn read receipts on or off.
    pub set_read_receipts: qt_method!(fn(&mut self, enabled: bool)),

    /// Counts loads, so a slow answer to an older question cannot land on
    /// top of a newer one -- switching profiles starts one per switch.
    generation: u64,
}

/// What one load brings back: name, status, picture, address, receipts.
type Fields = (String, String, String, String, bool);

impl Profile {
    /// Set the account and reload if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            // The fields still hold the previous profile's, and a save
            // before the load lands would write them onto this one.
            if self.loaded {
                self.loaded = false;
            }
            self.reload();
        }
    }

    /// Read the profile from the core.
    pub fn reload(&mut self) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Fields, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok((display_name, status, avatar_path, address, read_receipts)) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.display_name = display_name.into();
                        this_mut.status = status.into();
                        this_mut.avatar_path = avatar_path.into();
                        this_mut.address = address.into();
                        this_mut.read_receipts = read_receipts;
                        this_mut.loaded = true;
                    }
                    this.borrow().loaded_changed();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = async {
                let get = |key: &'static str| {
                    let rpc = rpc.clone();
                    async move {
                        rpc.call::<_, Option<String>>("get_config", (account_id, key))
                            .await
                            .map(Option::unwrap_or_default)
                            .map_err(|err| err.to_string())
                    }
                };
                Ok::<_, String>((
                    get("displayname").await?,
                    get("selfstatus").await?,
                    get("selfavatar").await?,
                    get("configured_addr").await?,
                    // The core stores it as "1"/"0" and defaults it on,
                    // so an absent value is not "off".
                    get("mdns_enabled").await? != "0",
                ))
            }
            .await;
            done(result);
        });
    }

    /// Store the name and the status.
    pub fn save(&mut self, display_name: QString, status: QString) {
        // Saving what was never loaded writes empty strings over a profile
        // the reader never saw.
        if !self.loaded {
            return;
        }
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback();
        let display_name = display_name.to_string();
        let status = status.to_string();

        runtime.spawn(async move {
            let result = async {
                rpc.call::<_, ()>(
                    "set_config",
                    (account_id, "displayname", Some(display_name)),
                )
                .await
                .map_err(|err| err.to_string())?;
                rpc.call::<_, ()>("set_config", (account_id, "selfstatus", Some(status)))
                    .await
                    .map_err(|err| err.to_string())?;
                Ok::<_, String>(())
            }
            .await;
            done(result);
        });
    }

    /// Use the image at `path` as the picture.
    pub fn set_picture(&mut self, path: QString) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        // A picker hands back a URL, the core wants a path.
        let path = path.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        if path.is_empty() {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback();

        runtime.spawn(async move {
            // `selfavatar` is a path to an image the core copies into its
            // blob directory, not the image itself. Verified against the
            // pinned binary: an empty string is refused outright with
            // "Copying new blobfile failed", and null is how it is
            // cleared.
            let result = rpc
                .call::<_, ()>("set_config", (account_id, "selfavatar", Some(path)))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Remove the picture.
    pub fn clear_picture(&mut self) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback();

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>(
                    "set_config",
                    (account_id, "selfavatar", Option::<String>::None),
                )
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Turn read receipts on or off.
    pub fn set_read_receipts(&mut self, enabled: bool) {
        let account_id = self.account_id;
        // Same guard as `save`: before the load lands, this object's idea
        // of the setting is the default rather than the account's.
        if account_id == 0 || !self.loaded {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback();
        let value = if enabled { "1" } else { "0" };

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("set_config", (account_id, "mdns_enabled", Some(value)))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Report a store, and read back what the core actually kept: it
    /// rewrites a picture into its own blob directory, so the path handed
    /// to it is not the path it holds afterwards.
    fn stored_callback(&self) -> impl Fn(Result<(), String>) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => {
                    this.borrow_mut().reload();
                    this.borrow().saved();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        })
    }
}
