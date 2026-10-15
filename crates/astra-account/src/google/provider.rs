// SPDX-License-Identifier: GPL-3.0-only
//! The refreshing Gmail provider: the engine's account-global
//! [`GmailProvider`] behind a token source that mints before every
//! call.
//!
//! The engine's provider takes a **static** bearer token, but Google
//! access tokens live about an hour, far shorter than an app session.
//! [`RefreshingGmailProvider`] mints a fresh access token from the
//! account's shared token source before each network call and delegates,
//! caching the built provider by the token it was built with so its HTTP
//! connection pool is reused and rebuilt only when the token refreshes,
//! never per request.
//!
//! Unlike the Graph wrapper, the Gmail one is **account-global**: Gmail's
//! history cursor is account-wide, so one provider covers every label
//! and message and there is no per-folder fan-out (the JMAP shape). The
//! wrapper also caps what it advertises: a capability it does not
//! forward is a capability the account does not have, however loudly the
//! delegate advertises it, so advertising and forwarding move together.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use engine_api::{
    AccountId, CalendarWrites, Capabilities, Draft, FailureClass, IdentityControls, MailEdit,
    MailEditReceipt, MailboxEdit, MailboxEditReceipt, MailboxWrites, Message, MessageReport,
    Provider, ReportControls, ReportEvidence, ReportVerdicts, SenderIdentity, SenderIdentityId,
    SubmissionReceipt, SyncScope, SyncWindow,
};
use engine_core::{mail::Mailbox, raw::RawMime, sync::SyncState, time::CalendarDate};
use engine_provider::{
    ConnectionInfo, EmailStream, HandOver, ProviderError, ProviderResult, ReportReceipt, ScopeSync,
};
use engine_tls::TlsClientConfig;
use futures::StreamExt;
use provider_google::{GmailProvider, GoogleClient};
use time::Date;

use crate::{AccountError, GraphTokenSource};

/// A [`Provider`] that refreshes its Google access token before every
/// network call and delegates to a freshly built [`GmailProvider`].
/// Account-global (no folder binding); one serves the whole account's
/// mail. Internal: the account layer hands callers `Box<dyn Provider>`
/// from the module's connect entry points.
pub(crate) struct RefreshingGmailProvider {
    /// The shared, self-refreshing token source (also shared with the
    /// account's calendar provider so one refresh serves both).
    tokens: Arc<GraphTokenSource>,
    /// The capabilities the wrapper reports: mail read/sync, mail
    /// writes, and submission, each of which it forwards to the delegate
    /// below. It advertises exactly what it forwards: a capability
    /// claimed but not forwarded falls through to the `Provider`
    /// trait's rejecting default.
    capabilities: Capabilities,
    /// The sync-depth cutoff, applied to the built delegate so the
    /// initial snapshot is windowed to recent mail (`None` syncs the
    /// whole account).
    since: Option<Date>,
    /// The account's shared TLS policy, cloned into each rebuilt Google
    /// client.
    tls: TlsClientConfig,
    /// The built delegate, cached by the access token it was built with,
    /// so its connection pool is reused across requests and rebuilt only
    /// when the token refreshes.
    cached: Mutex<Option<(String, Arc<GmailProvider>)>>,
}

impl core::fmt::Debug for RefreshingGmailProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The cached token is a live credential: the debug form claims
        // nothing but the wrapper's own name.
        f.debug_struct("RefreshingGmailProvider")
            .finish_non_exhaustive()
    }
}

impl RefreshingGmailProvider {
    /// Binds a refreshing Gmail provider (windowed to `since`) sharing
    /// `tokens` with the account's calendar provider.
    #[must_use]
    pub(crate) fn new(
        tokens: Arc<GraphTokenSource>,
        since: Option<Date>,
        tls: TlsClientConfig,
    ) -> Self {
        Self {
            tokens,
            capabilities: Capabilities::none()
                .with_mail()
                .with_mail_writes()
                // Forwarded by this wrapper's `MailboxWrites`.
                .with_mailbox_writes()
                // Forwarded by this wrapper's `Provider`, and without
                // phishing: Gmail's label set has no phishing member, so
                // asking for that verdict is a hard error rather than a
                // near-enough filing under spam.
                .with_mail_report(ReportControls {
                    verdicts: ReportVerdicts::without_phishing(),
                    evidence: ReportEvidence::Convention,
                })
                .with_submission()
                // This wrapper reports its own capabilities rather than
                // the delegate's, so a flag omitted here is a flag the
                // account does not have however loudly the adapter
                // underneath advertises it. `submit_email` forwards the
                // whole `Draft`, Gmail submits assembled RFC 5322 bytes,
                // and so the `method=` parameter an iMIP body part needs
                // (RFC 6047 section 2.4) survives.
                .with_scheduling_submission()
                // Forwarded below. Gmail's send-as settings are the
                // account holder's to change, so the name is `Writable`
                // rather than a directory's to hand down.
                .with_sender_identities(IdentityControls::Writable),
            since,
            tls,
            cached: Mutex::new(None),
        }
    }

