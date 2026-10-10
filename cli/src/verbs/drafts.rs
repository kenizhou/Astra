// SPDX-License-Identifier: GPL-3.0-only
//! `drafts save` and `drafts discard`: the two mutating verbs, both through
//! the engine's own outbox writes under the offline provider. Nothing here
//! sends: the durable op parks for the app's drain, which is the recorded
//! second-writer seam (docs/cli.md). `--preview` prints the very draft or
//! deletion the engine call would carry, from the one builder both paths
//! share.

use std::sync::atomic::{AtomicU64, Ordering};

use engine_api::{
    AccountId, Draft, EmailAddress, Engine, MessageIdHeader, PendingOpKind, Provider, ProviderKey,
};
use serde_json::{json, Value};

use super::Answer;
use crate::envelope::Code;
use crate::verbs::DraftArgs;

/// Builds the draft the verb stores: the one builder the run and the preview
/// share, so a preview cannot drift from what an engine call would carry.
/// The `Message-ID` is minted here, before the outbox sees the draft, the
/// way the app's composer mints one per composition.
pub(crate) fn build_draft(args: &DraftArgs) -> Result<Draft, (Code, String)> {
    let message_id = mint_message_id()
        .ok_or_else(|| (Code::Runtime, "a Message-ID could not be minted".to_owned()))?;
    let draft = Draft::new(
        message_id,
        EmailAddress::new(&args.from),
        args.to.iter().map(EmailAddress::new).collect(),
        args.subject.clone(),
        args.body.clone(),
    )
    .with_cc(args.cc.iter().map(EmailAddress::new).collect());
    Ok(draft)
}

/// A fresh `Message-ID`, the same form the app mints (`drafts/build.rs`):
/// clock plus process counter, so every mint is unique and stable across the
/// saves of one composition.
fn mint_message_id() -> Option<MessageIdHeader> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    MessageIdHeader::new(format!("draft-{nanos}-{seq}@astra.local")).ok()
}

/// Stores a draft through the engine's `put_draft`. When the provider leg
/// fails, as it always does offline, the durable op is the answer: its id is
/// read back from the outbox and the state says `queued`.
pub(crate) async fn save<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    args: &DraftArgs,
    preview: bool,
) -> Answer {
    let draft = build_draft(args)?;
    if preview {
        return Ok(preview_put(account, &draft));
    }
    match engine.put_draft(provider, account, &draft, None).await {
        Ok(outcome) => Ok(json!({
            "message_id": draft.message_id.as_str(),
            "key": outcome.key.as_str(),
            "op": outcome.op.get(),
            "state": "stored",
        })),
        Err(err) => {
            let resource = format!("draft:{}", draft.message_id.as_str());
            match parked_op(engine, account, &resource, PendingOpKind::MailDraftPut).await {
                Some(op) => Ok(json!({
                    "message_id": draft.message_id.as_str(),
                    "op": op,
                    "state": "queued",
                    "detail": format!("{err}"),
                })),
                None => Err((
                    Code::Runtime,
                    format!("the draft could not be stored: {err}"),
                )),
            }
        }
    }
}

/// Removes a stored draft through the engine's `delete_draft`, with the same
/// parked-op answer the save gives when the provider leg fails offline.
pub(crate) async fn discard<P: Provider>(
    engine: &Engine,
    provider: &P,
    account: &AccountId,
    key: &str,
    preview: bool,
) -> Answer {
    let key = ProviderKey::new(key.to_owned())
        .map_err(|err| (Code::Usage, format!("the --key is malformed: {err}")))?;
    if preview {
        return Ok(json!({
            "action": "delete_draft",
            "account": account.as_str(),
            "key": key.as_str(),
        }));
    }
    match engine.delete_draft(provider, account, &key).await {
        Ok(op) => Ok(json!({
            "key": key.as_str(),
            "op": op.get(),
            "state": "removed",
        })),
        Err(err) => {
            let resource = format!("mail:{}", key.as_str());
            match parked_op(engine, account, &resource, PendingOpKind::MailDraftDelete).await {
                Some(op) => Ok(json!({
                    "key": key.as_str(),
                    "op": op,
                    "state": "queued",
                    "detail": format!("{err}"),
                })),
                None => Err((
                    Code::Runtime,
                    format!("the draft could not be discarded: {err}"),
                )),
            }
        }
    }
}

/// What `drafts save --preview` prints: the action and the whole draft the
/// engine call would carry, minus the body's text (its length stands in).
fn preview_put(account: &AccountId, draft: &Draft) -> Value {
    json!({
        "action": "put_draft",
        "account": account.as_str(),
        "draft": {
            "message_id": draft.message_id.as_str(),
            "from": draft.from.email,
            "to": draft.to.iter().map(|a| a.email.clone()).collect::<Vec<_>>(),
            "cc": draft.cc.iter().map(|a| a.email.clone()).collect::<Vec<_>>(),
            "subject": draft.subject,
            "body_chars": draft.text_body.chars().count(),
        },
    })
}

/// The id of the op a failed write parked on `resource`, if the outbox holds
/// one: the newest row of that kind on that resource.
async fn parked_op(
    engine: &Engine,
    account: &AccountId,
    resource: &str,
    kind: PendingOpKind,
) -> Option<u64> {
    engine
        .outbox(account)
        .await
        .ok()?
        .into_iter()
        .filter(|row| row.kind == Some(kind) && row.resource_key.as_str() == resource)
        .map(|row| row.id.get())
        .next_back()
}
