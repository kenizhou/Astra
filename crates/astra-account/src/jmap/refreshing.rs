// SPDX-License-Identifier: GPL-3.0-only
//! [`RefreshingJmapProvider`]: a JMAP provider that keeps itself on a live
//! access token.
//!
//! # Why a wrapper is needed at all
//!
//! An OAuth access token lives about an hour; an app session lives much
//! longer. The engine's [`JmapProvider`] is built once with a fixed
//! credential and has no way to learn a new one, so a provider connected at
//! launch would start `401`ing after an hour and every later sync would
//! fail. Minting a token at connect time is not enough; connect happens
//! once.
//!
//! So this wraps the delegate and mints the token **before each call**,
//! rebuilding the delegate only when the token actually changed (roughly
//! hourly). The rebuild is not free for JMAP the way it is for Graph
//! (`JmapProvider::connect` re-runs session discovery) but at once an hour
//! that is immaterial, and caching by token keeps the reqwest connection
//! pool warm in between. This is the same shape as
//! `RefreshingGmailProvider`, and it is why the engine needs no OAuth code
//! of its own.
//!
//! The token source is the shared
//! [`GraphTokenSource`](crate::GraphTokenSource) in its JMAP costume: the
//! type is provider-neutral, so the single-flight refresh, the replay
//! cool-downs, and the rotation write through the [`TokenSink`] are
//! inherited rather than reimplemented. The rotation write is the one that
//! matters here: a JMAP server that ratchets its refresh tokens revokes
//! the whole grant over a replayed one, so a rotation nobody persisted
//! kills the account at the next launch. That discipline lives in the
//! source; this wrapper only guarantees every call spends a token the
//! source minted.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use engine_api::{
    AccountId, CalendarWrites, Capabilities, Draft, MailEdit, MailEditReceipt, MailboxEdit,
    MailboxEditReceipt, MailboxWrites, Message, MessageReport, Provider, SenderIdentity,
    SenderIdentityId, SubmissionReceipt, SyncScope, SyncWindow,
};
use engine_core::{
    calendar::{Calendar, Event},
    mail::Mailbox,
    raw::RawMime,
    sync::{JmapDataType, SyncState},
};
use engine_provider::{
    ConnectionInfo, EmailStream, EventDeletion, EventDraft, EventEdit, EventWrite,
    EventWriteReceipt, HandOver, ProviderError, ProviderResult, ReportReceipt, ScopeSync,
    SourceStream,
};
use engine_tls::TlsClientConfig;
use futures::StreamExt;
use provider_jmap::{Credentials, JmapConfig, JmapProvider};

use crate::GraphTokenSource;

/// A [`Provider`] for an OAuth JMAP account: refreshes the access token as
/// needed and delegates to a [`JmapProvider`] built with it.
/// Account-global; one serves the whole account's mail and calendar,
/// exactly like the non-OAuth JMAP path.
pub struct RefreshingJmapProvider {
    /// The JMAP server base URL every rebuilt delegate re-discovers its
    /// session from.
    base_url: String,
    /// The shared, self-refreshing token source. Shared with every other
    /// provider the account dials, so one refresh serves them all.
    tokens: Arc<GraphTokenSource>,
    /// The capabilities learned from the **initial** connect's session
    /// resource, so `connection_info` can answer synchronously (and before
    /// any delegate is rebuilt) without claiming a capability this server
    /// does not have.
    capabilities: Capabilities,
    /// The account's shared TLS policy, cloned into each rebuilt client.
    tls: TlsClientConfig,
    /// The built delegate, cached by the access token it was built with.
    /// Rebuilt only when a refresh produced a new token; never per request,
    /// which would re-run session discovery and open a fresh TLS connection
    /// every single call.
    cached: Mutex<Option<(String, Arc<JmapProvider>)>>,
}

impl core::fmt::Debug for RefreshingJmapProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never surface the token the cache is keyed on.
        f.debug_struct("RefreshingJmapProvider")
            .field("base_url", &self.base_url)
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

impl RefreshingJmapProvider {
    /// Wraps an already-connected `initial` delegate, which was built with
    /// `access_token` and whose session answered `capabilities`.
    ///
    /// Taking the first delegate rather than connecting lazily means the
    /// caller has already proved the grant works and read the server's real
    /// capabilities: so this never advertises mail or calendar support the
    /// session did not report, and never pays a second connect straight
    /// after the first.
    #[must_use]
    pub fn new(
        base_url: String,
        tokens: Arc<GraphTokenSource>,
        tls: TlsClientConfig,
        access_token: String,
        initial: JmapProvider,
        capabilities: Capabilities,
    ) -> Self {
        Self {
            base_url,
            tokens,
            capabilities,
            tls,
            cached: Mutex::new(Some((access_token, Arc::new(initial)))),
        }
    }

