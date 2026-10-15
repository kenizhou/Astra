// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the refreshing Graph calendar provider: the capability set
//! it answers before any delegate exists, the scope-denied probe
//! classification, and the rolling fetch window.

use engine_api::{AccountId, CalendarId, Provider, SyncScope, TimeZoneId, WriteGuard};
use engine_provider::ProviderError;
use time::{Duration, OffsetDateTime};

use super::{
    is_calendar_access_denied, rolling_window, RefreshingGraphCalendarProvider, WINDOW_DAYS_AHEAD,
    WINDOW_DAYS_BACK,
};
use crate::graph::provider::calendar_date;
use crate::graph::token_source::test_support::{mock_token_endpoint, source_at};

/// The account's TLS policy with no accepted exceptions: the shape every
/// offline provider test dials under.
fn tls() -> engine_tls::TlsClientConfig {
    engine_tls::client_config_with_exceptions(&crate::setup_trust_policy(), &[]).unwrap()
}

#[test]
fn binds_a_calendar_provider_with_enforced_writes_and_the_bound_scopes() {
    let (endpoint, _hits) = mock_token_endpoint(vec![]);
    let source = source_at(endpoint, None);
    let account = AccountId::try_from("alice@example.com@graph.microsoft.com").unwrap();
    let calendar = CalendarId::try_from("cal-123").unwrap();
    let provider = RefreshingGraphCalendarProvider::new(
        calendar.clone(),
        source,
        rolling_window().unwrap(),
        TimeZoneId::utc(),
        tls(),
    );
    // Calendar capability with a server-enforced write guard, and no mail capability.
    let info = provider.connection_info();
    assert!(info.capabilities.calendars());
    assert!(!info.capabilities.mail());
    assert_eq!(
        info.capabilities.calendar_write_guard(),
        Some(WriteGuard::Enforced),
    );
    // Reported before any delegate exists, and it has to be: a host that read `false`
    // here would conclude the answer needs an iMIP message of its own and send one
    // *beside* the one Graph already sent, so the organiser gets the reply twice.
    assert!(info.capabilities.calendar_scheduling());
    // The event scope names the bound calendar; the calendar-list scope is
    // account-wide.
    assert_eq!(
        provider.event_scope(&account),
        SyncScope::GraphCalendar {
            account: account.clone(),
            calendar,
        },
    );
    assert_eq!(
        provider.calendar_scope(&account),
        SyncScope::GraphCalendarList { account },
    );
}

#[test]
fn a_403_calendar_probe_is_classified_as_access_denied() {
    // A `403`/`ErrorAccessDenied` means the token lacks the calendar scope, so the
    // account must re-consent rather than wait out an outage.
    let denied = ProviderError::retryable(
        "Graph HTTP 403 (code Some(\"ErrorAccessDenied\")): access is denied",
    );
    assert!(is_calendar_access_denied(&denied));
    // A transient `500` is not a re-consent case; it badges nothing and retries.
    let transient = ProviderError::retryable("Graph HTTP 500 (code ErrorInternalServerError)");
    assert!(!is_calendar_access_denied(&transient));
}

#[test]
fn the_fetch_window_brackets_today() {
    let today = OffsetDateTime::now_utc().date();
    let window = rolling_window().unwrap();
    // Start is before today and end after it, so a freshly connected provider covers
    // the current week the grid opens on.
    assert_eq!(
        window.start,
        calendar_date(today - Duration::days(WINDOW_DAYS_BACK)).unwrap(),
    );
    assert_eq!(
        window.end,
        calendar_date(today + Duration::days(WINDOW_DAYS_AHEAD)).unwrap(),
    );
}
