// SPDX-License-Identifier: GPL-3.0-only
//! The `ai` verbs: the stored reads over the results store the incoming
//! pass fills (migration 0003) and the reply-drafting tables (the draft
//! pool and style profiles of 0004, the summarise cache of 0005), plus
//! the one refused spelling. The CLI dials no model: a verdict, draft,
//! summary, or profile exists here only because the app's pass or pull
//! recorded it, an unjudged message answers null, never a guess, and the
//! style rebuild refuses under the same rule rather than infer.

use astra_ai::AiStore;
use engine_api::AccountId;
use serde_json::{json, Value};

use super::Answer;
use crate::envelope::Code;

/// The cold stage's word in the results table.
const COLD_STAGE: &str = "cold";

/// The rebuild refusal's message, naming the rule it enforces: the
/// model-seam parallel of the send-class refusal.
const STYLE_REBUILD_REFUSAL: &str = "a style rebuild is model work: the app \
     infers the note from Sent mail and rebuilds the profile; the astra-cli \
     dials no model, so the verb refuses. `ai style` reads what the app built";

/// Answers each key's stored cold verdict: the newest `cold` stage row's
/// word with the correlation that judged it and the store's stamp, or
/// null when no pass has judged the message.
pub(crate) fn cold(store: &AiStore, account: &AccountId, keys: &[String]) -> Answer {
    let mut messages = Vec::with_capacity(keys.len());
    for key in keys {
        let rows = rows_for(store, account, key)?;
        let verdict = rows.iter().find(|row| row.stage == COLD_STAGE).map(|row| {
            json!({
                "verdict": row.verdict,
                "correlation": row.correlation.as_str(),
                "recorded_at": row.created_at,
            })
        });
        messages.push(json!({ "key": key, "cold": verdict }));
    }
    Ok(json!({ "messages": messages }))
}

/// Lists each key's stored stage rows, newest first (the store's own
/// order): every verdict the pass's stages recorded, with the rule id and
/// the category each row carries.
pub(crate) fn results(store: &AiStore, account: &AccountId, keys: &[String]) -> Answer {
    let mut messages = Vec::with_capacity(keys.len());
    for key in keys {
        let rows = rows_for(store, account, key)?;
        let results = rows
            .iter()
            .map(|row| {
                json!({
                    "stage": row.stage,
                    "rule_id": row.rule_id.as_ref().map(astra_ai::RuleId::as_str),
                    "verdict": row.verdict,
                    "category": row.category,
                    "correlation": row.correlation.as_str(),
                    "recorded_at": row.created_at,
                })
            })
            .collect::<Vec<Value>>();
        messages.push(json!({ "key": key, "results": results }));
    }
    Ok(json!({ "messages": messages }))
}

/// Answers each key's stored draft pool rows: every generation turn the
/// pool recorded for the message, newest first (the store's own order),
/// drafted rows carrying the reply and its confidence and skipped rows (a
/// failed or unparseable generation) carrying neither. The reply rides
/// whole: it is model-generated text and the verb exists to read it, the
/// same whole-body discipline `messages read` carries; the ledger's
/// redaction is the argument preview's, never the answer's.
pub(crate) fn drafts(store: &AiStore, account: &AccountId, keys: &[String]) -> Answer {
    let mut messages = Vec::with_capacity(keys.len());
    for key in keys {
        let rows = store
            .draft_pool_for_message(account.as_str(), key)
            .map_err(|err| {
                (
                    Code::Runtime,
                    format!("the draft pool could not be read: {err}"),
                )
            })?;
        let drafts = rows
            .iter()
            .map(|row| {
                json!({
                    "correlation": row.correlation.as_str(),
                    "reply": row.reply,
                    "confidence": row.confidence,
                    "skipped": row.reply.is_none(),
                    "created_at": row.created_at,
                })
            })
            .collect::<Vec<Value>>();
        messages.push(json!({ "key": key, "drafts": drafts }));
    }
    Ok(json!({ "messages": messages }))
}

/// Answers each key's cached summary row: the stored summary with the
/// body hash it was cached under and the store's stamp, or null when no
/// summarise has cached the message. Whether the row still hits is the
/// app's body-hash check at serving time; this read makes no claim about
/// it and reads no mail to judge.
pub(crate) fn summary(store: &AiStore, account: &AccountId, keys: &[String]) -> Answer {
    let mut messages = Vec::with_capacity(keys.len());
    for key in keys {
        let cached = store
            .summarise_cache_for_message(account.as_str(), key)
            .map_err(|err| {
                (
                    Code::Runtime,
                    format!("the summarise cache could not be read: {err}"),
                )
            })?;
        let summary = cached.map(|row| {
            json!({
                "summary": row.summary,
                "body_hash": row.body_hash,
                "created_at": row.created_at,
            })
        });
        messages.push(json!({ "key": key, "summary": summary }));
    }
    Ok(json!({ "messages": messages }))
}

/// Answers the account's stored style profile: the StyleInfer note and
/// the store-stamped build time, or null when no profile was built (no
/// Sent mail was ever inferred from).
pub(crate) fn style(store: &AiStore, account: &AccountId) -> Answer {
    let profile = store
        .style_profile(account.as_str())
        .map_err(|err| {
            (
                Code::Runtime,
                format!("the style profile could not be read: {err}"),
            )
        })?
        .map(|profile| {
            json!({
                "note": profile.note,
                "built_at": profile.built_at,
            })
        });
    Ok(json!({ "style": profile }))
}

/// Refuses the style rebuild: the spelling parses so the ledger names it,
/// but no rebuild can run here. Inferring the note from Sent mail is a
/// model completion, the model seam lives in the desktop host, and the
/// CLI holds no model connection and never will, so the verb refuses
/// with the pointer rather than pretend.
pub(crate) fn style_rebuild() -> Answer {
    Err((Code::Refused, STYLE_REBUILD_REFUSAL.to_owned()))
}

/// One message's stored rows, the store's newest-first read both verbs
/// share.
fn rows_for(
    store: &AiStore,
    account: &AccountId,
    key: &str,
) -> Result<Vec<astra_ai::AiResultRow>, (Code, String)> {
    store
        .ai_results_for_message(account.as_str(), key)
        .map_err(|err| {
            (
                Code::Runtime,
                format!("the results store could not be read: {err}"),
            )
        })
}
