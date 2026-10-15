// SPDX-License-Identifier: GPL-3.0-only
//! The refreshing Google calendar provider: the engine's calendar-bound
//! [`GoogleCalendarProvider`] behind the same shared token source as the
//! account's Gmail provider.
//!
//! Like the mail side, the engine's provider takes a **static** bearer
//! token, so [`RefreshingGoogleCalendarProvider`] mints a fresh access
//! token before each network call and delegates to a freshly built
//! provider (reusing its warm connection pool while the token is
//! unchanged, rebuilt only when the token refreshes). The provider is
//! bound to **one** calendar (the account's primary) and carries a
//! baked-in fetch window centred on today, re-centred on each reconnect.
//!
//! Two things are simpler than the Graph calendar. Google is
//! **IANA-native** (event times carry an IANA `timeZone`), so there is
//! no display-zone `Prefer` header to thread through. And Google asks
//! for the calendar scope at connect time beside mail, so there is no
//! re-consent case to detect: a calendar failure is simply non-fatal
//! (the `403` stays an error, and the re-auth banner is
//! Microsoft-only).
//!
//! **Reads** (`sync_calendars` / `sync_events`) are idempotent, so they
//! get the one-shot reconnect loop the mail providers use. **Writes**
//! (`create` / `patch` / `delete`) are single-attempt: a blind retry of
//! a non-idempotent call could double-create, and the app surfaces a
//! failed write for the user to retry.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use engine_api::{
    AccountId, Calendar, CalendarId, CalendarWrites, Capabilities, Event, EventDeletion,
    EventDraft, EventEdit, EventWriteReceipt, MailboxWrites, OverrideSurvival, Provider, SyncScope,
    WriteGuard,
};
use engine_core::sync::{SyncState, SyncUpdate};
use engine_provider::{ConnectionInfo, ProviderError, ProviderResult, ScopeSync};
use engine_tls::TlsClientConfig;
use provider_google::{CalendarWindow, GoogleCalendarProvider, GoogleClient};
use time::{Duration, OffsetDateTime};

use super::provider::{calendar_date, should_reconnect};
use crate::{account_log_handle, AccountError, GraphTokenSource};

/// What a Google series edit costs the occurrences the user changed by
/// hand: moving the series' time destroys them, and renaming the series
/// renames the one they had renamed. Only a rule change leaves them
/// alone.
///
/// The **fallback** copy of the engine adapter's own constant, for the
/// window before a delegate exists; `connection_info` reports the
/// delegate's once one is built, and that is the value the engine's live
/// suite measures against the real server. A host reading capabilities
/// before the first token fetch must not be told a Google series edit is
/// free.
const GOOGLE_OVERRIDE_SURVIVAL: OverrideSurvival = OverrideSurvival {
    survives_time_change: false,
    survives_rule_change: true,
    clobbers_own_fields: true,
};

/// How many days **back** the baked-in fetch window spans, so a freshly
/// connected provider already covers everything the calendar grid can
/// scroll to; a reconnect re-centres it.
const WINDOW_DAYS_BACK: i64 = 120;

/// How many days **ahead** the baked-in fetch window spans.
const WINDOW_DAYS_AHEAD: i64 = 400;

/// A [`Provider`] bound to one Google calendar that refreshes its access
/// token before every network call and delegates to a freshly built
/// [`GoogleCalendarProvider`]. Internal; the account layer hands callers
/// `Box<dyn Provider>` from [`connect_google_calendar_providers`].
pub(crate) struct RefreshingGoogleCalendarProvider {
    /// The calendar this provider syncs and writes to (the account's
    /// primary).
    calendar: CalendarId,
    /// The shared, self-refreshing token source, shared with the
    /// account's Gmail provider so one refresh serves both.
    tokens: Arc<GraphTokenSource>,
    /// Calendar read plus a server-enforced write guard; Google rejects
    /// a stale `If-Match` with a `412`, exactly like Graph.
    capabilities: Capabilities,
    /// The event fetch window, centred on today at connect time.
    window: CalendarWindow,
    /// The account's shared TLS policy, cloned into each rebuilt Google
    /// client.
    tls: TlsClientConfig,
    /// The built delegate, cached by the access token it was built with,
    /// so its connection pool is reused across requests; rebuilt only
    /// when the token refreshes.
    cached: Mutex<Option<(String, Arc<GoogleCalendarProvider>)>>,
}

impl core::fmt::Debug for RefreshingGoogleCalendarProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The cached token is a live credential: the debug form names the
        // bound calendar and nothing else.
        f.debug_struct("RefreshingGoogleCalendarProvider")
            .field("calendar", &self.calendar)
            .finish_non_exhaustive()
    }
}

impl RefreshingGoogleCalendarProvider {
    /// Binds a refreshing calendar provider to `calendar`, sharing
    /// `tokens` (and its single refresh flight) with the account's Gmail
    /// provider.
    #[must_use]
    pub(crate) fn new(
        calendar: CalendarId,
        tokens: Arc<GraphTokenSource>,
        window: CalendarWindow,
        tls: TlsClientConfig,
    ) -> Self {
        Self {
            calendar,
            tokens,
            capabilities: Capabilities::none()
                .with_calendars()
                .with_calendar_writes(WriteGuard::Enforced, GOOGLE_OVERRIDE_SURVIVAL)
                // Google schedules server-side (`sendUpdates`), so the
                // answer travels by itself. Only the *fallback* set;
                // `connection_info` reports the delegate's own once one
                // is built, but a host that reads capabilities before the
                // first token fetch must not be told this account needs
                // client-side iMIP, which would have it send a second,
                // duplicate answer.
                .with_calendar_scheduling(),
            window,
            tls,
            cached: Mutex::new(None),
        }
    }