    /// The delegate provider, minting from the token source first: the
    /// delegate is cached by the token it was built with and rebuilt only
    /// when a refresh produced a new token, so the underlying HTTP client
    /// (and its warm connection pool) is reused across requests.
    async fn delegate(&self) -> ProviderResult<Arc<GmailProvider>> {
        let token = self.tokens.access_token().await.map_err(|err| match err {
            AccountError::SigninRejected(detail) => ProviderError::authentication(detail),
            other => ProviderError::retryable(other.to_string()),
        })?;
        {
            let cache = self.cached.lock().expect("gmail delegate mutex poisoned");
            if let Some((cached_token, provider)) = cache.as_ref() {
                if *cached_token == token {
                    return Ok(Arc::clone(provider));
                }
            }
        }
        let client = GoogleClient::connect(token.clone(), &self.tls, &self.tokens.retry())
            .map_err(ProviderError::from)?;
        let mut gmail = GmailProvider::new(client);
        if let Some(date) = self.since.and_then(calendar_date) {
            gmail = gmail.with_since(date);
        }
        let provider = Arc::new(gmail);
        *self.cached.lock().expect("gmail delegate mutex poisoned") =
            Some((token, Arc::clone(&provider)));
        Ok(provider)
    }

    /// Drops the cached delegate so the next [`delegate`](Self::delegate)
    /// rebuilds a fresh client, and with it a fresh connection pool. Used
    /// after a retryable transport failure, whose keep-alive socket (one
    /// the OS killed while the machine slept, for example) is presumed
    /// dead; the token is unchanged, so this only rebuilds the client.
    fn invalidate_delegate(&self) {
        *self.cached.lock().expect("gmail delegate mutex poisoned") = None;
    }
}

/// Whether a failed Google call warrants dropping the cached delegate and
/// rebuilding it: a retryable **transport** error (a stale keep-alive
/// socket after sleep, a broken pipe) that a fresh client and connection
/// pool may clear. Distinct from a 429, which is waited out one layer
/// down on the same connection, and from an auth failure (never
/// retried). Tried at most once per call. For the reads that is trivially
/// safe (idempotent GETs), and for [`edit_mail`](Provider::edit_mail) it
/// is safe because a Gmail edit is a label **delta** to a fixed state
/// (`modify`/`trash`/`delete`), so re-applying it lands on the same
/// result.
///
/// [`submit_email`](Provider::submit_email) is the exception: it uses
/// this only to **drop** a dead delegate, never to re-issue, because a
/// send is not idempotent (a resent `messages.send` double-delivers) and
/// a retryable transport error cannot tell a lost-before-send from a
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

impl CalendarWrites for RefreshingGmailProvider {}

#[async_trait]
impl Provider for RefreshingGmailProvider {
    fn connection_info(&self) -> ConnectionInfo {
        // Capped capabilities, the delegate's transport facts.
        with_delegate_transport(
            ConnectionInfo::new(self.capabilities),
            self.cached
                .lock()
                .expect("gmail delegate mutex poisoned")
                .as_ref()
                .map(|(_, provider)| provider.connection_info()),
        )
    }

    fn mailbox_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::GmailLabelList {
            account: account.clone(),
        }
    }

    fn email_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::GmailMessages {
            account: account.clone(),
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

    /// Applies a [`MailEdit`] (mark-read/flag, move/archive, permanent
    /// delete) by delegating to the underlying [`GmailProvider`], with
    /// the same one-shot reconnect as the read calls. Gmail's edits are
    /// keyed by the message's immutable Gmail id, and this provider is
    /// account-global, so it can edit a message under any label.
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

    /// Submits a [`Draft`] via `messages.send` by delegating to the
    /// underlying [`GmailProvider`] (Gmail files the Sent copy itself),
    /// minting the access token first; but, **unlike** the reads and
    /// [`edit_mail`](Self::edit_mail), it **never re-issues the send on
    /// a transport failure.**
    ///
    /// A send is the one non-idempotent Gmail call: re-posting
    /// `messages.send` delivers the message twice. Whether a failed
    /// attempt may have reached Gmail is the outbox's to decide, from the
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

    /// Gmail reports a message by moving it under its `SPAM` label,
    /// which this forwards on the same reconnect loop
    /// [`edit_mail`](Self::edit_mail) uses: a report is idempotent, so a
    /// retry after a stale socket cannot double-apply.
    async fn set_sender_name(
        &self,
        account: &AccountId,
        identity: &SenderIdentityId,
        name: &str,
    ) -> ProviderResult<()> {
        let mut provider = self.delegate().await?;
        let mut reconnected = false;
        loop {
            match provider.set_sender_name(account, identity, name).await {
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
/// retries it anyway, after re-reading the label list.
#[async_trait]
impl MailboxWrites for RefreshingGmailProvider {
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
