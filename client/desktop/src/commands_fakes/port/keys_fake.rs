// SPDX-License-Identifier: GPL-3.0-only
//! The key import and collected-review fake's state: the preset answers and
//! the recorded asks, split out of the port module so that file stays under
//! the line cap beside the trait impl that serves them. The passphrase is
//! never recorded: it stays in-process, so only the account and the byte count
//! of an import cross into the fake. Collected keys are public material; the
//! review records the fields the command hands through, never key bytes.

use astra_ai::{KnowledgeId, SnippetId};
use astra_app::{KnowledgeView, Snippet};

use crate::commands_fakes::FakeApp;

/// The key import and collected-review fake's state: the preset answers and
/// the recorded asks.
#[derive(Default)]
pub(crate) struct KeysFake {
    /// The import's preset answer; `None` refuses as unseeded.
    pub(crate) import: Option<Result<astra_crypto_store::CryptoKeyRow, String>>,
    /// The (account, byte count) pairs the import was asked for.
    pub(crate) import_into: Vec<(String, usize)>,
    /// The list's preset answer; `None` answers an empty list.
    pub(crate) list: Option<Result<Vec<astra_crypto_store::CryptoKeyRow>, String>>,
    /// The accounts the list was asked for.
    pub(crate) list_into: Vec<String>,
    /// The collected list's preset answer; `None` answers an empty list.
    pub(crate) collected: Option<Result<Vec<astra_crypto_store::CollectedKeyRow>, String>>,
    /// The accounts the collected list was asked for.
    pub(crate) collected_into: Vec<String>,
    /// The promote's preset answer; `None` refuses as unseeded.
    pub(crate) promote: Option<Result<astra_crypto_store::CryptoKeyRow, String>>,
    /// The (account, standard, fingerprint, email, verified) tuples the promote
    /// was asked for.
    pub(crate) promote_into: Vec<(String, String, String, Option<String>, bool)>,
    /// The reject's preset answer; `None` answers `Ok(())`.
    pub(crate) reject: Option<Result<(), String>>,
    /// The (account, id, standard, fingerprint, email) tuples the reject was
    /// asked for.
    pub(crate) reject_into: Vec<(String, i64, String, String, Option<String>)>,
}

impl FakeApp {
    /// Seeds the fake's knowledge listing with one entry, minted around
    /// `id`, so the listing and the forget have a row to round trip.
    pub(crate) fn holding_knowledge(&self, id: &str, kind: &str, text: &str) {
        self.assist.lock().unwrap().knowledge.push(KnowledgeView {
            id: KnowledgeId::new(id.to_owned()),
            kind: kind.to_owned(),
            text: text.to_owned(),
            source_account: "work".to_owned(),
            source_key: "imap:v1:u42".to_owned(),
            created_at: 1,
        });
    }

    /// Seeds the fake's snippet answer with one snippet, minted around
    /// `id`, so the listing and the search have rows to answer.
    pub(crate) fn holding_snippet(&self, id: &str, title: &str, body: &str) {
        self.assist.lock().unwrap().snippets.push(Snippet {
            id: SnippetId::new(id.to_owned()),
            title: title.to_owned(),
            body: body.to_owned(),
            created_at: 1,
            updated_at: 1,
        });
    }
}
