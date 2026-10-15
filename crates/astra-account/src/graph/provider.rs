// SPDX-License-Identifier: GPL-3.0-only
//! The refreshing Graph mail provider: the engine's folder-bound
//! [`GraphProvider`] behind a token source that mints before every call.
//!
//! The engine's provider takes a **static** bearer token, but Graph
//! access tokens live about an hour, far shorter than an app session.
//! [`RefreshingGraphProvider`] mints a fresh access token from the
//! account's shared [`GraphTokenSource`] before each network call and
//! delegates, caching the built provider by the token it was built with
//! so its HTTP connection pool is reused and rebuilt only when the token
//! refreshes, never per request.
//!
//! The wrapper also caps what it advertises: a capability it does not
//! forward is a capability the account does not have, however loudly the
//! delegate advertises it, so advertising and forwarding move together.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use engine_api::{
    AccountId, CalendarWrites, Capabilities, Draft, FailureClass, IdentityControls, MailEdit,
    MailEditReceipt, MailboxEdit, MailboxEditReceipt, MailboxId, MailboxWrites, Message,
    MessageReport, Provider, ReportControls, ReportEvidence, ReportVerdicts, SenderIdentity,
    SubmissionReceipt, SyncScope, SyncWindow,
};
use engine_core::{mail::Mailbox, raw::RawMime, sync::SyncState, time::CalendarDate};
use engine_provider::{
    ConnectionInfo, EmailStream, HandOver, ProviderError, ProviderResult, ReportReceipt, ScopeSync,
};
use engine_tls::TlsClientConfig;
use futures::StreamExt;
use provider_graph::{GraphClient, GraphProvider, MailboxPrincipal};
use time::Date;

use super::GraphTokenSource;
use crate::AccountError;

/// A [`Provider`] bound to one Graph mail folder that refreshes its access
/// token before every network call and delegates to a freshly built
/// [`GraphProvider`]. Internal; the account layer hands callers
/// `Box<dyn Provider>` from the dial, never the concrete type.
pub(crate) struct RefreshingGraphProvider {
    folder: MailboxId,
    tokens: Arc<GraphTokenSource>,
    capabilities: Capabilities,
    /// The sync-depth cutoff, applied to the built delegate so the initial
    /// snapshot is windowed to recent mail (`None` syncs the whole
    /// folder).
    since: Option<Date>,
    /// The account's shared TLS policy, cloned into each rebuilt Graph
    /// client.
    tls: TlsClientConfig,
    /// The built delegate, cached by the access token it was built with,
    /// so its connection pool is reused across requests and rebuilt only
    /// when the token refreshes.
    cached: Mutex<Option<(String, Arc<GraphProvider>)>>,
}

impl core::fmt::Debug for RefreshingGraphProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The cached token is a live credential: the debug form names the
        // bound folder and nothing else.
        f.debug_struct("RefreshingGraphProvider")
            .field("folder", &self.folder)
            .finish_non_exhaustive()
    }
}

impl RefreshingGraphProvider {
    /// Binds a refreshing provider to `folder` (windowed to `since`),
    /// sharing `tokens` with the account's other folders.
    #[must_use]
    pub(crate) fn new(
        folder: MailboxId,
        tokens: Arc<GraphTokenSource>,
        since: Option<Date>,
        tls: TlsClientConfig,
    ) -> Self {
        Self {
            folder,
            tokens,
            capabilities: Capabilities::none()
                .with_mail()
                .with_mail_writes()
                // Forwarded by this wrapper's `MailboxWrites`.
                .with_mailbox_writes()
                // Forwarded by this wrapper's `Provider`. A flag omitted
                // here is a flag the account does not have however loudly
                // the delegate advertises it, so advertising and
                // forwarding move together.
                .with_mail_report(ReportControls {
                    verdicts: ReportVerdicts::all(),
                    evidence: ReportEvidence::Acknowledged,
                })
                .with_submission()
                // The whole `Draft` is forwarded to the delegate
                // untouched, so whatever it can send, this wrapper can.
                // Graph submits assembled RFC 5322 bytes and therefore
                // owns the `method=` parameter that makes an iTIP object
                // a scheduling message rather than a calendar file
                // (RFC 6047 §2.4).
                .with_scheduling_submission()
                // Forwarded below, and **read-only**: a Graph mailbox's
                // display name is a directory attribute a tenant
                // administrator owns, so claiming `Writable` here would
                // put an editor in front of an edit that cannot land.
                .with_sender_identities(IdentityControls::ReadOnly),
            // Server-side drafts are deliberately not advertised although
            // the engine's GraphProvider implements them: without the flag
            // the composer keeps no draft on the server for a Microsoft
            // account (the IMAP dial does advertise it). The recorded
            // gap, named here rather than left silent.
            since,
            tls,
            cached: Mutex::new(None),
        }
    }

