// SPDX-License-Identifier: GPL-3.0-only
//! The fixtures the folder proofs share: the app over the harness sender
//! account (the pane's writes are per-account, so one dial serves), the
//! pane-row lookups the assertions read, and the sweep that withdraws every
//! proof folder while rescuing the seeded notes a run may have left inside
//! one, so the harness keeps its three-note contract whatever interrupted
//! run came before.

use std::sync::Arc;

use astra_viewmodel::{FolderRole, FolderRow};
use async_trait::async_trait;
use engine_api::{AccountId, EmailAddress, Engine, Provider};

use crate::{
    reference::MessageRef, Account, App, AppObserver, FolderIntent, FolderRef, Intent,
    MailboxConnector,
};

use super::{dial_with_accepted_certificate, inbox_key, CountingObserver, NOTES};

/// The parent folder's name, unique among the harness fixtures.
pub(super) const PARENT: &str = "Harness folder proofs";

/// The proof folder's name as it is created, nested under [`PARENT`].
pub(super) const NESTED: &str = "Folder leg";

/// The proof folder's name after the rename step.
pub(super) const RENAMED: &str = "Folder leg renamed";

/// The dedicated move proof's folder, created at the top of the tree.
pub(super) const MOVED: &str = "Move proof folder";

/// The host-side folder binder the desktop loop installs: a late bind over
/// the dial's shared connections. A folder the proof creates binds no
/// provider at the dial, so without the binder no pass would ever sync it
/// and its mail would be invisible until a restart.
struct HarnessBinder {
    /// The accepted config the dial handed back.
    config: astra_account::AccountConfig,
    /// The shared connections the dial drew from.
    connections: Arc<astra_account::ImapConnections>,
}

#[async_trait]
impl MailboxConnector<Box<dyn Provider>> for HarnessBinder {
    async fn connect_folder(
        &self,
        _account: &AccountId,
        mailbox_key: &str,
    ) -> Option<Box<dyn Provider>> {
        astra_account::connect_imap_mailbox(&self.connections, &self.config, mailbox_key)
            .await
            .ok()
    }
}

/// Dials the harness sender account and returns the app over it: the shape
/// the folder proofs build, the same way the desktop host builds its loop,
/// binder included (a folder the proof creates needs a late bind before any
/// pass can sync it).
pub(super) async fn sender_app(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (App<Box<dyn Provider>>, AccountId) {
    let (account, providers, config, connections) =
        dial_with_accepted_certificate(host, port, user, password).await;
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: account.clone(),
            providers,
            identity: EmailAddress::new("astra1@example.test"),
        }],
        Some(Box::new(HarnessBinder {
            config,
            connections,
        })),
        observer,
    );
    (app, account)
}

/// The pane's row for the account's folder named `name`, the folder proofs'
/// fixture assertion.
pub(super) fn folder_row_by_name(
    app: &App<Box<dyn Provider>>,
    account: &AccountId,
    name: &str,
) -> FolderRow {
    app.mailbox_list()
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .and_then(|row| row.folders.iter().find(|folder| folder.name == name))
        .cloned()
        .unwrap_or_else(|| panic!("the pane lists the account's folder named {name}"))
}

/// Whether the folder keyed `key` sits under the account's Trash row: the
/// trash step's assertion, walked up the pane's drawn tree.
pub(super) fn under_trash(app: &App<Box<dyn Provider>>, account: &AccountId, key: &str) -> bool {
    let snapshot = app.mailbox_list();
    let Some(row) = snapshot
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
    else {
        return false;
    };
    let Some(trash) = row
        .folders
        .iter()
        .find(|folder| folder.role == Some(FolderRole::Trash))
    else {
        return false;
    };
    let mut cursor = row.folders.iter().find(|folder| folder.key.as_str() == key);
    while let Some(folder) = cursor {
        if folder.key.as_str() == trash.key.as_str() {
            return true;
        }
        cursor = folder.parent.as_ref().and_then(|parent| {
            row.folders
                .iter()
                .find(|other| other.key.as_str() == parent.as_str())
        });
    }
    false
}

/// The stored message whose subject matches, as a move names it: the row's
/// own key is the ref a move batch carries.
pub(super) async fn note_ref(
    app: &App<Box<dyn Provider>>,
    account: &AccountId,
    subject: &str,
) -> MessageRef {
    app.engine
        .messages(account)
        .await
        .expect("the account's stored messages")
        .into_iter()
        .find(|message| message.envelope.subject.as_deref() == Some(subject))
        .map_or_else(
            || panic!("the seeded note {subject} is in the store"),
            |message| MessageRef {
                account: account.clone(),
                key: message.id.key().clone(),
            },
        )
}

/// Withdraws every folder the proofs create and rescues the seeded notes a
/// run may have left inside one: each note rides back to the INBOX through
/// the product's own move, then each leftover folder is deleted for good,
/// so the account's tree keeps its seeded shape. The sweep runs at the start
/// and the end of the folder proofs, so a run interrupted before its cleanup
/// converges.
pub(super) async fn withdraw_proof_folders(app: &App<Box<dyn Provider>>, account: &AccountId) {
    let names = [PARENT, NESTED, RENAMED, MOVED];
    let snapshot = app.mailbox_list();
    let leftovers: Vec<String> = snapshot
        .account_folders
        .iter()
        .find(|row| row.account_id == *account)
        .map_or_else(Vec::new, |row| {
            row.folders
                .iter()
                .filter(|folder| names.contains(&folder.name.as_str()))
                .map(|folder| folder.key.as_str().to_owned())
                .collect()
        });
    drop(snapshot);
    if leftovers.is_empty() {
        return;
    }
    // The rescue comes first: the delete that follows is permanent, and a
    // seeded note left inside a proof folder is a note the harness loses.
    let stuck: Vec<MessageRef> = app
        .engine
        .messages(account)
        .await
        .expect("the account's stored messages")
        .into_iter()
        .filter(|message| {
            message
                .envelope
                .subject
                .as_deref()
                .is_some_and(|subject| NOTES.contains(&subject))
                && message.mailboxes.iter().any(|mailbox| {
                    leftovers
                        .iter()
                        .any(|leftover| leftover == mailbox.as_str())
                })
        })
        .map(|message| MessageRef {
            account: account.clone(),
            key: message.id.key().clone(),
        })
        .collect();
    if !stuck.is_empty() {
        let inbox = FolderRef::from_parts(account.as_str(), inbox_key(app, account))
            .expect("the INBOX folder reference");
        app.dispatch(Intent::MoveMessages {
            rows: stuck,
            folder: inbox,
        })
        .await
        .unwrap();
    }
    for folder in leftovers {
        app.dispatch(Intent::Folders(FolderIntent::Delete {
            account: account.clone(),
            folder,
        }))
        .await
        .unwrap();
    }
}
