// SPDX-License-Identifier: GPL-3.0-only
//! The autosuggest read: the composer's recipient completion over the
//! engine's ranked suggestions, passed through untouched.
//!
//! The caller hands the **token** (the text after the last comma in the
//! field), never the field, and the read forwards it verbatim: splitting or
//! trimming here would be assuming the field's shape, which is the host's
//! business. The ranking is the engine's (an exact address, then a prefix,
//! then a substring, each tier settled by saved-ness, recency, and counts
//! over sent-mail history beside the cards); this path re-ranks nothing and
//! hides nothing, because a host that trusted an ordering we had reordered
//! would seed its own undo. A blank token returns nothing: the empty field
//! is not a search for everyone.

use engine_api::{Provider, RecipientSuggestion};

use crate::App;

impl<P: Provider> App<P> {
    /// The ranked suggestions for one completion token: the engine's order,
    /// the engine's limit discipline (capped at its own maximum), at most
    /// `limit` entries. Errors are logged and answered with nothing, because
    /// a completion list that silently emptied and one that failed to load
    /// look the same to the user typing.
    pub async fn recipient_suggestions(
        &self,
        token: &str,
        limit: usize,
    ) -> Vec<RecipientSuggestion> {
        if token.trim().is_empty() {
            return Vec::new();
        }
        match self.engine.recipient_suggestions(token, limit).await {
            Ok(suggestions) => suggestions.suggestions,
            Err(err) => {
                log::warn!("people: the suggestion read failed: {err}");
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
#[path = "../people_suggest_tests.rs"]
mod tests;
