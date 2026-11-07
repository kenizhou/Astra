// SPDX-License-Identifier: GPL-3.0-only
//! `messages mark-read`, `set-flagged`, `archive`, and `trash`: the four
//! single-message act verbs, each through the engine's outbox under the
//! offline provider. Nothing here sends and nothing deletes permanently:
//! the row's trash is the agent's ceiling, the app layer's delete verb
//! stays unexposed. `--preview` prints the very `MailEdit` the engine call
//! would carry, from the one builder both paths share, the drafts pattern.

use std::sync::atomic::{AtomicU64, Ordering};

use engine_api::{
    AccountId, Engine, MailEdit, Mailbox, MailboxId, MailboxRole, Provider, ProviderKey,
};
use serde_json::{json, Value};

use super::Answer;
use crate::envelope::Code;

/// Marks one message read (`true`) or unread (`false`) through the engine's
/// keyword edit.
pub(crate) async fn mark_read<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    key: &str,
    read: bool,
    preview: bool,
) -> Answer {
    let key = parse_key(key)?;
    ensure_message(engine, account, &key).await?;
    let edit = MailEdit::mark_seen(key.clone(), read);
    apply(engine, provider, account, &key, &edit, preview).await
}

/// Flags one message (`true`) or clears its flag (`false`).
pub(crate) async fn set_flagged<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    key: &str,
    flagged: bool,
    preview: bool,
) -> Answer {
    let key = parse_key(key)?;
    ensure_message(engine, account, &key).await?;
    let edit = MailEdit::set_flagged(key.clone(), flagged);
    apply(engine, provider, account, &key, &edit, preview).await
}

/// Moves one message to the account's archive folder.
pub(crate) async fn archive<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    key: &str,
    preview: bool,
) -> Answer {
    move_to(
        engine,
        provider,
        account,
        key,
        &MailboxRole::Archive,
        preview,
    )
    .await
}

/// Moves one message to the account's trash folder, recoverable from there.
pub(crate) async fn trash<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    key: &str,
    preview: bool,
) -> Answer {
    move_to(engine, provider, account, key, &MailboxRole::Trash, preview).await
}

/// Builds the move edit for `role`'s resolved destination: the one builder
/// the run and the preview share. A destination that resolves to nothing is
/// a runtime error naming the missing folder, never an edit queued against
/// a key that names nothing.
async fn move_to<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    key: &str,
    role: &MailboxRole,
    preview: bool,
) -> Answer {
    let key = parse_key(key)?;
    ensure_message(engine, account, &key).await?;
    let tree = engine
        .mailboxes(account)
        .await
        .map_err(runtime("the folders could not be read"))?;
    let Some(destination) = resolve_move_target(&tree, role) else {
        return Err((
            Code::Runtime,
            format!("the account has no {} folder to move to", role_word(role)),
        ));
    };
    let edit = MailEdit::move_to(key.clone(), destination);
    apply(engine, provider, account, &key, &edit, preview).await
}

/// Applies `edit` through the engine's outbox. A preview prints the edit
/// itself, the very payload the op would store; a run answers the durable
/// op and whether the provider leg took it (`applied`) or the op waits for
/// the app's drain (`queued`), the drafts verbs' own two-state answer.
async fn apply<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    key: &ProviderKey,
    edit: &MailEdit,
    preview: bool,
) -> Answer {
    if preview {
        return Ok(json!({
            "action": "edit_mail",
            "account": account.as_str(),
            "key": key.as_str(),
            "edit": serde_json::to_value(edit).unwrap_or(Value::Null),
        }));
    }
    let idempotency = mint_idempotency();
    match engine
        .edit_mail(provider, account, &idempotency, edit)
        .await
    {
        Ok(engine_api::MailEditSent::Applied(outcome)) => Ok(json!({
            "key": key.as_str(),
            "op": outcome.op.get(),
            "state": "applied",
        })),
        Ok(engine_api::MailEditSent::Queued { op, .. }) => Ok(json!({
            "key": key.as_str(),
            "op": op.get(),
            "state": "queued",
        })),
        Err(err) => Err((
            Code::Runtime,
            format!("the edit could not be queued: {err}"),
        )),
    }
}

/// A provider key from a `--key` word, or a usage error naming the fault.
fn parse_key(key: &str) -> Result<ProviderKey, (Code, String)> {
    ProviderKey::new(key.to_owned())
        .map_err(|err| (Code::Usage, format!("the --key is malformed: {err}")))
}

/// Fails loudly when `key` names no message the store holds: an edit
/// against an absent row would queue an op the server can only refuse, and
/// the assistant would claim success over mail that never moved.
async fn ensure_message(
    engine: &Engine,
    account: &AccountId,
    key: &ProviderKey,
) -> Result<(), (Code, String)> {
    let held = engine
        .mail_by_keys(account, std::slice::from_ref(key))
        .await
        .map_err(runtime("the messages could not be read"))?;
    if held.is_empty() {
        return Err((
            Code::Runtime,
            format!(
                "no message in that account carries the key {}",
                key.as_str()
            ),
        ));
    }
    Ok(())
}

/// A fresh idempotency key per edit, the engine's own contract (the outbox
/// dedups by account and key across every op state, so one key reused
/// across two edits would collapse them into one settled op).
fn mint_idempotency() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("edit:{nanos}:{seq}")
}

/// The mailbox a move to `role` lands in: the RFC 6156 SPECIAL-USE role,
/// else a conventional folder name, else (Archive only) the account's `\All`
/// mailbox. Mirrors `astra-app/src/mail_writes.rs`, its sibling in the app
/// layer; the two crates do not depend on each other, and both spellings
/// are pinned by their own tests, the closed action set's own precedent.
fn resolve_move_target(tree: &[Mailbox], role: &MailboxRole) -> Option<MailboxId> {
    tree.iter()
        .find(|mailbox| mailbox.role.as_ref() == Some(role))
        .or_else(|| {
            tree.iter()
                .find(|mailbox| name_matches_role(&mailbox.name, role))
        })
        .or_else(|| {
            (role == &MailboxRole::Archive)
                .then(|| {
                    tree.iter()
                        .find(|mailbox| mailbox.role.as_ref() == Some(&MailboxRole::All))
                })
                .flatten()
        })
        .map(|mailbox| mailbox.id.clone())
}

/// Whether `name` is a conventional folder name for `role`, matched on the
/// leaf of a hierarchical name, case-insensitively.
fn name_matches_role(name: &str, role: &MailboxRole) -> bool {
    let leaf = name.rsplit(['/', '.', '\\']).next().unwrap_or(name).trim();
    let candidates: &[&str] = match role {
        MailboxRole::Archive => &[
            "archive",
            "archives",
            "archief",
            "archieven",
            "archiv",
            "archivio",
            "archivo",
            "arkiv",
        ],
        MailboxRole::Trash => &[
            "trash",
            "deleted",
            "deleted items",
            "deleted messages",
            "bin",
            "prullenbak",
            "papierkorb",
            "corbeille",
            "cestino",
            "papelera",
        ],
        _ => return false,
    };
    candidates
        .iter()
        .any(|candidate| leaf.eq_ignore_ascii_case(candidate))
}

/// `role` as the error message's word.
fn role_word(role: &MailboxRole) -> &'static str {
    match role {
        MailboxRole::Archive => "archive",
        MailboxRole::Trash => "trash",
        _ => "folder",
    }
}

/// The runtime-error constructor the shared helpers use.
fn runtime(context: &'static str) -> impl Fn(engine_api::ApiError) -> (Code, String) + Clone {
    move |err| (Code::Runtime, format!("{context}: {err}"))
}
