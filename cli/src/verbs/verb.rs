// SPDX-License-Identifier: GPL-3.0-only
//! The verb vocabulary: every invocation's parsed shape with its
//! arguments, the ledger's spelling of each, and the mutating set
//! `--preview` binds. A module of its own so the dispatch stays under
//! the gate's file-length cap.

use super::{DraftArgs, SnippetArgs};

/// The verbs, with their arguments: the mail reads and writes, the rules
/// verbs over the rules store, and the AI reads over the results,
/// draft-pool, cache, profile, and document tables.
pub(crate) enum Verb {
    /// List the account index's ids.
    AccountsList,
    /// List one account's folders.
    FoldersList {
        /// The account id from the index.
        account: String,
    },
    /// List messages, newest first, optionally one folder's.
    MessagesList {
        /// The account id from the index.
        account: String,
        /// The folder's mailbox key, when one folder is asked for.
        folder: Option<String>,
        /// How many rows to answer with.
        limit: usize,
    },
    /// Read whole messages by provider key.
    MessagesRead {
        /// The account id from the index.
        account: String,
        /// The provider keys to resolve.
        keys: Vec<String>,
    },
    /// Store a draft through the engine's outbox.
    DraftsSave {
        /// The assembled arguments, body text included.
        args: DraftArgs,
    },
    /// Remove a stored draft through the engine's outbox.
    DraftsDiscard {
        /// The account id from the index.
        account: String,
        /// The stored draft's provider key.
        key: String,
    },
    /// List the stored rules.
    RulesList,
    /// Store one rule, explicit condition and actions, no generation.
    RulesCreate {
        /// The rule's short name.
        name: String,
        /// The natural language condition sentence.
        condition: String,
        /// The actions as their closed-set wire words.
        actions: Vec<String>,
    },
    /// Delete one stored rule by id.
    RulesDelete {
        /// The stored rule's id.
        id: String,
    },
    /// List one account's knowledge entries, newest first.
    KnowledgeList {
        /// The account id from the index.
        account: String,
    },
    /// Forget one stored knowledge entry by id.
    KnowledgeForget {
        /// The entry's id.
        id: String,
    },
    /// The refused extraction: the model seam lives in the app.
    KnowledgeExtract,
    /// List the snippet library, the most recently touched first.
    SnippetsList,
    /// Store one snippet, title and body text included.
    SnippetsCreate {
        /// The assembled arguments, body text included.
        args: SnippetArgs,
    },
    /// Delete one stored snippet by id.
    SnippetsDelete {
        /// The snippet's id.
        id: String,
    },
    /// The refused search: the model seam lives in the app.
    SnippetsSearch,
    /// Read the stored cold verdicts for messages, never a live call.
    AiCold {
        /// The account id from the index.
        account: String,
        /// The provider keys to read verdicts for.
        keys: Vec<String>,
    },
    /// List the stored stage rows for messages.
    AiResults {
        /// The account id from the index.
        account: String,
        /// The provider keys to list rows for.
        keys: Vec<String>,
    },
    /// List the stored draft pool rows for messages.
    AiDrafts {
        /// The account id from the index.
        account: String,
        /// The provider keys to list pool rows for.
        keys: Vec<String>,
    },
    /// Answer each key's cached summary row, when one is stored.
    AiSummary {
        /// The account id from the index.
        account: String,
        /// The provider keys to read cached summaries for.
        keys: Vec<String>,
    },
    /// Read the account's stored style profile.
    AiStyle {
        /// The account id from the index.
        account: String,
    },
    /// The refused style rebuild: the model seam lives in the app.
    AiStyleRebuild,
    /// Read the account's latest stored digest.
    AiDigest {
        /// The account id from the index.
        account: String,
    },
    /// Read the account's latest stored analytics report.
    AiReport {
        /// The account id from the index.
        account: String,
    },
    /// The refused digest generation: the model seam lives in the app.
    AiDigestGenerate,
    /// The refused report generation: the model seam lives in the app.
    AiReportGenerate,
}

impl Verb {
    /// The ledger's verb spelling.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::AccountsList => "accounts list",
            Self::FoldersList { .. } => "folders list",
            Self::MessagesList { .. } => "messages list",
            Self::MessagesRead { .. } => "messages read",
            Self::DraftsSave { .. } => "drafts save",
            Self::DraftsDiscard { .. } => "drafts discard",
            Self::RulesList => "rules list",
            Self::RulesCreate { .. } => "rules create",
            Self::RulesDelete { .. } => "rules delete",
            Self::KnowledgeList { .. } => "knowledge list",
            Self::KnowledgeForget { .. } => "knowledge forget",
            Self::KnowledgeExtract => "knowledge extract",
            Self::SnippetsList => "snippets list",
            Self::SnippetsCreate { .. } => "snippets create",
            Self::SnippetsDelete { .. } => "snippets delete",
            Self::SnippetsSearch => "snippets search",
            Self::AiCold { .. } => "ai cold",
            Self::AiResults { .. } => "ai results",
            Self::AiDrafts { .. } => "ai drafts",
            Self::AiSummary { .. } => "ai summary",
            Self::AiStyle { .. } => "ai style",
            Self::AiStyleRebuild => "ai style rebuild",
            Self::AiDigest { .. } => "ai digest",
            Self::AiReport { .. } => "ai report",
            Self::AiDigestGenerate => "ai digest generate",
            Self::AiReportGenerate => "ai report generate",
        }
    }

    /// Whether this verb changes anything, and so ships `--preview`.
    pub(crate) fn mutating(&self) -> bool {
        matches!(
            self,
            Self::DraftsSave { .. }
                | Self::DraftsDiscard { .. }
                | Self::RulesCreate { .. }
                | Self::RulesDelete { .. }
                | Self::KnowledgeForget { .. }
                | Self::SnippetsCreate { .. }
                | Self::SnippetsDelete { .. }
        )
    }
}
