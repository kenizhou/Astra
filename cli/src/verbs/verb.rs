// SPDX-License-Identifier: GPL-3.0-only
//! The verb vocabulary: every invocation's parsed shape with its
//! arguments, the ledger's spelling of each, and the mutating set
//! `--preview` binds. A module of its own so the dispatch stays under
//! the gate's file-length cap.

use super::completions::Shell;
use super::{DraftArgs, SnippetArgs};

/// `keys import`'s arguments, with the material resolved at parse time so
/// the audit preview can count it and execution cannot fail reading it,
/// the drafts precedent. Public material only: the store refuses a
/// secret-bearing input.
#[derive(PartialEq)]
pub(crate) struct KeyImportArgs {
    /// The account id from the index.
    pub(crate) account: String,
    /// The cert material, armored or binary.
    pub(crate) bytes: Vec<u8>,
    /// Which source the material came from, for the audit preview.
    pub(crate) source: &'static str,
    /// The pinned standard, when the caller named one.
    pub(crate) standard: Option<String>,
}

/// `keys promote`'s arguments: the staged key to accept and the trust
/// decision the acceptance records beside it.
#[derive(PartialEq)]
pub(crate) struct KeyPromoteArgs {
    /// The account id from the index.
    pub(crate) account: String,
    /// The key's standard, `openpgp` or `smime`.
    pub(crate) standard: String,
    /// The staged key's fingerprint.
    pub(crate) fingerprint: String,
    /// The address the acceptance binds, when the caller names one.
    pub(crate) email: Option<String>,
    /// Whether the fingerprint was confirmed out of band.
    pub(crate) verified: bool,
}

/// `keys reject`'s arguments: the staging row to remove and the key the
/// rejected decision names.
#[derive(PartialEq)]
pub(crate) struct KeyRejectArgs {
    /// The account id from the index.
    pub(crate) account: String,
    /// The staging row's id.
    pub(crate) id: i64,
    /// The key's standard, `openpgp` or `smime`.
    pub(crate) standard: String,
    /// The staged key's fingerprint.
    pub(crate) fingerprint: String,
    /// The address the rejection binds, when the caller names one.
    pub(crate) email: Option<String>,
}

/// The verbs, with their arguments: the mail reads and writes, the rules
/// verbs over the rules store, the AI reads over the results,
/// draft-pool, cache, profile, and document tables, and the shell
/// completion generator.
#[derive(PartialEq)]
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
    /// Search mail, one page of hits newest first.
    MessagesSearch {
        /// The search DSL query.
        query: String,
        /// The account id from the index, when one account is named.
        account: Option<String>,
        /// The folder key narrowing within the named account.
        folder: Option<String>,
        /// How many hits to answer with.
        limit: usize,
        /// How many hits to skip, for paging.
        offset: usize,
    },
    /// Mark one message read or unread through the engine's outbox.
    MessagesMarkRead {
        /// The account id from the index.
        account: String,
        /// The message's provider key.
        key: String,
        /// Whether the `--unread` flag asked for the unread direction.
        unread: bool,
    },
    /// Flag one message or clear its flag through the engine's outbox.
    MessagesSetFlagged {
        /// The account id from the index.
        account: String,
        /// The message's provider key.
        key: String,
        /// Whether the `--clear` flag asked for the clearing direction.
        clear: bool,
    },
    /// Move one message to the account's archive.
    MessagesArchive {
        /// The account id from the index.
        account: String,
        /// The message's provider key.
        key: String,
    },
    /// Move one message to the account's trash.
    MessagesTrash {
        /// The account id from the index.
        account: String,
        /// The message's provider key.
        key: String,
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
    /// List one account's keys across both standards, public view only.
    KeysList {
        /// The account id from the index.
        account: String,
    },
    /// List one account's staged collected keys, newest first.
    KeysCollected {
        /// The account id from the index.
        account: String,
    },
    /// Import a peer's public cert into one account's key store.
    KeysImport {
        /// The assembled arguments, cert material included.
        args: KeyImportArgs,
    },
    /// Promote a staged key and record the acceptance that makes it
    /// send-usable.
    KeysPromote {
        /// The assembled arguments.
        args: KeyPromoteArgs,
    },
    /// Reject a staged key and record the refusal.
    KeysReject {
        /// The assembled arguments.
        args: KeyRejectArgs,
    },
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
    /// Generate one shell's completion script from the verb vocabulary.
    Completions {
        /// The shell whose completion idiom the script targets.
        shell: Shell,
    },
}

impl Verb {
    /// The ledger's verb spelling.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::AccountsList => "accounts list",
            Self::FoldersList { .. } => "folders list",
            Self::MessagesList { .. } => "messages list",
            Self::MessagesRead { .. } => "messages read",
            Self::MessagesSearch { .. } => "messages search",
            Self::MessagesMarkRead { .. } => "messages mark-read",
            Self::MessagesSetFlagged { .. } => "messages set-flagged",
            Self::MessagesArchive { .. } => "messages archive",
            Self::MessagesTrash { .. } => "messages trash",
            Self::DraftsSave { .. } => "drafts save",
            Self::DraftsDiscard { .. } => "drafts discard",
            Self::RulesList => "rules list",
            Self::RulesCreate { .. } => "rules create",
            Self::RulesDelete { .. } => "rules delete",
            Self::KnowledgeList { .. } => "knowledge list",
            Self::KnowledgeForget { .. } => "knowledge forget",
            Self::KnowledgeExtract => "knowledge extract",
            Self::KeysList { .. } => "keys list",
            Self::KeysCollected { .. } => "keys collected",
            Self::KeysImport { .. } => "keys import",
            Self::KeysPromote { .. } => "keys promote",
            Self::KeysReject { .. } => "keys reject",
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
            Self::Completions { .. } => "completions",
        }
    }

    /// Whether this verb changes anything, and so ships `--preview`.
    pub(crate) fn mutating(&self) -> bool {
        matches!(
            self,
            Self::DraftsSave { .. }
                | Self::DraftsDiscard { .. }
                | Self::MessagesMarkRead { .. }
                | Self::MessagesSetFlagged { .. }
                | Self::MessagesArchive { .. }
                | Self::MessagesTrash { .. }
                | Self::RulesCreate { .. }
                | Self::RulesDelete { .. }
                | Self::KnowledgeForget { .. }
                | Self::SnippetsCreate { .. }
                | Self::SnippetsDelete { .. }
                | Self::KeysImport { .. }
                | Self::KeysPromote { .. }
                | Self::KeysReject { .. }
        )
    }
}