    /// The delegate provider, minting from the token source first: the
    /// delegate is cached by the token it was built with and rebuilt only
    /// when a refresh produced a new token, so its connection pool is
    /// reused across requests.
    async fn delegate(&self) -> ProviderResult<Arc<GoogleCalendarProvider>> {
        let token = self.tokens.access_token().await.map_err(|err| match err {
            AccountError::SigninRejected(detail) => ProviderError::authentication(detail),
            other => ProviderError::retryable(other.to_string()),
        })?;
        {
            let cache = self
                .cached
                .lock()
                .expect("google calendar delegate mutex poisoned");
            if let Some((cached_token, provider)) = cache.as_ref() {
                if *cached_token == token {
                    return Ok(Arc::clone(provider));
                }
            }
        }
        let client = GoogleClient::connect(token.clone(), &self.tls, &self.tokens.retry())
            .map_err(ProviderError::from)?;
        let provider = Arc::new(
            GoogleCalendarProvider::new(client, self.calendar.clone()).with_window(self.window),
        );
        *self
            .cached
            .lock()
            .expect("google calendar delegate mutex poisoned") =
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
            .expect("google calendar delegate mutex poisoned") = None;
    }
}

impl MailboxWrites for RefreshingGoogleCalendarProvider {}

#[async_trait]
impl Provider for RefreshingGoogleCalendarProvider {
    fn connection_info(&self) -> ConnectionInfo {
        self.cached
            .lock()
            .expect("google calendar delegate mutex poisoned")
            .as_ref()
            .map_or_else(
                || ConnectionInfo::new(self.capabilities),
                |(_, provider)| provider.connection_info(),
            )
    }

    fn calendar_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::GoogleCalendarList {
            account: account.clone(),
        }
    }

    fn event_scope(&self, account: &AccountId) -> SyncScope {
        SyncScope::GoogleCalendar {
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
impl CalendarWrites for RefreshingGoogleCalendarProvider {
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

/// Connects the Google calendar provider a Google account syncs its
/// agenda through: **one** token-refreshing provider bound to the
/// account's **primary** calendar. Enumerates `calendarList` once with a
/// fresh token, picks the primary (falling back to the first), and binds
/// a shared-token refreshing provider carrying a fetch window centred on
/// today.
///
/// The dial connects this only for its non-fatal effect: a calendar
/// failure leaves the account's mail up with an empty agenda. There is
/// no re-auth classification to make on the way out: Google asks for the
/// calendar scope at connect time, so a refused calendar is an error,
/// never a banner.
///
/// # Errors
///
/// Returns [`AccountError`] if the initial token refresh or the
/// calendar-list sync fails, or the account has no calendar at all.
pub async fn connect_google_calendar_providers(
    account_id: &AccountId,
    tokens: Arc<GraphTokenSource>,
    tls: &TlsClientConfig,
) -> Result<Box<dyn Provider>, AccountError> {
    let window = rolling_window()?;
    let calendars = list_calendars(account_id, &tokens, tls, window).await?;
    let calendar = calendars
        .iter()
        .find(|calendar| calendar.is_default)
        .or_else(|| calendars.first())
        .map(|calendar| calendar.id.clone())
        .ok_or_else(|| AccountError::Google("account has no calendar".to_owned()))?;
    Ok(Box::new(RefreshingGoogleCalendarProvider::new(
        calendar,
        tokens,
        window,
        tls.clone(),
    )))
}

/// Enumerates the account's calendars (a full snapshot) with a one-off
/// client on a fresh access token: the parallel of the mail side's label
/// listing.
async fn list_calendars(
    account_id: &AccountId,
    tokens: &Arc<GraphTokenSource>,
    tls: &TlsClientConfig,
    window: CalendarWindow,
) -> Result<Vec<Calendar>, AccountError> {
    let token = tokens.access_token().await?;
    let client = GoogleClient::connect(token, tls, &tokens.retry())
        .map_err(|err| AccountError::Google(err.to_string()))?;
    // The `calendarList` call ignores the bound calendar, so any
    // placeholder id serves to construct the probe (the `primary` alias
    // Google always accepts).
    let placeholder =
        CalendarId::try_from("primary").map_err(|err| AccountError::Google(err.to_string()))?;
    let handle = account_log_handle(account_id.as_str());
    log::debug!("google[{handle}]: fetching the calendar list");
    let listing = GoogleCalendarProvider::new(client, placeholder)
        .with_window(window)
        .sync_calendars(account_id, None)
        .await
        .map_err(|err| AccountError::Google(err.to_string()))?;
    let calendars = match listing.update {
        SyncUpdate::Snapshot { objects, .. } => objects,
        SyncUpdate::Delta { changed, .. } => changed,
    };
    log::debug!(
        "google[{handle}]: the calendar list returned {} calendar(s)",
        calendars.len(),
    );
    Ok(calendars)
}

/// The event fetch window, centred on today: a few months back and a bit
/// over a year ahead, so a freshly connected provider covers everything
/// the calendar grid can scroll to.
fn rolling_window() -> Result<CalendarWindow, AccountError> {
    let today = OffsetDateTime::now_utc().date();
    let start = calendar_date(today - Duration::days(WINDOW_DAYS_BACK))
        .ok_or_else(|| AccountError::Google("calendar window start out of range".to_owned()))?;
    let end = calendar_date(today + Duration::days(WINDOW_DAYS_AHEAD))
        .ok_or_else(|| AccountError::Google("calendar window end out of range".to_owned()))?;
    Ok(CalendarWindow::new(start, end))
}

#[cfg(test)]
#[path = "calendar_tests.rs"]
mod tests;
