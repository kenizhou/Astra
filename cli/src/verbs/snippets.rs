// SPDX-License-Identifier: GPL-3.0-only
//! `snippets list`, `snippets create`, `snippets delete`, and the refused
//! `snippets search`: the snippet library's verbs over the store the
//! composer's picker shares (migration 0008, the same sqlite file the
//! audit ledger lives in). The store verbs answer without the engine: no
//! mail is read and no provider is dialled.
//!
//! The search refusal, recorded here and in the CLI contract: ranked
//! search is one model completion, the model seam lives in the desktop
//! host, and the CLI holds no model connection and never will. The
//! spelling parses into a refusal that names the rule and points where
//! the ranking runs, so `snippets list` stays the agent's own read.

use astra_ai::{AiStore, NewSnippet, SnippetId};
use serde_json::{json, Value};

use super::{Answer, SnippetArgs};
use crate::envelope::Code;

/// The search refusal's message, naming the rule it enforces: the
/// model-seam parallel of the send-class refusal.
const SEARCH_REFUSAL: &str = "snippet search is model work: the app runs \
     the ranked search over the library with its substring fallback; the \
     astra-cli dials no model, so the verb refuses. `snippets list` reads \
     the library the agent filters itself";

/// Lists every snippet, the most recently touched first (the store's own
/// order, the picker's listing read). Bodies ride whole: a snippet's body
/// is exactly what an insert places, and the verb exists to read it.
pub(crate) fn list(store: &AiStore) -> Answer {
    let rows = store.list_snippets().map_err(|err| {
        (
            Code::Runtime,
            format!("the snippet store could not be read: {err}"),
        )
    })?;
    let snippets: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id.as_str(),
                "title": row.title,
                "body": row.body,
                "created_at": row.created_at,
                "updated_at": row.updated_at,
            })
        })
        .collect();
    Ok(json!({ "snippets": snippets }))
}

/// Stores one snippet with a freshly minted id. `--preview` prints the
/// exact snippet the store would take; the run prints it back with the
/// store's stamps beside `state: created`.
pub(crate) fn create(store: &AiStore, args: &SnippetArgs, preview: bool) -> Answer {
    let id = SnippetId::generate();
    let snippet = json!({
        "id": id.as_str(),
        "title": args.title,
        "body": args.body,
    });
    if preview {
        return Ok(json!({ "action": "create_snippet", "snippet": snippet }));
    }
    let landed = store
        .insert_snippet(&NewSnippet {
            id,
            title: args.title.clone(),
            body: args.body.clone(),
        })
        .map_err(|err| {
            (
                Code::Runtime,
                format!("the snippet could not be stored: {err}"),
            )
        })?;
    Ok(json!({
        "snippet": {
            "id": landed.id.as_str(),
            "title": landed.title,
            "body": landed.body,
            "created_at": landed.created_at,
            "updated_at": landed.updated_at,
        },
        "state": "created",
    }))
}

/// Deletes one snippet by id. `--preview` names the exact deletion; a run
/// against an id no snippet carries is a runtime error naming it.
pub(crate) fn delete(store: &AiStore, id: &str, preview: bool) -> Answer {
    if preview {
        return Ok(json!({ "action": "delete_snippet", "id": id }));
    }
    match store.delete_snippet(&SnippetId::new(id.to_owned())) {
        Ok(true) => Ok(json!({ "id": id, "state": "deleted" })),
        Ok(false) => Err((
            Code::Runtime,
            format!("no snippet carries id {id}: nothing was deleted"),
        )),
        Err(err) => Err((
            Code::Runtime,
            format!("the snippet could not be deleted: {err}"),
        )),
    }
}

/// Refuses the search: the spelling parses so the ledger names it, but no
/// ranking can run here. Ranking the library for a search is a model
/// completion, the model seam lives in the desktop host, and the CLI
/// holds no model connection and never will, so the verb refuses with the
/// pointer rather than fall back silently.
pub(crate) fn search() -> Answer {
    Err((Code::Refused, SEARCH_REFUSAL.to_owned()))
}
