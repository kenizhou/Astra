// SPDX-License-Identifier: GPL-3.0-only
//! `knowledge list`, `knowledge forget`, and the refused `knowledge
//! extract`: the knowledge verbs over the store the app's extraction
//! fills (migration 0007, the same sqlite file the audit ledger lives
//! in). The store verbs answer without the engine: no mail is read and
//! no provider is dialled.
//!
//! The extraction refusal, recorded here and in the CLI contract: the CLI
//! never dials a model. Extraction is a batch completion over the
//! account's recent mail, and the model seam lives in the desktop host
//! the app assembles; the spelling parses into a refusal that names the
//! rule and points where the run happens, so `knowledge list` reads what
//! the app landed.

use astra_ai::{AiStore, KnowledgeId};
use engine_api::AccountId;
use serde_json::{json, Value};

use super::Answer;
use crate::envelope::Code;

/// The extraction refusal's message, naming the rule it enforces: the
/// model-seam parallel of the send-class refusal.
const EXTRACT_REFUSAL: &str = "knowledge extraction is model work: the app \
     runs the batch over the account's recent mail and lands the entries; \
     the astra-cli dials no model, so the verb refuses. `knowledge list` \
     reads what the app landed";

/// Lists every knowledge entry the account holds, newest first (the
/// store's own order), each row carrying the source pair that tracks the
/// message it was extracted from. The entry's text rides whole: it is
/// model-derived text and the verb exists to read it.
pub(crate) fn list(store: &AiStore, account: &AccountId) -> Answer {
    let rows = store
        .knowledge_for_account(account.as_str())
        .map_err(|err| {
            (
                Code::Runtime,
                format!("the knowledge store could not be read: {err}"),
            )
        })?;
    let entries: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id.as_str(),
                "kind": row.kind,
                "text": row.text,
                "source_account": row.source_message_account,
                "source_key": row.source_message_key,
                "created_at": row.created_at,
            })
        })
        .collect();
    Ok(json!({ "entries": entries }))
}

/// Forgets one knowledge entry by id. Entries never auto-delete, so this
/// is the only exit. `--preview` names the exact forgetting; a run
/// against an id no entry carries is a runtime error naming it.
pub(crate) fn forget(store: &AiStore, id: &str, preview: bool) -> Answer {
    if preview {
        return Ok(json!({ "action": "forget_knowledge", "id": id }));
    }
    match store.forget_knowledge(&KnowledgeId::new(id.to_owned())) {
        Ok(true) => Ok(json!({ "id": id, "state": "forgotten" })),
        Ok(false) => Err((
            Code::Runtime,
            format!("no knowledge entry carries id {id}: nothing was forgotten"),
        )),
        Err(err) => Err((
            Code::Runtime,
            format!("the knowledge entry could not be forgotten: {err}"),
        )),
    }
}

/// Refuses the extraction: the spelling parses so the ledger names it,
/// but no batch can run here. Extracting knowledge from mail is a model
/// completion, the model seam lives in the desktop host, and the CLI
/// holds no model connection and never will, so the verb refuses with
/// the pointer rather than pretend.
pub(crate) fn extract() -> Answer {
    Err((Code::Refused, EXTRACT_REFUSAL.to_owned()))
}