    /// The delegate provider, minting from the token source first: the
    /// delegate is cached by the token it was built with and rebuilt only
    /// when a refresh produced a new token, so the underlying HTTP client
    /// (and its warm connection pool) is reused across requests.
    async fn delegate(&self) -> ProviderResult<Arc<GraphProvider>> {
        let token = self.tokens.access_token().await.map_err(|err| match err {
            AccountError::SigninRejected(detail) => ProviderError::authentication(detail),
            other => ProviderError::retryable(other.to_string()),
        })?;
        {
            let cache = self.cached.lock().expect("graph delegate mutex poisoned");
            if let Some((cached_token, provider)) = cache.as_ref() {
                if *cached_token == token {
                    return Ok(Arc::clone(provider));
                }
            }
        }
        let client = GraphClient::for_mailbox(
            token.clone(),
            MailboxPrincipal::Me,
            &self.tls,
            &self.tokens.retry(),
        )
        .map_err(ProviderError::from)?;
        let mut graph = GraphProvider::new(client, self.folder.clone());
        if let Some(date) = self.since.and_then(calendar_date) {
            graph = graph.with_since(date);
        }
        let provider = Arc::new(graph);
        *self.cached.lock().expect("graph delegate mutex poisoned") =
            Some((token, Arc::clone(&provider)));
        Ok(provider)
    }

    /// Drops the cached delegate so the next [`delegate`](Self::delegate)
    /// rebuilds a fresh client, and with it a fresh connection pool. Used
    /// after a retryable transport failure, whose keep-alive socket (one
    /// the OS killed while the machine slept, for example) is presumed
    /// dead; the token is unchanged, so this only rebuilds the client.
    fn invalidate_delegate(&self) {
        *self.cached.lock().expect("graph delegate mutex poisoned") = None;
    }
}

/// Whether a failed Graph call warrants dropping the cached delegate and
/// rebuilding it: a retryable **transport** error (a stale keep-alive
/// socket after sleep, a broken pipe) that a fresh client and connection
/// pool may clear. Distinct from a 429, which is waited out one layer
/// down on the same connection, and from an auth failure (never
/// retried). Tried at most once per call. For the reads that is trivially
/// safe (idempotent GETs), and for [`edit_mail`](Provider::edit_mail) it
/// is safe because a transport error on Graph's stateless HTTP means the
/// request died at the socket before the server applied it, so
/// re-dialing and re-issuing once cannot double-apply, and the edit is
/// outbox-mediated regardless.
///
/// [`submit_email`](Provider::submit_email) is the exception: it uses
/// this only to **drop** a dead delegate, never to re-issue, because a
/// send is not idempotent (a resent `sendMail` double-delivers) and a
/// retryable transport error cannot tell a lost-before-send from a
/// lost-after-send. The outbox, not this wrapper, owns the retry.
pub(super) fn should_reconnect(err: &ProviderError) -> bool {
    err.class() == FailureClass::Retryable
}

/// Converts a `time` date into the engine's calendar date, shared with
/// the calendar provider's window arithmetic.
pub(super) fn calendar_date(date: Date) -> Option<CalendarDate> {
    CalendarDate::new(date.year(), u8::from(date.month()), date.day()).ok()
}

