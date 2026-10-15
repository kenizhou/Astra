// SPDX-License-Identifier: GPL-3.0-only
//! The refreshing Graph calendar provider: the engine's calendar-bound
//! [`GraphCalendarProvider`] behind the same shared token source as the
//! account's mail providers.
//!
//! Like the mail side, the engine's provider takes a **static** bearer
//! token, so [`RefreshingGraphCalendarProvider`] mints a fresh access
//! token before each network call and delegates to a freshly built
//! provider (reusing its warm connection pool while the token is
//! unchanged, rebuilt only when the token refreshes). The provider is
//! bound to **one** calendar (the account's default) and carries a
//! baked-in fetch window plus display zone: Graph's
//! `calendarView/delta` needs an explicit date range, and the
//! `Prefer: outlook.timezone` read needs the host's display zone. The
//! window is centred on today at connect time, so a reconnect re-centres
//! it and it never drifts far from today.
//!
//! **Reads** (`sync_calendars` / `sync_events`) are idempotent
//! GET/delta calls, so they get the same one-shot reconnect loop the
//! mail providers use. **Writes** (`create` / `patch` / `delete`) are
//! single-attempt: a blind retry of a non-idempotent `POST` could
//! double-create, and the app surfaces a failed write for the user to
//! retry.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use engine_api::{
    AccountId, Calendar, CalendarId, CalendarWrites, Capabilities, Event, EventDeletion,
    EventDraft, EventEdit, EventWriteReceipt, MailboxWrites, OverrideSurvival, Provider, SyncScope,
    TimeZoneId, WriteGuard,
};
use engine_core::sync::{SyncState, SyncUpdate};
use engine_provider::{ConnectionInfo, ProviderError, ProviderResult, ScopeSync};
use engine_tls::TlsClientConfig;
use provider_graph::{CalendarWindow, GraphCalendarProvider, GraphClient, MailboxPrincipal};
use time::{Duration, OffsetDateTime};

use super::provider::{calendar_date, should_reconnect};
use super::GraphTokenSource;
use crate::{account_log_handle, AccountError};

/// What a Graph series edit costs the occurrences the user changed by
/// hand: moving the series' time **or** changing its rule destroys every
/// one of them.
///
/// The **fallback** copy of the engine adapter's own constant, for the
/// window before a delegate exists; `connection_info` reports the
/// delegate's once one is built, and that is the value the engine's live
/// suite measures against the real server. A host reading capabilities
/// before the first token fetch must not be told a Graph series edit is
/// free.
const GRAPH_OVERRIDE_SURVIVAL: OverrideSurvival = OverrideSurvival {
    survives_time_change: false,
    survives_rule_change: false,
    clobbers_own_fields: false,
};

/// How many days **back** the baked-in fetch window spans, so a freshly
/// connected provider already covers everything the calendar grid can
/// scroll to; a reconnect re-centres it.
const WINDOW_DAYS_BACK: i64 = 120;

/// How many days **ahead** the baked-in fetch window spans.
const WINDOW_DAYS_AHEAD: i64 = 400;

/// A [`Provider`] bound to one Graph calendar that refreshes its access
/// token before every network call and delegates to a freshly built
/// [`GraphCalendarProvider`]. Internal; the account layer hands callers
/// `Box<dyn Provider>` from the dial, never the concrete type.
#[derive(Debug)]
pub(crate) struct RefreshingGraphCalendarProvider {
    /// The calendar this provider syncs and writes to (the account's
    /// default).
    calendar: CalendarId,
    /// The shared, self-refreshing token source, shared with the
    /// account's mail providers so one refresh serves them all.
    tokens: Arc<GraphTokenSource>,
    /// The capabilities reported before the first call builds a delegate
    /// (calendar read + server-enforced write guard); Graph rejects a
    /// stale `If-Match` with a `412`.
    capabilities: Capabilities,
    /// The `calendarView` date range, centred on today at connect time.
    window: CalendarWindow,
    /// The host display zone sent as `Prefer: outlook.timezone`, so
    /// Graph returns each event's wall clock in it and a recurring
    /// series expands DST-correctly.
    display_zone: TimeZoneId,
    /// The account's shared TLS policy, cloned into each rebuilt Graph
    /// client.
    tls: TlsClientConfig,
    /// The built delegate, cached by the access token it was built with,
    /// so its connection pool is reused across requests; rebuilt only
    /// when the token refreshes.
    cached: Mutex<Option<(String, Arc<GraphCalendarProvider>)>>,
}

