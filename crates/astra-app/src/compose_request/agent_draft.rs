// SPDX-License-Identifier: GPL-3.0-only
//! The agent draft seam: the compose request an assistant's `create_draft`
//! opens, the same surface the smart-reply chip's acceptance rides. The
//! agent-sidecar precedent, generalised one step: a chip prefills a reply
//! under the message it answers, an agent prefills a whole composition the
//! human then reviews, edits, and sends themselves.
//!
//! Nothing here sends. The snapshot published is a prefill, exactly the
//! shape a reply's dispatch publishes; whatever the composition eventually
//! does is a submit intent's business, and only a human presses send.

use astra_viewmodel::{ComposeKind, ComposeRequestSnapshot};
use engine_api::{AccountId, Provider};

use crate::reading::resolve;
use crate::reference::MessageRef;
use crate::App;

use super::{draft_paragraphs, quote, quote_text};

/// Why an agent draft could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenDraftError {
    /// No account is configured to compose from.
    #[error("there is no account to compose from")]
    NoAccount,
}

impl<P: Provider> App<P> {
    /// Opens an agent's draft in the composer: publishes it on the
    /// [`Surface::ComposeRequest`](crate::Surface::ComposeRequest) surface,
    /// unsent, exactly as a reply's own dispatch does. The host's composer
    /// hydrates from the snapshot on its next focus (the panel's timing, the
    /// known gap the design records).
    ///
    /// `account` names the account to compose from, or `None` for the first
    /// configured one. `reply_to`, when present, threads the draft under the
    /// original: the snapshot carries the original's own `Message-ID` and
    /// the quoted original below the agent's words, and the human's send
    /// re-resolves the original for its `References` chain. A `reply_to`
    /// that no longer resolves still opens the draft, unthreaded: the
    /// agent's words are the point, and refusing them over a message that
    /// moved would lose a draft the user asked for.
    ///
    /// # Errors
    ///
    /// [`OpenDraftError::NoAccount`] when no account is configured.
    pub async fn compose_agent_draft(
        &self,
        account: Option<&AccountId>,
        to: &[String],
        cc: &[String],
        subject: &str,
        body: &str,
        reply_to: Option<&MessageRef>,
    ) -> Result<(), OpenDraftError> {
        let handles = self.account_handles().await;
        let handle = match account {
            Some(id) => handles.iter().find(|handle| &handle.id == id),
            None => handles.first(),
        };
        let Some(handle) = handle else {
            return Err(OpenDraftError::NoAccount);
        };
        let from = handle.id.clone();

        // The original, when there is one to answer: threading header and
        // the quoted body below the agent's words.
        let mut original = None;
        if let Some(message) = reply_to {
            match resolve(&self.engine, message).await {
                Some(resolved) => original = Some(resolved),
                None => log::warn!(
                    "agent draft: the message to answer no longer resolves; the draft opens \
                     unthreaded"
                ),
            }
        }
        let fetched_body = match (reply_to, original.as_ref()) {
            (Some(message), Some(resolved)) => {
                let handle = self.account_handle(&message.account).await;
                let provider = handle
                    .as_deref()
                    .and_then(|handle| handle.providers.first());
                match provider {
                    Some(provider) => self
                        .engine
                        .message_body(provider, &message.account, resolved)
                        .await
                        .ok(),
                    None => None,
                }
            }
            _ => None,
        };

        let mut blocks = draft_paragraphs(body);
        if let Some(resolved) = &original {
            let quoted = quote_text(fetched_body.as_ref());
            if !quoted.trim().is_empty() {
                blocks.push(astra_composer::Block::Quote(quote(
                    ComposeKind::Reply,
                    resolved,
                    quoted,
                )));
            }
        }
        let snapshot = ComposeRequestSnapshot {
            kind: ComposeKind::Reply,
            from,
            to: join_addresses(to),
            cc: join_addresses(cc),
            subject: subject.to_owned(),
            in_reply_to: original.as_ref().and_then(|original| {
                original
                    .envelope
                    .message_id
                    .first()
                    .map(|id| id.as_str().to_owned())
            }),
            document: astra_composer::ComposerDocument {
                blocks,
                attachments: Vec::new(),
            },
        };
        // The submit arms re-resolve the original the request was computed
        // from to thread the outgoing reply under its chain; an unthreaded
        // draft holds no source.
        *self
            .compose_source
            .lock()
            .expect("compose-source mutex poisoned") = reply_to.cloned();
        self.compose_request.publish(Some(snapshot));
        Ok(())
    }
}

/// One recipient list as the composer's fields take it: comma-joined, the
/// empty entries and surrounding space trimmed away.
fn join_addresses(addresses: &[String]) -> String {
    addresses
        .iter()
        .map(|address| address.trim())
        .filter(|address| !address.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "agent_draft_tests.rs"]
mod tests;