/// Combines a wrapper's capped `capabilities` with the live `delegate`'s
/// transport facts.
///
/// A wrapper reports its **own** capabilities on purpose: a flag omitted
/// there is a flag the account does not have, however loudly the adapter
/// underneath advertises it, and that is what stops this crate promising
/// a write it never forwards. The other fields are not promises at all.
/// They are observations about the delegate's live connection, and a
/// wrapper that reconstructs them from [`ConnectionInfo::new`] does not
/// report "unknown"; it reports the *default*, which is a confident
/// wrong answer. Before a delegate exists (`None`) the capped value's
/// own conservative defaults stand: no TLS or HTTP version observed, and
/// a fetch width of one.
fn with_delegate_transport(
    capped: ConnectionInfo,
    delegate: Option<ConnectionInfo>,
) -> ConnectionInfo {
    let Some(delegate) = delegate else {
        return capped;
    };
    ConnectionInfo {
        tls_version: delegate.tls_version,
        http_version: delegate.http_version,
        concurrent_fetches: delegate.concurrent_fetches,
        sources_per_request: delegate.sources_per_request,
        ..capped
    }
}

impl CalendarWrites for RefreshingGraphProvider {}

#[async_trait]
impl Provider for RefreshingGraphProvider {
    fn connection_info(&self) -> ConnectionInfo {
        // Capped capabilities, the delegate's transport facts.
        with_delegate_transport(
            ConnectionInfo::new(self.capabilities),
            self.cached
                .lock()
                .expect("graph delegate mutex poisoned")
                .as_ref()
                .map(|(_, provider)| provider.connection_info()),
        )
    }