    /// The capabilities the initial session reported.
    #[must_use]
    pub(super) fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// Returns the delegate, reusing the cached one while the access token
    /// is unchanged and rebuilding it (a fresh session discovery) only when
    /// a refresh produced a new token.
    ///
    /// # Errors
    ///
    /// Returns an authentication error when the refresh token is dead (the
    /// host must re-auth), or a retryable error for a transient
    /// refresh/connect failure.
    pub(super) async fn delegate(&self) -> ProviderResult<Arc<JmapProvider>> {
        let token = self.tokens.access_token().await.map_err(|err| match err {
            crate::AccountError::SigninRejected(detail) => ProviderError::authentication(detail),
            other => ProviderError::retryable(other.to_string()),
        })?;
        {
            let cache = self.cached.lock().expect("jmap delegate mutex poisoned");
            if let Some((cached_token, provider)) = cache.as_ref() {
                if *cached_token == token {
                    return Ok(Arc::clone(provider));
                }
            }
        }
        log::debug!("jmap: access token changed; rebuilding the provider session");
        let config = JmapConfig::new(self.base_url.clone(), Credentials::bearer(token.clone()))
            .with_tls(self.tls.clone())
            .with_retry(self.tokens.retry());
        let rebuilt = JmapProvider::connect(config)
            .await
            .map_err(|err| ProviderError::retryable(err.to_string()))?;
        let provider = Arc::new(rebuilt);
        *self.cached.lock().expect("jmap delegate mutex poisoned") =
            Some((token, Arc::clone(&provider)));
        Ok(provider)
    }
}

