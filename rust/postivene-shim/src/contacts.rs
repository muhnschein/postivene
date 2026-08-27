//! Contacts, and the ways a chat gets started.
//!
//! Until this existed the app could only show conversations that arrived on
//! their own: nothing created one.

use std::cell::RefCell;

use qmetaobject::*;

use crate::core::connection;
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

    /// Add a contact by address and open the chat with them. `name` may be
    /// empty. Answers on `chat_ready`.
    pub start_chat_with_address: qt_method!(fn(&mut self, address: QString, name: QString)),

    /// Create a group with the given name and members, and open it.
    /// Answers on `chat_ready`.
    pub create_group: qt_method!(fn(&mut self, name: QString, member_ids: QVariantList)),

    /// A chat is ready to be shown.
    pub chat_ready: qt_signal!(chat_id: u32),
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

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<ContactItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
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

    /// Add a contact by address and open the chat with them.
    pub fn start_chat_with_address(&mut self, address: QString, name: QString) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.chat_callback();

        let address = address.to_string();
        let name = name.to_string();
        runtime.spawn(async move {
            let result = async {
                let name = if name.is_empty() { None } else { Some(name) };
                let contact_id: u32 = rpc
                    .call("create_contact", (account_id, address, name))
                    .await
                    .map_err(|err| err.to_string())?;
                rpc.call::<_, u32>("create_chat_by_contact_id", (account_id, contact_id))
                    .await
                    .map_err(|err| err.to_string())
            }
            .await;
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
        let done = self.chat_callback();

        let name = name.to_string();
        let members: Vec<u32> = member_ids
            .into_iter()
            .filter_map(|value| i32::from_qvariant(value.clone()))
            .filter_map(|value| u32::try_from(value).ok())
            .collect();
        runtime.spawn(async move {
            let result = async {
                // `protect` true: an encrypted group of key-contacts, which
                // is what the reference client's "New Group" makes.
                let chat_id: u32 = rpc
                    .call("create_group_chat", (account_id, name, true))
                    .await
                    .map_err(|err| err.to_string())?;
                for member in members {
                    rpc.call::<_, ()>("add_contact_to_chat", (account_id, chat_id, member))
                        .await
                        .map_err(|err| err.to_string())?;
                }
                Ok::<_, String>(chat_id)
            }
            .await;
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

/// One row from the core's contact object.
fn contact_row(contact: &serde_json::Value) -> ContactItem {
    let address = contact
        .get("address")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let display_name = contact
        .get("displayName")
        .and_then(serde_json::Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or(address);
    ContactItem {
        contact_id: contact
            .get("id")
            .and_then(serde_json::Value::as_u64)
            .and_then(|id| u32::try_from(id).ok())
            .unwrap_or(0),
        display_name: display_name.into(),
        address: address.into(),
        is_verified: contact
            .get("isVerified")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        is_key_contact: contact
            .get("isKeyContact")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    }
}
