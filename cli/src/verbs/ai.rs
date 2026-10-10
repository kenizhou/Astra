// SPDX-License-Identifier: GPL-3.0-only
//! `ai cold` and `ai results`: the stored-verdict reads over the results
//! store the incoming pass fills (migration 0003). The CLI dials no
//! model: a verdict exists here only because a pass recorded it, and an
//! unjudged message answers null, never a guess.

use astra_ai::AiStore;
use engine_api::AccountId;
use serde_json::{json, Value};

use super::Answer;
use crate::envelope::Code;

/// The cold stage's word in the results table.
const COLD_STAGE: &str = "cold";

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
