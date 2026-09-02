//! Contacts, and the ways a chat gets started.
//!
//! Until this existed the app could only show conversations that arrived on
//! their own: nothing created one.

use std::cell::RefCell;

use qmetaobject::*;

use crate::core::connection;
use crate::json;
use crate::models::{ContactItem, ContactListModel};

/// Known, unblocked contacts, and the calls that turn one into a chat.
///
/// ```qml
/// ContactList { id: contacts; account_id: page.accountId }
/// SilicaListView { model: contacts.rows }
/// ```
#[derive(QObject, Default)]
pub struct ContactList {
    base: qt_base_class!(trait QObject),

    /// Whose contacts these are. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Emitted when the account changes.
    pub account_changed: qt_signal!(),

    /// Filter, matched by the core against name and address. Setting it
    /// reloads.
    pub query: qt_property!(QString; WRITE set_query NOTIFY query_changed),
    /// Emitted when the query changes.
    pub query_changed: qt_signal!(),

    /// The rows, for a `SilicaListView`'s `model`.
    pub rows: qt_property!(RefCell<ContactListModel>; CONST),

    /// How many rows there are.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    /// Emitted after any change to `rows`.
    pub rows_changed: qt_signal!(),

    /// Something failed. The message is the core's own.
    pub error: qt_signal!(message: QString),

    /// Reload the list.
    pub reload: qt_method!(fn(&mut self)),

    /// Open the one-to-one chat with a contact, creating it if needed.
    /// Answers on `chat_ready`.
    pub open_chat_with: qt_method!(fn(&mut self, contact_id: u32)),

    /// Create a group with the given name and members, and open it.
    /// Answers on `chat_ready`.
    pub create_group: qt_method!(fn(&mut self, name: QString, member_ids: QVariantList)),

    /// Follow an invite -- a scanned QR payload or a pasted
    /// `https://i.delta.chat/...` link -- and open the chat it leads to.
    /// This is how a Delta Chat contact is normally added: an address alone
    /// cannot be encrypted to (docs/PROJECT.md). Answers on `chat_ready`.
    pub join_by_invite: qt_method!(fn(&mut self, qr_content: QString)),

    /// Fetch this account's own invite, the one to hand out. Answers on
    /// `invite_ready`.
    pub fetch_invite: qt_method!(fn(&mut self)),
    /// This account's invite link.
    pub invite_ready: qt_signal!(link: QString),

    /// A chat is ready to be shown.
    pub chat_ready: qt_signal!(chat_id: u32),

    /// Counts loads, so a slow answer to an old query cannot land on top of
    /// a newer one. Typing "anna" starts four of these and they are not
    /// answered in the order they were asked.
    generation: u64,
}

