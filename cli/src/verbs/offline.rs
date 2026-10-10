// SPDX-License-Identifier: GPL-3.0-only
//! The provider every CLI write and body fetch goes through: it dials
//! nothing. Draft writes park in the engine's outbox for the app's drain,
//! and an uncached body is an error rather than a network fetch.

use async_trait::async_trait;
use engine_api::{
    AccountId, CalendarWrites, Capabilities, Draft, MailboxWrites, Message, Provider, ProviderKey,
};
use engine_core::raw::RawMime;
use engine_core::sync::{SyncState, SyncWindow};
use engine_provider::{ConnectionInfo, EmailStream, ProviderError};

/// The words every offline provider failure carries. Classified retryable
/// so the outbox parks the write for the app's drain rather than settling
/// it as a permanent failure.
const OFFLINE: &str = "the astra-cli dials no server; the app's outbox executes this write";

/// The offline provider: no connection, no fetch, no dial.
pub(crate) struct Offline;

#[async_trait]
impl Provider for Offline {
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::new(Capabilities::none())
    }

    fn stream_email<'a>(
        &'a self,
        account: &'a AccountId,
        cursor: Option<&'a SyncState>,
        _window: SyncWindow,
        _fetch_batch: usize,
        _chunk_size: usize,
    ) -> EmailStream<'a> {
        let _ = (account, cursor);
        Box::pin(futures::stream::once(async {
            Err(ProviderError::retryable(OFFLINE))
        }))
    }

    async fn fetch_message_source(
        &self,
        _account: &AccountId,
        _message: &Message,
    ) -> Result<RawMime, ProviderError> {
        Err(ProviderError::retryable(OFFLINE))
    }

    async fn put_draft(
        &self,
        _account: &AccountId,
        _draft: &Draft,
        _replacing: Option<&ProviderKey>,
    ) -> Result<ProviderKey, ProviderError> {
        Err(ProviderError::retryable(OFFLINE))
    }

    async fn delete_draft(
        &self,
        _account: &AccountId,
        _draft: &ProviderKey,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::retryable(OFFLINE))
    }
}

impl MailboxWrites for Offline {}
impl CalendarWrites for Offline {}