/// Combines the wrapper's session-derived `capabilities` with the live
/// delegate's transport facts.
///
/// The capabilities are the initial session's own report, never the
/// wrapper's guess. The other fields are not promises at all: they are
/// observations about the delegate's live connection, and a wrapper that
/// reconstructs them from [`ConnectionInfo::new`] does not report
/// "unknown"; it reports the *default*, which is a confident wrong answer.
/// Before a delegate exists (`None`) those conservative defaults stand: no
/// TLS or HTTP version observed, and a fetch width of one.
fn with_delegate_transport(
    capabilities: Capabilities,
    delegate: Option<ConnectionInfo>,
) -> ConnectionInfo {
    let capped = ConnectionInfo::new(capabilities);
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

#[async_trait]
impl Provider for RefreshingJmapProvider {
    fn connection_info(&self) -> ConnectionInfo {
        // The capabilities the initial session actually reported, carrying
        // the live delegate's transport facts.
        with_delegate_transport(
            self.capabilities(),
            self.cached
                .lock()
                .expect("jmap delegate mutex poisoned")
                .as_ref()
                .map(|(_, provider)| provider.connection_info()),
        )
    }

    fn mailbox_scope(&self, account: &AccountId) -> SyncScope {
        // The engine's default JMAP scopes are already the account-wide
        // ones this provider serves; forwarding explicitly keeps the
        // wrapper honest if the delegate ever narrows them.
        SyncScope::JmapType {
            account: account.clone(),
            data_type: JmapDataType::Mailbox,
        }
    }

    fn email_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::JmapType {
            account: account.clone(),
            data_type: JmapDataType::Email,
        }
    }

    fn default_sync_window(&self) -> SyncWindow {
        SyncWindow::full()
    }

    async fn sync_mailboxes(
        &self,
        account: &AccountId,
        cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Mailbox>> {
        self.delegate().await?.sync_mailboxes(account, cursor).await
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
            // The delegate is resolved inside the stream (the token may
            // have refreshed since the stream was constructed), then its
            // chunks are forwarded one for one.
            let provider = self.delegate().await?;
            let mut stream = provider.stream_email(
                &account,
                cursor.as_ref(),
                window,
                fetch_batch,
                chunk_size,
            );
            while let Some(chunk) = stream.next().await {
                yield chunk?;
            }
        })
    }

    async fn sync_email(
        &self,
        account: &AccountId,
        cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Message>> {
        self.delegate().await?.sync_email(account, cursor).await
    }

    async fn submit_email(
        &self,
        account: &AccountId,
        draft: &Draft,
        hand_over: &HandOver<'_>,
    ) -> ProviderResult<SubmissionReceipt> {
        self.delegate()
            .await?
            .submit_email(account, draft, hand_over)
            .await
    }

    async fn edit_mail(
        &self,
        account: &AccountId,
        edit: &MailEdit,
    ) -> ProviderResult<MailEditReceipt> {
        self.delegate().await?.edit_mail(account, edit).await
    }

    async fn fetch_message_source(
        &self,
        account: &AccountId,
        message: &Message,
    ) -> ProviderResult<RawMime> {
        self.delegate()
            .await?
            .fetch_message_source(account, message)
            .await
    }

    /// Forwarded, because the delegate's batch is one `Blob/get` for the
    /// lot where the server offers it, and the trait's default would fetch
    /// the batch one message at a time through
    /// [`fetch_message_source`](Self::fetch_message_source) instead.
    fn fetch_message_sources<'a>(
        &'a self,
        account: &'a AccountId,
        messages: &'a [Message],
    ) -> SourceStream<'a> {
        Box::pin(async_stream::stream! {
            let provider = match self.delegate().await {
                Ok(provider) => provider,
                Err(err) => {
                    for index in 0..messages.len() {
                        yield (index, Err(ProviderError::new(err.class(), err.to_string())));
                    }
                    return;
                }
            };
            let mut sources = provider.fetch_message_sources(account, messages);
            while let Some(item) = sources.next().await {
                yield item;
            }
        })
    }

    fn calendar_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::JmapType {
            account: account.clone(),
            data_type: JmapDataType::Calendar,
        }
    }

    fn event_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::JmapType {
            account: account.clone(),
            data_type: JmapDataType::CalendarEvent,
        }
    }

    async fn sync_calendars(
        &self,
        account: &AccountId,
        cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Calendar>> {
        self.delegate().await?.sync_calendars(account, cursor).await
    }

    async fn sync_events(
        &self,
        account: &AccountId,
        cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Event>> {
        self.delegate().await?.sync_events(account, cursor).await
    }

    /// The identities the session's submission account can send as,
    /// forwarded to the session-backed delegate.
    ///
    /// This wrapper reports the **session's own** capabilities rather than
    /// a capped set, so `sender_identities` is advertised whenever the
    /// server advertised submission. Advertising and forwarding move
    /// together: a verb left unforwarded here answers "unsupported" while
    /// the capability says otherwise, and a host reading the capability
    /// would draw an editor over it.
    async fn sender_identities(&self, account: &AccountId) -> ProviderResult<Vec<SenderIdentity>> {
        self.delegate().await?.sender_identities(account).await
    }

    async fn set_sender_name(
        &self,
        account: &AccountId,
        identity: &SenderIdentityId,
        name: &str,
    ) -> ProviderResult<()> {
        self.delegate()
            .await?
            .set_sender_name(account, identity, name)
            .await
    }

    /// JMAP reports a message by setting the RFC 8621 `$junk`/`$notjunk`
    /// keyword, forwarded to the session-backed delegate exactly as
    /// [`edit_mail`](Provider::edit_mail) is.
    async fn report_message(
        &self,
        account: &AccountId,
        report: &MessageReport,
    ) -> ProviderResult<ReportReceipt> {
        self.delegate().await?.report_message(account, report).await
    }
}

#[async_trait]
impl MailboxWrites for RefreshingJmapProvider {
    async fn edit_mailbox(
        &self,
        account: &AccountId,
        edit: &MailboxEdit,
    ) -> ProviderResult<MailboxEditReceipt> {
        self.delegate().await?.edit_mailbox(account, edit).await
    }
}

#[async_trait]
impl CalendarWrites for RefreshingJmapProvider {
    async fn create_event(
        &self,
        account: &AccountId,
        draft: &EventDraft,
    ) -> ProviderResult<EventWriteReceipt> {
        self.delegate().await?.create_event(account, draft).await
    }

    async fn patch_event(
        &self,
        account: &AccountId,
        base: &Event,
        edit: &EventEdit,
    ) -> ProviderResult<EventWriteReceipt> {
        self.delegate()
            .await?
            .patch_event(account, base, edit)
            .await
    }

    async fn put_event(
        &self,
        account: &AccountId,
        write: &EventWrite,
    ) -> ProviderResult<EventWriteReceipt> {
        self.delegate().await?.put_event(account, write).await
    }

    async fn delete_event(
        &self,
        account: &AccountId,
        base: Option<&Event>,
        deletion: &EventDeletion,
    ) -> ProviderResult<()> {
        self.delegate()
            .await?
            .delete_event(account, base, deletion)
            .await
    }
}

#[cfg(test)]
#[path = "../jmap_refreshing_tests.rs"]
mod tests;
