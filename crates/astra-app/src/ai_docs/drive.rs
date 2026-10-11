// SPDX-License-Identifier: GPL-3.0-only
//! The AI documents host pulls: the A4 services' app-facing halves, the
//! reads and actions the desktop commands ride. The generation pull
//! collects the window's mail newest first (the engine's newest-first
//! window read, each message resolved with its plain body the style
//! collection's own fetch, a row whose body cannot be fetched skipped
//! whole), builds the stored categorisation from the results rows the
//! incoming pass recorded, and hands both to the digest or report
//! service under a correlation minted for the pull's own turn: ONE
//! generation, then the document store's replace-shaped put. The latest
//! read serves the store's newest row of one kind, and the split map is
//! pure derivation over the stored categorisation: no model call at
//! all. Without the AI bundle the generations refuse with the typed
//! no-bundle failure and the reads answer their quiet empty.

use astra_ai::{AiDocumentKind, AiStore, CorrelationId, NewAiDocument};
use engine_api::{AccountId, MailListRow, MailboxId, MailboxRole, Provider, ProviderKey};

use super::{
    clamped_days, generate_digest, generate_report, split_map as derive_split, DigestError,
    DocMessage, ReportError, SplitSide, StoredCategories,
};
use crate::ai_results::AiStage;
use crate::reference::MessageRef;
use crate::App;

/// The days a generation window covers when the caller named none: the
/// spec's own default, the one number the command boundary and any
/// direct caller share.
pub const DOCUMENT_WINDOW_DEFAULT_DAYS: u32 = 7;

/// How many of the account's newest messages one generation window
/// reads at most, whatever the days asked: the pinned cost bound that
/// keeps a thirty-one-day window's body fetches finite. The window is
/// the read's newest rows filtered to the period, so a busy mailbox
/// digests its newest mail within the window rather than every row the
/// period spans.
pub const DOCUMENT_WINDOW_READ_LIMIT: usize = 200;

/// One generated document as the host command answers it: the period
/// the window covered, the content the one generation wrote, and
/// whether the put replaced a row the same period already held. The
/// digest and the report answer the same shape, their content each
/// kind's own text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DigestView {
    /// The stored window's start (unix seconds).
    pub period_start: i64,
    /// The stored window's end (unix seconds), inclusive.
    pub period_end: i64,
    /// The document's content, the one generation's output.
    pub content: String,
    /// Whether the store already held a document of this kind over the
    /// same period, so this generation replaced it.
    pub regenerated: bool,
}

/// One stored document as the latest read answers it: the fields the
/// panel renders, none of the store's own internals.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DocumentView {
    /// The document's kind, its snake_case wire word.
    pub kind: AiDocumentKind,
    /// The stored window's start (unix seconds).
    pub period_start: i64,
    /// The stored window's end (unix seconds), inclusive.
    pub period_end: i64,
    /// The document's content.
    pub content: String,
    /// The store's creation stamp (unix seconds), fresh on every
    /// replacement behind the key.
    pub created_at: i64,
}

/// One message's split entry as the host serves it: the message's
/// provider key beside the side the pinned mapping assigns its stored
/// category.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SplitEntry {
    /// The message's provider key, unique within its account.
    pub key: String,
    /// The side the pinned mapping assigns the message's stored
    /// category word.
    pub side: SplitSide,
}

