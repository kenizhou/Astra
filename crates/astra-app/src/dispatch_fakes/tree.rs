// SPDX-License-Identifier: GPL-3.0-only
//! The folder-tree fixtures and the tree-mutating half of the fake provider:
//! the key, folder, and inbox builders every fixture names, and the
//! [`MailboxWrites`] leg that applies a tree edit to the server's copy the way
//! a writable server does, so the engine's outbox path runs end to end over
//! the fake. Split from the fixture root so that file stays under the
//! 500-line limit; the re-export keeps every test module's `fakes::…` paths
//! unchanged.

#![allow(dead_code)]

use async_trait::async_trait;

use engine_api::{AccountId, Mailbox, MailboxEdit, MailboxEditReceipt, MailboxId, MailboxRole};
use engine_provider::{MailboxWrites, ProviderResult};

use super::FakeProvider;

/// A folder key, the way every fixture names one.
pub(crate) fn key(value: &str) -> MailboxId {
    MailboxId::try_from(value).unwrap()
}

/// A plain top-level folder called `name`.
pub(crate) fn folder(key_value: &str, name: &str) -> Mailbox {
    Mailbox::new(key(key_value), name)
}

/// The role-bearing Inbox every account's tree leads with.
pub(crate) fn inbox() -> Mailbox {
    let mut mailbox = folder("inbox", "Inbox");
    mailbox.role = Some(MailboxRole::Inbox);
    mailbox
}

#[async_trait]
impl MailboxWrites for FakeProvider {
    /// Applies one tree edit to the server's copy and answers the key the edit
    /// resolved to, the IMAP shape: a folder's key is its path, so a rename or
    /// a move mints a new key and every descendant's key moves with it.
    async fn edit_mailbox(
        &self,
        _account: &AccountId,
        edit: &MailboxEdit,
    ) -> ProviderResult<MailboxEditReceipt> {
        self.mailbox_edits.lock().unwrap().push(edit.clone());
        let mut tree = self.mailboxes.lock().unwrap();
        match edit {
            MailboxEdit::Create { name, parent } => {
                let key = child_key(parent.as_ref(), name);
                let mut mailbox = Mailbox::new(key.clone(), name.clone());
                mailbox.parent = parent.clone();
                tree.push(mailbox);
                Ok(MailboxEditReceipt::resolved(key))
            }
            MailboxEdit::Update {
                target,
                name,
                parent,
            } => {
                let key = child_key(parent.as_ref(), name);
                if target == &key {
                    return Ok(MailboxEditReceipt::resolved(key));
                }
                rekey_subtree(&mut tree, target, &key);
                if let Some(folder) = tree.iter_mut().find(|mailbox| mailbox.id == key) {
                    folder.name = name.clone();
                    folder.parent = parent.clone();
                }
                Ok(MailboxEditReceipt::resolved(key))
            }
            MailboxEdit::Trash {
                target,
                trash,
                name,
                ..
            } => {
                let key = child_key(Some(trash), name);
                rekey_subtree(&mut tree, target, &key);
                if let Some(folder) = tree.iter_mut().find(|mailbox| mailbox.id == key) {
                    folder.name = name.clone();
                    folder.parent = Some(trash.clone());
                }
                Ok(MailboxEditReceipt::resolved(key))
            }
            MailboxEdit::Delete { target } => {
                let prefix = format!("{}/", target.as_str());
                tree.retain(|mailbox| {
                    mailbox.id != *target && !mailbox.id.as_str().starts_with(&prefix)
                });
                Ok(MailboxEditReceipt::removed())
            }
        }
    }
}

/// The path key a folder called `name` directly inside `parent` holds: the
/// IMAP convention the fake's whole tree models, `/`-joined.
fn child_key(parent: Option<&MailboxId>, name: &str) -> MailboxId {
    let path = match parent {
        Some(parent) => format!("{}/{}", parent.as_str(), name),
        None => name.to_owned(),
    };
    MailboxId::try_from(path.as_str()).unwrap()
}

/// Moves `target` and every folder beneath it under the `to` key prefix,
/// rebasing the descendants' paths the way a path-keyed server does.
fn rekey_subtree(tree: &mut [Mailbox], target: &MailboxId, to: &MailboxId) {
    let prefix = format!("{}/", target.as_str());
    for mailbox in tree.iter_mut() {
        if mailbox.id == *target {
            mailbox.id = to.clone();
        } else if let Some(rest) = mailbox.id.as_str().strip_prefix(&prefix) {
            let path = format!("{}/{}", to.as_str(), rest);
            mailbox.id = MailboxId::try_from(path.as_str()).unwrap();
            mailbox.parent = Some(to.clone());
        }
    }
}
