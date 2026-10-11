// SPDX-License-Identifier: GPL-3.0-only
//! The AI documents store: the document kind, the row types, and the
//! put and read APIs the digest and report generation and the CLI ride
//! over [`AiStore`]. One row is one stored document: a digest or an
//! analytics report an account's period holds, the content the one
//! generation wrote, and the correlation id of the turn that wrote it.
//! The key is (account, kind, period_start, period_end): a
//! regeneration of the same period replaces the row, so an account
//! holds one document per kind per period while a changed window lands
//! as its own row. The put replaces rows with its own delete plus
//! insert inside one transaction, the text cache's convention, so a
//! repeated put cannot stack a second row and a concurrent put from
//! another process cannot interleave one.

use serde::{Deserialize, Serialize};

use crate::correlation::CorrelationId;
use crate::store::AiStore;
use crate::AiStoreError;

/// The kind of one stored AI document. The closed set is this table's
/// own contract (the digest and the analytics report, the A4 design's
/// first two decisions); the wire words are snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiDocumentKind {
    /// The on-demand digest of an account's recent mail.
    Digest,
    /// The analytics report over the recent window.
    Report,
}

impl AiDocumentKind {
    /// The snake_case wire word.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Digest => "digest",
            Self::Report => "report",
        }
    }

    /// The kind for a wire word, when the word is one of the closed set.
    pub fn from_wire(word: &str) -> Option<Self> {
        match word {
            "digest" => Some(Self::Digest),
            "report" => Some(Self::Report),
            _ => None,
        }
    }
}

impl std::fmt::Display for AiDocumentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One document to store: every column the caller owns; the store mints
/// the id and stamps `created_at` itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAiDocument {
    /// The account the document belongs to.
    pub account: String,
    /// The document's kind.
    pub kind: AiDocumentKind,
    /// The stored window's start (unix seconds).
    pub period_start: i64,
    /// The stored window's end (unix seconds).
    pub period_end: i64,
    /// The document's content, the one generation's output.
    pub content: String,
    /// The generation turn the document rode.
    pub correlation: CorrelationId,
}

/// One document as the store reads it back: every column of the
/// `ai_documents` table, `id` and `created_at` set by the store itself.
/// Serialisable so the CLI can carry rows across the process boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiDocumentRow {
    /// The row's identity, minted by the store.
    pub id: i64,
    /// The account the document belongs to.
    pub account: String,
    /// The document's kind.
    pub kind: AiDocumentKind,
    /// The stored window's start (unix seconds).
    pub period_start: i64,
    /// The stored window's end (unix seconds).
    pub period_end: i64,
    /// The document's content.
    pub content: String,
    /// The store's creation stamp (unix seconds), fresh on every
    /// replacement behind the key.
    pub created_at: i64,
    /// The generation turn the document rode.
    pub correlation: CorrelationId,
}

impl AiStore {
    /// Stores one document under its (account, kind, period) key,
    /// replacing any row the key already holds: the newest generation's
    /// content is the only one the key answers, and the replacement
    /// carries a fresh `created_at` and a fresh id (the store mints
    /// both itself, the ledger convention). The replace runs as a
    /// delete plus insert inside one transaction, so a concurrent put
    /// from another process cannot interleave one. Answers the row as
    /// it landed.
    ///
    /// # Errors
    /// Fails when the write cannot run.
    pub fn put_document(&self, document: &NewAiDocument) -> Result<AiDocumentRow, AiStoreError> {
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(AiStoreError::Sqlite)?;
        tx.execute(
            "DELETE FROM ai_documents
             WHERE account = ?1 AND kind = ?2 AND period_start = ?3 AND period_end = ?4",
            rusqlite::params![
                document.account,
                document.kind.as_str(),
                document.period_start,
                document.period_end
            ],
        )
        .map_err(AiStoreError::Sqlite)?;
        let row = tx
            .query_row(
                "INSERT INTO ai_documents (
                     account, kind, period_start, period_end, content,
                     created_at, correlation_id
                 ) VALUES (
                     ?1, ?2, ?3, ?4, ?5, strftime('%s','now'), ?6
                 )
                 RETURNING id, account, kind, period_start, period_end,
                           content, created_at, correlation_id",
                rusqlite::params![
                    document.account,
                    document.kind.as_str(),
                    document.period_start,
                    document.period_end,
                    document.content,
                    document.correlation.as_str(),
                ],
                map_document_row,
            )
            .map_err(AiStoreError::Sqlite)?;
        tx.commit().map_err(AiStoreError::Sqlite)?;
        Ok(row)
    }

    /// The account's newest document of one kind, when the account holds
    /// one: the read the reading surface's document panel and the CLI's
    /// latest verb ride. Recency is the creation stamp, then the id, so
    /// same-second puts still order deterministically. `None` names an
    /// account (or kind) with no document stored.
    ///
    /// # Errors
    /// Fails when the query cannot run.
    pub fn latest_document(
        &self,
        account: &str,
        kind: AiDocumentKind,
    ) -> Result<Option<AiDocumentRow>, AiStoreError> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id, account, kind, period_start, period_end,
                    content, created_at, correlation_id
             FROM ai_documents
             WHERE account = ?1 AND kind = ?2
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            rusqlite::params![account, kind.as_str()],
            map_document_row,
        )
        .map(Some)
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(AiStoreError::Sqlite(other)),
        })
    }

    /// Every document one account holds, newest first (creation stamp,
    /// then id, so same-second puts still order deterministically): the
    /// list the CLI's reading verb serves. An empty answer names an
    /// account no generation has touched.
    ///
    /// # Errors
    /// Fails when the query cannot run.
    pub fn documents_for(&self, account: &str) -> Result<Vec<AiDocumentRow>, AiStoreError> {
        let conn = self.lock();
        let mut statement = conn
            .prepare(
                "SELECT id, account, kind, period_start, period_end,
                        content, created_at, correlation_id
                 FROM ai_documents
                 WHERE account = ?1
                 ORDER BY created_at DESC, id DESC",
            )
            .map_err(AiStoreError::Sqlite)?;
        let rows = statement
            .query_map(rusqlite::params![account], map_document_row)
            .map_err(AiStoreError::Sqlite)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(AiStoreError::Sqlite)
    }
}

/// Maps one read row into [`AiDocumentRow`]. The kind word comes back
/// through the closed set; a word outside it names a corrupted row, so
/// the read fails rather than guessing.
fn map_document_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AiDocumentRow> {
    let kind_word: String = row.get(2)?;
    let kind = AiDocumentKind::from_wire(&kind_word).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(UnknownKindWord(kind_word)),
        )
    })?;
    Ok(AiDocumentRow {
        id: row.get(0)?,
        account: row.get(1)?,
        kind,
        period_start: row.get(3)?,
        period_end: row.get(4)?,
        content: row.get(5)?,
        created_at: row.get(6)?,
        correlation: CorrelationId::new(row.get(7)?),
    })
}

/// The error a kind word outside the closed set fails a read with.
#[derive(Debug)]
struct UnknownKindWord(String);

impl std::fmt::Display for UnknownKindWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown ai document kind word: {}", self.0)
    }
}

impl std::error::Error for UnknownKindWord {}