impl RefreshingGraphCalendarProvider {
    /// Binds a refreshing calendar provider to `calendar`, sharing
    /// `tokens` (and its single refresh flight) with the account's mail
    /// providers.
    pub(crate) fn new(
        calendar: CalendarId,
        tokens: Arc<GraphTokenSource>,
        window: CalendarWindow,
        display_zone: TimeZoneId,
        tls: TlsClientConfig,
    ) -> Self {
        Self {
            calendar,
            tokens,
            capabilities: Capabilities::none()
                .with_calendars()
                .with_calendar_writes(WriteGuard::Enforced, GRAPH_OVERRIDE_SURVIVAL)
                // Graph schedules server-side, with no opt-out a client
                // could reach: an RSVP stored here reaches the organiser
                // without this app sending anything. Only the *fallback*
                // set; `connection_info` reports the delegate's own once
                // one is built, but a host that reads capabilities before
                // the first token fetch must not be told this account
                // needs client-side iMIP, which would have it send a
                // second, duplicate answer.
                .with_calendar_scheduling(),
            window,
            display_zone,
            tls,
            cached: Mutex::new(None),
        }
    }

    /// The delegate provider, minting from the token source first: the
    /// delegate is cached by the token it was built with and rebuilt only
    /// when a refresh produced a new token, so its connection pool is
    /// reused across requests.
    async fn delegate(&self) -> ProviderResult<Arc<GraphCalendarProvider>> {
        let token = self.tokens.access_token().await.map_err(|err| match err {
            AccountError::SigninRejected(detail) => ProviderError::authentication(detail),
            other => ProviderError::retryable(other.to_string()),
        })?;
        {
            let cache = self
                .cached
                .lock()
                .expect("graph calendar delegate mutex poisoned");
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
        let provider = Arc::new(GraphCalendarProvider::new(
            client,
            self.calendar.clone(),
            self.window,
            self.display_zone.clone(),
        ));
        *self
            .cached
            .lock()
            .expect("graph calendar delegate mutex poisoned") =
            Some((token, Arc::clone(&provider)));
        Ok(provider)
    }

    /// Drops the cached delegate so the next [`delegate`](Self::delegate)
    /// rebuilds a fresh client; used after a retryable transport failure
    /// whose keep-alive socket is presumed dead (the token is unchanged,
    /// so this only rebuilds the client).
    fn invalidate_delegate(&self) {
        *self
            .cached
            .lock()
            .expect("graph calendar delegate mutex poisoned") = None;
    }
}

impl MailboxWrites for RefreshingGraphCalendarProvider {}

#[async_trait]
impl Provider for RefreshingGraphCalendarProvider {
    fn connection_info(&self) -> ConnectionInfo {
        self.cached
            .lock()
            .expect("graph calendar delegate mutex poisoned")
            .as_ref()
            .map_or_else(
                || ConnectionInfo::new(self.capabilities),
                |(_, provider)| provider.connection_info(),
            )
    }

    fn calendar_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::GraphCalendarList {
            account: account.clone(),
        }
    }

