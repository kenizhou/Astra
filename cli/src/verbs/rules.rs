// SPDX-License-Identifier: GPL-3.0-only
//! `rules list`, `rules create`, and `rules delete`: the rules verbs over
//! the rules store the app's incoming pass shares (migration 0002, the
//! same sqlite file the audit ledger lives in). The store verbs answer
//! without the engine: no mail is read and no provider is dialled.
//!
//! The create seam, recorded here and in the CLI contract: `rules create`
//! takes an explicit condition sentence and generates nothing. The
//! background client and its `prompt_to_rules` path live in the app the
//! desktop host assembles; the CLI holds no model connection and never
//! will (the offline-provider contract). The agent composes the condition
//! itself, through its own model, and the verb stores what it is given.

use astra_ai::{AiStore, NewRule, RuleId, RuleRow};
use serde_json::{json, Value};

use super::Answer;
use crate::envelope::Code;

/// The closed action set as the CLI spells it: the wire words the store's
/// actions payloads carry. The app layer's `Action` set is the same two
/// words; a change to either lands in both places with the pins that tie
/// them.
pub(crate) const CLOSED_ACTIONS: &[&str] = &["mark_read", "set_flagged"];

/// Lists the stored rules, the actions payload parsed back to its wire
/// words; a stored payload outside the closed set fails the verb loudly,
/// naming the rule, the same contract the app's load carries.
pub(crate) fn list(store: &AiStore) -> Answer {
    let rows = store.list_rules().map_err(|err| {
        (
            Code::Runtime,
            format!("the rules store could not be read: {err}"),
        )
    })?;
    let mut rules = Vec::with_capacity(rows.len());
    for row in rows {
        rules.push(row_json(&row)?);
    }
    Ok(json!({ "rules": rules }))
}

/// Stores one rule with a freshly minted id, enabled. `--preview` prints
/// the exact rule the store would take; the run prints it back beside
/// `state: created`.
pub(crate) fn create(
    store: &AiStore,
    name: &str,
    condition: &str,
    actions: &[String],
    preview: bool,
) -> Answer {
    let id = RuleId::generate();
    let rule = json!({
        "id": id.as_str(),
        "name": name,
        "condition": condition,
        "actions": actions,
        "enabled": true,
    });
    if preview {
        return Ok(json!({ "action": "create_rule", "rule": rule }));
    }
    let payload = serde_json::to_string(actions).expect("the closed set serialises");
    let new = NewRule {
        id,
        name: name.to_owned(),
        condition: condition.to_owned(),
        actions: payload,
        enabled: true,
    };
    store.create_rule(&new).map_err(|err| {
        (
            Code::Runtime,
            format!("the rule could not be stored: {err}"),
        )
    })?;
    Ok(json!({ "rule": rule, "state": "created" }))
}

/// Deletes one rule by id. `--preview` names the exact deletion; a run
/// against an id no rule carries is a runtime error naming it.
pub(crate) fn delete(store: &AiStore, id: &str, preview: bool) -> Answer {
    if preview {
        return Ok(json!({ "action": "delete_rule", "id": id }));
    }
    match store.delete_rule(&RuleId::new(id.to_owned())) {
        Ok(true) => Ok(json!({ "id": id, "state": "deleted" })),
        Ok(false) => Err((
            Code::Runtime,
            format!("no rule carries id {id}: nothing was deleted"),
        )),
        Err(err) => Err((
            Code::Runtime,
            format!("the rule could not be deleted: {err}"),
        )),
    }
}

/// One stored row as the verb's answer carries it: the whole rule, with
/// the actions payload parsed to its wire words. A payload that is not a
/// JSON array of the closed set is a runtime error naming the rule and
/// the payload, the loud rejection the app's load carries.
fn row_json(row: &RuleRow) -> Result<Value, (Code, String)> {
    let actions: Vec<String> = serde_json::from_str(&row.actions).map_err(|_| {
        (
            Code::Runtime,
            format!(
                "rule {} carries actions outside the closed set: {}",
                row.id, row.actions
            ),
        )
    })?;
    for action in &actions {
        if !CLOSED_ACTIONS.contains(&action.as_str()) {
            return Err((
                Code::Runtime,
                format!(
                    "rule {} carries actions outside the closed set: {action}",
                    row.id
                ),
            ));
        }
    }
    Ok(json!({
        "id": row.id.as_str(),
        "name": row.name,
        "condition": row.condition,
        "actions": actions,
        "enabled": row.enabled,
        "created_at": row.created_at,
    }))
}
