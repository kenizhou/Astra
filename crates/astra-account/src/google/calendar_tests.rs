// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the refreshing Google calendar provider: the capability set
//! it answers before any delegate exists, the bound scopes, and the
//! rolling fetch window.

use engine_api::{AccountId, CalendarId, Provider, SyncScope, WriteGuard};
use time::{Duration, OffsetDateTime};

use super::{
    rolling_window, RefreshingGoogleCalendarProvider, WINDOW_DAYS_AHEAD, WINDOW_DAYS_BACK,
};
use crate::google::provider::calendar_date;
use crate::google::test_support::google_source_at;
use crate::graph::token_source::test_support::mock_token_endpoint;

/// The account's TLS policy with no accepted exceptions: the shape every
/// offline provider test dials under.
fn tls() -> engine_tls::TlsClientConfig {
    engine_tls::client_config_with_exceptions(&crate::setup_trust_policy(), &[]).unwrap()
}

#[test]
fn binds_a_calendar_provider_with_enforced_writes_and_the_bound_scopes() {
    let (endpoint, _hits) = mock_token_endpoint(vec![]);
    let source = google_source_at(endpoint, None, None);
    let account = AccountId::try_from("alice@gmail.com@google").unwrap();
    let calendar = CalendarId::try_from("primary").unwrap();
    let provider = RefreshingGoogleCalendarProvider::new(
        calendar.clone(),
        source,
        rolling_window().unwrap(),
        tls(),
    );
    // Calendar capability with a server-enforced write guard, and no
    // mail capability.
    let info = provider.connection_info();
    assert!(info.capabilities.calendars());
    assert!(!info.capabilities.mail());
    assert_eq!(
        info.capabilities.calendar_write_guard(),
        Some(WriteGuard::Enforced),
    );
    // Reported before any delegate exists, and it has to be: a host
    // that read `false` here would conclude the answer needs an iMIP
    // message of its own and send one beside the one Google's
    // `sendUpdates` already sent.
    assert!(info.capabilities.calendar_scheduling());
    // The event scope names the bound calendar; the calendar-list scope
    // is account-wide.
    assert_eq!(
        provider.event_scope(&account),
        SyncScope::GoogleCalendar {
            account: account.clone(),
            calendar,
        },
    );
    assert_eq!(
        provider.calendar_scope(&account),
        SyncScope::GoogleCalendarList { account },
    );
}

#[test]
fn the_fetch_window_brackets_today() {
    let today = OffsetDateTime::now_utc().date();
    let window = rolling_window().unwrap();
    // Start is before today and end after it, so a freshly connected
    // provider covers the current week the grid opens on.
    assert_eq!(
        window.start,
        calendar_date(today - Duration::days(WINDOW_DAYS_BACK)).unwrap(),
    );
    assert_eq!(
        window.end,
        calendar_date(today + Duration::days(WINDOW_DAYS_AHEAD)).unwrap(),
    );
}