    fn event_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::GraphCalendar {
            account: account.clone(),
            calendar: self.calendar.clone(),
        }
    }

    async fn sync_calendars(
        &self,
        account: &AccountId,
        cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Calendar>> {
        let mut provider = self.delegate().await?;
        let mut reconnected = false;
        loop {
            match provider.sync_calendars(account, cursor).await {
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

    async fn sync_events(
        &self,
        account: &AccountId,
        cursor: Option<&SyncState>,
    ) -> ProviderResult<ScopeSync<Event>> {
        let mut provider = self.delegate().await?;
        let mut reconnected = false;
        loop {
            match provider.sync_events(account, cursor).await {
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

#[async_trait]
impl CalendarWrites for RefreshingGraphCalendarProvider {
    async fn create_event(
        &self,
        account: &AccountId,
        draft: &EventDraft,
    ) -> ProviderResult<EventWriteReceipt> {
        // A create is a non-idempotent `POST`; a blind retry could
        // double-create, so this makes a single attempt (the token is
        // minted by `delegate`). A failure surfaces to the app.
        let provider = self.delegate().await?;
        provider.create_event(account, draft).await
    }

    async fn patch_event(
        &self,
        account: &AccountId,
        base: &Event,
        edit: &EventEdit,
    ) -> ProviderResult<EventWriteReceipt> {
        // `If-Match`-guarded, so a retry after a partial success risks a
        // spurious `412`. Single attempt, like `create_event`.
        let provider = self.delegate().await?;
        provider.patch_event(account, base, edit).await
    }

    async fn delete_event(
        &self,
        account: &AccountId,
        base: Option<&Event>,
        deletion: &EventDeletion,
    ) -> ProviderResult<()> {
        let provider = self.delegate().await?;
        provider.delete_event(account, base, deletion).await
    }
}

/// Connects the Graph calendar provider a Microsoft account syncs its
/// agenda through: **one** token-refreshing provider bound to the
/// account's **default** calendar, the Graph parallel of a CalDAV
/// account's primary-calendar bind. Enumerates `/me/calendars` once with
/// a fresh token, picks the default (falling back to the first), and
/// binds a shared-token refreshing provider carrying a fetch window
/// centred on today and the host's `display_zone`.
///
/// The dial connects this only for its non-fatal effect: a calendar
/// failure (an account whose grant predates the `Calendars.ReadWrite`
/// scope 403s here) leaves the account's mail up with an empty agenda.
///
/// # Errors
///
/// Returns [`AccountError`] if the initial token refresh or the
/// calendar-list sync fails, or the account has no calendar at all.
pub async fn connect_graph_calendar_providers(
    account_id: &AccountId,
    tokens: Arc<GraphTokenSource>,
    display_zone: TimeZoneId,
    tls: &TlsClientConfig,
) -> Result<Box<dyn Provider>, AccountError> {
    let window = rolling_window()?;
    let calendars = list_calendars(account_id, &tokens, tls, window, display_zone.clone()).await?;
    let calendar = calendars
        .iter()
        .find(|calendar| calendar.is_default)
        .or_else(|| calendars.first())
        .map(|calendar| calendar.id.clone())
        .ok_or_else(|| AccountError::Graph("account has no calendar".to_owned()))?;
    Ok(Box::new(RefreshingGraphCalendarProvider::new(
        calendar,
        tokens,
        window,
        display_zone,
        tls.clone(),
    )))
}

/// Enumerates the account's calendars (a full snapshot) with a one-off
/// client on a fresh access token. The bound calendar is irrelevant to
/// the `/me/calendars` list call, so any placeholder id serves to
/// construct the probe: the parallel of the mail side's folder listing.
async fn list_calendars(
    account_id: &AccountId,
    tokens: &Arc<GraphTokenSource>,
    tls: &TlsClientConfig,
    window: CalendarWindow,
    display_zone: TimeZoneId,
) -> Result<Vec<Calendar>, AccountError> {
    let token = tokens.access_token().await?;
    let client = GraphClient::for_mailbox(token, MailboxPrincipal::Me, tls, &tokens.retry())
        .map_err(|err| AccountError::Graph(err.to_string()))?;
    let placeholder =
        CalendarId::try_from("calendar").map_err(|err| AccountError::Graph(err.to_string()))?;
    let handle = account_log_handle(account_id.as_str());
    log::debug!("graph[{handle}]: fetching the calendar list");
    let listing = GraphCalendarProvider::new(client, placeholder, window, display_zone)
        .sync_calendars(account_id, None)
        .await
        .map_err(|err| {
            // A `403` on the calendar-list probe is definitive: the token
            // lacks the calendar scope (an account connected before
            // calendar support, or revoked consent), so the user must
            // re-authenticate: not a transient outage.
            if is_calendar_access_denied(&err) {
                AccountError::CalendarAccessDenied(err.to_string())
            } else {
                AccountError::Graph(err.to_string())
            }
        })?;
    let calendars = match listing.update {
        SyncUpdate::Snapshot { objects, .. } => objects,
        SyncUpdate::Delta { changed, .. } => changed,
    };
    log::debug!(
        "graph[{handle}]: the calendar list returned {} calendar(s)",
        calendars.len(),
    );
    Ok(calendars)
}

/// The `calendarView` fetch window, centred on today: a few months back
/// and a bit over a year ahead, so a freshly connected provider covers
/// everything the calendar grid can scroll to.
fn rolling_window() -> Result<CalendarWindow, AccountError> {
    let today = OffsetDateTime::now_utc().date();
    let start = calendar_date(today - Duration::days(WINDOW_DAYS_BACK))
        .ok_or_else(|| AccountError::Graph("calendar window start out of range".to_owned()))?;
    let end = calendar_date(today + Duration::days(WINDOW_DAYS_AHEAD))
        .ok_or_else(|| AccountError::Graph("calendar window end out of range".to_owned()))?;
    Ok(CalendarWindow::new(start, end))
}

/// Whether a calendar-list probe error is an authorisation refusal (HTTP
/// `403` / `ErrorAccessDenied`) rather than a transient failure; i.e.
/// the token lacks the calendar scope. Matched on the provider error's
/// rendered form (Graph surfaces both the status and the
/// `ErrorAccessDenied` code), the only place the HTTP status reaches
/// this layer.
fn is_calendar_access_denied(err: &ProviderError) -> bool {
    let text = err.to_string();
    text.contains("403") || text.contains("ErrorAccessDenied")
}

#[cfg(test)]
#[path = "calendar_tests.rs"]
mod tests;