    fn mailbox_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::GraphFolderList {
            account: account.clone(),
        }
    }

    fn email_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::GraphFolder {
            account: account.clone(),
            folder: self.folder.clone(),
        }
    }

    async fn sync_mailboxes(
        &self,
        account: &AccountId,
        cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Mailbox>> {
        let mut provider = self.delegate().await?;
        let mut reconnected = false;
        loop {
            match provider.sync_mailboxes(account, cursor).await {
                Ok(value) => return Ok(value),
                Err(err) if !reconnected && should_reconnect(&err) => {
                    self.invalidate_delegate();
                    provider = self.delegate().await?;
                    reconnected = true;
                }
                Err(err) => return Err(err),
            }
        }
    }

    fn stream_email<'a>(
        &'a self,
        account: &'a AccountId,
        cursor: Option<&'a SyncState>,
        window: SyncWindow,
        fetch_batch: usize,
        chunk_size: usize,
    ) -> EmailStream<'a> {
        let account = account.clone();
        let cursor = cursor.cloned();
        Box::pin(async_stream::try_stream! {
            let mut reconnected = false;
            loop {
                let provider = self.delegate().await?;
                let mut chunks = Vec::new();
                let result = {
                    let mut stream = provider.stream_email(
                        &account,
                        cursor.as_ref(),
                        window,
                        fetch_batch,
                        chunk_size,
                    );
                    let mut result = Ok(());
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(chunk) => chunks.push(chunk),
                            Err(err) => {
                                result = Err(err);
                                break;
                            }
                        }
                    }
                    result
                };
                match result {
                    Ok(()) => {
                        for chunk in chunks {
                            yield chunk;
                        }
                        break;
                    }
                    Err(err) if !reconnected && should_reconnect(&err) => {
                        self.invalidate_delegate();
                        reconnected = true;
                    }
                    Err(err) => Err(err)?,
                }
            }
        })
    }

    async fn fetch_message_source(
        &self,
        account: &AccountId,
        message: &Message,
    ) -> ProviderResult<RawMime> {
        let mut provider = self.delegate().await?;
        let mut reconnected = false;
        loop {
            match provider.fetch_message_source(account, message).await {
                Ok(value) => return Ok(value),
                Err(err) if !reconnected && should_reconnect(&err) => {
                    self.invalidate_delegate();
                    provider = self.delegate().await?;
                    reconnected = true;
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// Applies a [`MailEdit`] (mark-read/flag, move, permanent delete) by
    /// delegating to the underlying [`GraphProvider`], with the same
    /// one-shot reconnect as the read calls. The `edit` is keyed by the
    /// message's immutable Graph id, so this folder-bound provider can
    /// edit a message in any folder.
    async fn edit_mail(
        &self,
        account: &AccountId,
        edit: &MailEdit,
    ) -> ProviderResult<MailEditReceipt> {
        let mut provider = self.delegate().await?;
        let mut reconnected = false;
        loop {
            match provider.edit_mail(account, edit).await {
                Ok(value) => return Ok(value),
                Err(err) if !reconnected && should_reconnect(&err) => {
                    self.invalidate_delegate();
                    provider = self.delegate().await?;
                    reconnected = true;
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// Submits a [`Draft`] via `POST /me/sendMail` by delegating to the
    /// underlying [`GraphProvider`] (Graph files the Sent copy itself),
    /// minting the access token first; but, **unlike** the reads and
    /// [`edit_mail`](Self::edit_mail), it **never re-issues the send on a
    /// transport failure.**
    ///
    /// A send is the one non-idempotent Graph call: re-posting
    /// `sendMail` delivers the message twice. Whether a failed attempt
    /// may have reached Graph is the outbox's to decide, from the
    /// hand-over record the adapter writes before the end of the request
    /// goes out (`hand_over`); it retries one that cannot have left and
    /// asks about one that may have. So on a transport error this only
    /// **drops** the (possibly dead-socketed) cached delegate, so the
    /// next attempt dials a fresh connection, then propagates the error
    /// instead of resending itself.
    async fn submit_email(
        &self,
        account: &AccountId,
        draft: &Draft,
        hand_over: &HandOver<'_>,
    ) -> ProviderResult<SubmissionReceipt> {
        let provider = self.delegate().await?;
        provider
            .submit_email(account, draft, hand_over)
            .await
            .inspect_err(|err| {
                if should_reconnect(err) {
                    self.invalidate_delegate();
                }
            })
    }

    async fn sender_identities(&self, account: &AccountId) -> ProviderResult<Vec<SenderIdentity>> {
        let mut provider = self.delegate().await?;
        let mut reconnected = false;
        loop {
            match provider.sender_identities(account).await {
                Ok(value) => return Ok(value),
                Err(err) if !reconnected && should_reconnect(&err) => {
                    self.invalidate_delegate();
                    provider = self.delegate().await?;
                    reconnected = true;
                }
                Err(err) => return Err(err),
            }
        }
    }

    async fn report_message(
        &self,
        account: &AccountId,
        report: &MessageReport,
    ) -> ProviderResult<ReportReceipt> {
        let mut provider = self.delegate().await?;
        let mut reconnected = false;
        loop {
            match provider.report_message(account, report).await {
                Ok(value) => return Ok(value),
                Err(err) if !reconnected && should_reconnect(&err) => {
                    self.invalidate_delegate();
                    provider = self.delegate().await?;
                    reconnected = true;
                }
                Err(err) => return Err(err),
            }
        }
    }
}

/// A folder change is made **once**: the token is minted first, and a
/// dropped connection only drops the cached delegate, as for a send.
/// Re-issuing it is not safe on every transport (a rename replayed
/// against the path it already left meets nothing), and the outbox
/// retries it anyway, after re-reading the folder list.
#[async_trait]
impl MailboxWrites for RefreshingGraphProvider {
    async fn edit_mailbox(
        &self,
        account: &AccountId,
        edit: &MailboxEdit,
    ) -> ProviderResult<MailboxEditReceipt> {
        let provider = self.delegate().await?;
        let result = provider.edit_mailbox(account, edit).await;
        if let Err(err) = &result {
            if should_reconnect(err) {
                self.invalidate_delegate();
            }
        }
        result
    }
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