impl ContactList {
    /// How many rows there are.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Set the account and reload if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            self.reload();
        }
    }

    /// Set the filter and reload if it changed.
    pub fn set_query(&mut self, query: QString) {
        if self.query.to_string() != query.to_string() {
            self.query = query;
            self.query_changed();
            self.reload();
        }
    }

    /// Reload the list.
    pub fn reload(&mut self) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let query = self.query.to_string();
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<ContactItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Answered after something newer was asked: these are results
            // for a query the reader has already typed past.
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok(items) => {
                    this.borrow_mut().rows.borrow_mut().reset_data(items);
                    this.borrow().rows_changed();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let query = if query.is_empty() { None } else { Some(query) };
            // listFlags 0: known, unblocked contacts, without the special
            // "add self" and "verified only" filters.
            let result = rpc
                .call::<_, Vec<serde_json::Value>>("get_contacts", (account_id, 0, query))
                .await
                .map(|contacts| contacts.iter().map(contact_row).collect())
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Open the one-to-one chat with a contact.
    pub fn open_chat_with(&mut self, contact_id: u32) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.chat_callback();

        runtime.spawn(async move {
            let result = rpc
                .call::<_, u32>("create_chat_by_contact_id", (account_id, contact_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Create a group and add the given members.
    pub fn create_group(&mut self, name: QString, member_ids: QVariantList) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(u32, Vec<String>), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok((chat_id, refused)) => {
                    // Said first, then opened anyway: the group is real,
                    // and leaving the reader on the picker with an error is
                    // how one ends up stranded with no way back to it.
                    if !refused.is_empty() {
                        this.borrow().error(
                            format!(
                                "the group was made, but some people could not be added ({})",
                                refused.join("; ")
                            )
                            .into(),
                        );
                    }
                    this.borrow().chat_ready(chat_id);
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        let name = name.to_string();
        let members: Vec<u32> = member_ids
            .into_iter()
            .filter_map(|value| i32::from_qvariant(value.clone()))
            .filter_map(|value| u32::try_from(value).ok())
            .collect();
        runtime.spawn(async move {
            let result = async {
                // Encrypted, of key-contacts, which is what the reference
                // client's "New Group" makes. It is the method that decides
                // that -- `create_group_chat_unencrypted` is the other one.
                // The third argument is upstream's deprecated `protect`,
                // which its own docs say to pass `false`; it is bound as
                // `_protect` there and read by nothing.
                let chat_id: u32 = rpc
                    .call("create_group_chat", (account_id, name, false))
                    .await
                    .map_err(|err| err.to_string())?;
                // Every member attempted, and the chat handed back either
                // way: it exists on the core from the call above, so
                // failing out of here left a half-built group the reader
                // was never shown and could not find.
                let mut refused = Vec::new();
                for member in members {
                    if let Err(err) = rpc
                        .call::<_, ()>("add_contact_to_chat", (account_id, chat_id, member))
                        .await
                    {
                        refused.push(format!("{member}: {err}"));
                    }
                }
                Ok::<_, String>((chat_id, refused))
            }
            .await;
            done(result);
        });
    }

    /// Follow an invite and open the chat it leads to.
    pub fn join_by_invite(&mut self, qr_content: QString) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.chat_callback();

        let qr_content = qr_content.to_string();
        runtime.spawn(async move {
            let result = async {
                // Ask the core what the payload is before acting on it: it
                // knows the formats, and guessing at them here would be the
                // protocol work docs/PROJECT.md rules out.
                let qr: serde_json::Value = rpc
                    .call("check_qr", (account_id, qr_content.clone()))
                    .await
                    .map_err(|err| err.to_string())?;
                let kind = json::str_at(&qr, "kind");
                if !matches!(kind, "askVerifyContact" | "askVerifyGroup") {
                    return Err(format!(
                        "that link is not a contact or group invite ({kind})"
                    ));
                }
                // Returns as soon as the chat exists; the handshake itself
                // finishes in the background.
                rpc.call::<_, u32>("secure_join", (account_id, qr_content))
                    .await
                    .map_err(|err| err.to_string())
            }
            .await;
            done(result);
        });
    }

    /// Fetch this account's own invite link.
    pub fn fetch_invite(&mut self) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<String, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(link) => this.borrow().invite_ready(link.into()),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            // A null chat gives the account's own contact invite; a chat id
            // would give that group's.
            let result = rpc
                .call::<_, String>(
                    "get_chat_securejoin_qr_code",
                    (account_id, Option::<u32>::None),
                )
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// The shared completion path of the three chat-opening methods.
    fn chat_callback(&self) -> impl Fn(Result<u32, String>) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |result: Result<u32, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(chat_id) => this.borrow().chat_ready(chat_id),
                Err(err) => this.borrow().error(err.into()),
            }
        })
    }
}

/// `DC_CONTACT_ID_SELF`: the account's own contact. Never listed by
/// `get_contacts`, but a member of every group the account is in.
pub(crate) const SELF_CONTACT_ID: u32 = 1;

/// One row from the core's contact object.
pub(crate) fn contact_row(contact: &serde_json::Value) -> ContactItem {
    let address = json::str_at(contact, "address");
    let display_name = match json::str_at(contact, "displayName") {
        "" => address,
        name => name,
    };
    let contact_id = json::u32_at(contact, "id");
    ContactItem {
        contact_id,
        display_name: display_name.into(),
        address: address.into(),
        is_verified: json::flag(contact, "isVerified"),
        is_key_contact: json::flag(contact, "isKeyContact"),
        is_self: contact_id == SELF_CONTACT_ID,
        // `color` is pinned by the integration test, which checks it on a
        // message's sender -- the same shape as a contact. The picture key
        // is not pinned, so a rename upstream shows up as a contact
        // falling back to its initial rather than as a failure.
        color: json::text(contact, "color"),
        avatar_path: json::text(contact, "profileImage"),
    }
}