/// Failures of the document pulls: the honest no-bundle refusal, the
/// window read that cannot run, the service's own typed generation
/// failures, and the document store refusing a read or the put. Every
/// failure answers with no content: the caller never renders half a
/// document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentError {
    /// The app runs no AI bundle, so no generation can be spent.
    #[error("no ai bundle is running")]
    NoBundle,
    /// The engine's window read failed, so the collection refuses
    /// rather than digest a window it could not see.
    #[error("the generation window could not be read: {0}")]
    Window(String),
    /// The digest service failed; the message is its own.
    #[error("digest generation failed: {0}")]
    Digest(#[from] DigestError),
    /// The report service failed; the message is its own.
    #[error("report generation failed: {0}")]
    Report(#[from] ReportError),
    /// The document store refused the read or the write.
    #[error("the document store refused: {0}")]
    Store(String),
}

/// The wall clock in unix seconds, the shape the period bounds and the
/// store's stamps ride in; zero only if the clock reads before the
/// epoch, which the epoch itself then windows.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

/// The period a `days` window covers over `now_unix`, quantised to
/// whole UTC days: it opens at the midnight `days` minus one days
/// behind the "now" day's own midnight and closes at that day's last
/// second, whatever hour the turn ran at. The quantisation is what
/// makes the store's same-period replace the product's regenerate: a
/// second generation the same day lands on the period the first stored.
/// The days clamp applies here, so no caller can window past the pinned
/// bounds.
#[must_use]
pub fn window_bounds(days: u32, now_unix: i64) -> (i64, i64) {
    let days = i64::from(clamped_days(days));
    let today_start = now_unix - now_unix.rem_euclid(86_400);
    let start = today_start - (days - 1) * 86_400;
    (start, start + days * 86_400 - 1)
}

/// One civil UTC instant as the tuple the window filter compares on.
type Civil = (i32, u8, u8, u8, u8, u8);

/// The unix second as the civil tuple, `None` outside the range the
/// calendar holds.
fn civil_of(unix: i64) -> Option<Civil> {
    let instant = time::OffsetDateTime::from_unix_timestamp(unix).ok()?;
    Some((
        instant.year(),
        u8::from(instant.month()),
        instant.day(),
        instant.hour(),
        instant.minute(),
        instant.second(),
    ))
}

/// The row's date as the civil tuple, `None` when the row carries no
/// date at all: an undated row cannot be proven inside a period, so it
/// never rides the window.
fn civil_date(row: &MailListRow) -> Option<Civil> {
    let date = row.mail.date_utc?;
    Some((
        date.year(),
        date.month(),
        date.day(),
        date.hour(),
        date.minute(),
        date.second(),
    ))
}

impl<P: Provider> App<P> {
    /// Generates the account's digest over the last `days` days: the
    /// window's mail collected newest first with its stored
    /// categorisation, ONE summarising generation under a correlation
    /// minted for the pull, and the answer stored under the window's
    /// period, a same-period row replaced.
    ///
    /// # Errors
    /// [`DocumentError::NoBundle`] when the app runs no AI bundle;
    /// the collection, generation, and store failures as
    /// [`DocumentError`] names them, no content riding a failure.
    pub async fn generate_digest(
        &self,
        account: &AccountId,
        days: u32,
    ) -> Result<DigestView, DocumentError> {
        self.generate_document(account, days, AiDocumentKind::Digest, now_secs())
            .await
    }

    /// Generates the account's analytics report over the last `days`
    /// days: the same window the digest rides with the Sent mailbox's
    /// rows inside it counted in code, ONE structured generation under
    /// a correlation minted for the pull, and the assembled document
    /// stored under the window's period, a same-period row replaced.
    ///
    /// # Errors
    /// [`DocumentError::NoBundle`] when the app runs no AI bundle;
    /// the collection, generation, and store failures as
    /// [`DocumentError`] names them, no content riding a failure.
    pub async fn generate_report(
        &self,
        account: &AccountId,
        days: u32,
    ) -> Result<DigestView, DocumentError> {
        self.generate_document(account, days, AiDocumentKind::Report, now_secs())
            .await
    }

    /// The account's newest stored document of one kind, as the panel
    /// renders it; `None` when no generation has stored one. A read
    /// that fails answers empty with a warning rather than failing the
    /// pull.
    pub fn latest_document(
        &self,
        account: &AccountId,
        kind: AiDocumentKind,
    ) -> Option<DocumentView> {
        let pass = self.ai.as_ref()?;
        match pass.store.latest_document(account.as_str(), kind) {
            Ok(row) => row.map(|row| DocumentView {
                kind: row.kind,
                period_start: row.period_start,
                period_end: row.period_end,
                content: row.content,
                created_at: row.created_at,
            }),
            Err(err) => {
                log::warn!("ai docs: the latest document read failed: {err}");
                None
            }
        }
    }

    /// The account's split map over its stored categorisation: the
    /// window the message list itself loads (the snapshot's own read
    /// depth), each row's newest decided categorise word handed to the
    /// pure mapping, no model call at all. A window read that fails
    /// answers empty with a warning; no bundle, no split.
    pub async fn split_map(&self, account: &AccountId) -> Vec<SplitEntry> {
        let Some(pass) = self.ai.as_ref() else {
            return Vec::new();
        };
        let rows = match self
            .engine
            .mail_window(std::slice::from_ref(account), self.load_window())
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                log::warn!("ai docs: the split map's window read failed: {err}");
                return Vec::new();
            }
        };
        let stored = Self::stored_categories(&pass.store, account, &rows);
        derive_split(&stored)
            .into_iter()
            .map(|(key, side)| SplitEntry {
                key: key.as_str().to_owned(),
                side,
            })
            .collect()
    }

    /// One generation of `kind` over the account's `days` window as of
    /// `now_unix`: the shared heart of both public pulls, the "now" a
    /// parameter so the tests window the fixture day deterministically.
    /// Collects the window newest first, builds the stored
    /// categorisation, spends ONE call, and stores the answer under the
    /// period's key with the replace named.
    async fn generate_document(
        &self,
        account: &AccountId,
        days: u32,
        kind: AiDocumentKind,
        now_unix: i64,
    ) -> Result<DigestView, DocumentError> {
        let Some(pass) = self.ai.as_ref() else {
            return Err(DocumentError::NoBundle);
        };
        let (start, end) = window_bounds(days, now_unix);
        let sent = self.sent_mailbox(account).await;
        let rows = self
            .engine
            .mail_window(std::slice::from_ref(account), DOCUMENT_WINDOW_READ_LIMIT)
            .await
            .map_err(|err| DocumentError::Window(err.to_string()))?;
        let start_civil = civil_of(start);
        let end_civil = civil_of(end);
        let in_period = |row: &MailListRow| {
            match (civil_date(row), start_civil, end_civil) {
                (Some(date), Some(start_civil), Some(end_civil)) => {
                    start_civil <= date && date <= end_civil
                }
                // An undated row, or bounds the calendar cannot hold:
                // the row never rides a window it cannot be proven in.
                _ => false,
            }
        };

        let mut messages = Vec::new();
        let mut sent_count = 0u32;
        for row in &rows {
            if !in_period(row) {
                continue;
            }
            if let Some(sent) = sent.as_ref() {
                if row.mailboxes.contains(sent) {
                    sent_count += 1;
                }
            }
            let Some(body) = self.window_body(account, &row.mail.key).await else {
                log::warn!("ai docs: a window message's body fetch failed; it is skipped");
                continue;
            };
            messages.push(DocMessage {
                key: row.mail.key.clone(),
                from_name: row.mail.from_name.clone().unwrap_or_default(),
                from_address: row.mail.from_addr.clone().unwrap_or_default(),
                subject: row.mail.subject.clone().unwrap_or_default(),
                body,
                unread: row.mail.flags.is_unread(),
            });
        }
        let stored = Self::stored_categories(&pass.store, account, &rows);

        let correlation = CorrelationId::generate();
        let content = match kind {
            AiDocumentKind::Digest => {
                generate_digest(
                    account,
                    days,
                    &messages,
                    &stored,
                    &correlation,
                    pass.complete.as_ref(),
                )
                .await?
            }
            AiDocumentKind::Report => {
                generate_report(
                    account,
                    days,
                    &messages,
                    &stored,
                    sent_count,
                    &correlation,
                    pass.complete.as_ref(),
                )
                .await?
            }
        };

        let existed = match pass.store.documents_for(account.as_str()) {
            Ok(rows) => rows
                .iter()
                .any(|row| row.kind == kind && row.period_start == start && row.period_end == end),
            Err(err) => {
                log::warn!(
                    "ai docs: the stored-period read failed; the replace goes unnamed: {err}"
                );
                false
            }
        };
        pass.store
            .put_document(&NewAiDocument {
                account: account.as_str().to_owned(),
                kind,
                period_start: start,
                period_end: end,
                content: content.clone(),
                correlation,
            })
            .map_err(|err| DocumentError::Store(err.to_string()))?;
        Ok(DigestView {
            period_start: start,
            period_end: end,
            content,
            regenerated: existed,
        })
    }

    /// The account's Sent mailbox, when its tree names one: the role
    /// bearing folder whose rows the report's sent count walks. A read
    /// that fails answers `None` with a warning and the count rides the
    /// stats-safe zero, the choice the report's statistics own.
    async fn sent_mailbox(&self, account: &AccountId) -> Option<MailboxId> {
        let mailboxes = match self.engine.mailboxes(account).await {
            Ok(mailboxes) => mailboxes,
            Err(err) => {
                log::warn!("ai docs: the mailbox read failed; the sent count rides zero: {err}");
                return None;
            }
        };
        mailboxes
            .into_iter()
            .find(|mailbox| mailbox.role == Some(MailboxRole::Sent))
            .map(|mailbox| mailbox.id)
    }

    /// The stored categorisation over `rows`: each row's newest decided
    /// categorise word, keyed by the message's provider key. A message
    /// no pass judged, or whose only categorise row was a skip, holds no
    /// entry and lands in the uncategorised bucket; a read that fails
    /// logs and leaves the message uncategorised.
    fn stored_categories(
        store: &AiStore,
        account: &AccountId,
        rows: &[MailListRow],
    ) -> StoredCategories {
        let mut stored = StoredCategories::new();
        for row in rows {
            let results = match store
                .ai_results_for_message(account.as_str(), row.mail.key.as_str())
            {
                Ok(results) => results,
                Err(err) => {
                    log::warn!(
                        "ai docs: a window message's results read failed; it rides uncategorised: {err}"
                    );
                    continue;
                }
            };
            if let Some(word) = results
                .iter()
                .find(|result| result.stage == AiStage::Categorise.as_str())
                .and_then(|result| result.category.clone())
            {
                stored.insert(row.mail.key.clone(), word);
            }
        }
        stored
    }

    /// One message's plain body, resolved fresh by key through the
    /// reading path: the fetch the window messages ride, the extraction
    /// candidates' own read under its own name. `None` when the message
    /// no longer resolves, its account is gone, holds no provider, or
    /// the body cannot be fetched.
    async fn window_body(&self, account: &AccountId, key: &ProviderKey) -> Option<String> {
        let handle = self.account_handle(account).await?;
        let provider = handle.providers.first()?;
        let message = MessageRef {
            account: account.clone(),
            key: key.clone(),
        };
        let original = crate::reading::resolve(&self.engine, &message).await?;
        let body = self
            .engine
            .message_body(provider, account, &original)
            .await
            .ok()?;
        Some(body.plain().map(str::to_owned).unwrap_or_default())
    }
}

#[cfg(test)]
#[path = "drive_tests.rs"]
mod tests;
