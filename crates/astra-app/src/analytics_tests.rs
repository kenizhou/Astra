// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the analytics recorder: the consent gate both ways (no
//! consent, no row, no error; consent standing and the row landing with
//! its counts), and the one-click withdrawal stopping the flow at the
//! next call.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use astra_ai::AiStore;

use crate::calendar_fakes::CalendarFake;
use crate::settings_fakes::RecordingObserver;
use crate::{AnalyticsEvent, App, Intent, SettingsIntent};

/// An app over no accounts, the shape a recorder test needs: the consent
/// preference and the installed ledger, nothing else.
fn app() -> App<CalendarFake> {
    let app = App::new(
        Arc::new(engine_api::Engine::open_in_memory().unwrap()),
        Vec::new(),
        None,
        Arc::new(RecordingObserver {
            surfaces: Arc::new(Mutex::new(Vec::new())),
        }),
    );
    app.install_analytics_store(Arc::new(AiStore::open_in_memory().unwrap()));
    app
}

/// One sync-pass-shaped event, the shape the host's own reports use.
fn event(shape: &str) -> AnalyticsEvent {
    AnalyticsEvent::of(shape)
        .with_count("messages", 12)
        .with_count("accounts", 2)
}

/// The ledger's rows, read back through the store the app writes through.
fn rows(app: &App<CalendarFake>) -> Vec<astra_ai::AnalyticsEventRow> {
    app.analytics
        .lock()
        .expect("analytics mutex poisoned")
        .as_ref()
        .expect("the test installs a ledger")
        .recent_analytics_events(100)
        .expect("the read runs")
}

/// Grants or withdraws consent, the one click each is.
async fn consent(app: &App<CalendarFake>, granted: bool) {
    app.dispatch(Intent::Settings(SettingsIntent::SetAnalyticsConsented {
        consented: granted,
    }))
    .await
    .unwrap();
}

#[tokio::test]
async fn no_consent_means_no_row_and_no_error() {
    let app = app();
    // The default consent never stood, so the recorder answers nothing:
    // no row lands, and the call itself cannot fail the host's flow (the
    // method answers no error at all).
    app.record_analytics(&event("sync_pass"));

    assert!(
        rows(&app).is_empty(),
        "no consent, no row: the ledger stays empty"
    );
}

#[tokio::test]
async fn consent_stands_and_the_row_lands_with_its_counts() {
    let app = app();
    consent(&app, true).await;

    app.record_analytics(&event("sync_pass"));

    let landed = rows(&app);
    assert_eq!(landed.len(), 1, "one row lands per recorded event");
    assert_eq!(landed[0].shape, "sync_pass");
    assert_eq!(
        landed[0].counts,
        BTreeMap::from([("messages".to_owned(), 12), ("accounts".to_owned(), 2),]),
        "the named counts land verbatim, content never"
    );
}

#[tokio::test]
async fn the_one_click_withdrawal_stops_the_flow_at_the_next_call() {
    let app = app();
    consent(&app, true).await;
    app.record_analytics(&event("sync_pass"));
    assert_eq!(rows(&app).len(), 1);

    // The withdrawal is the preference write itself: one click, and the
    // next call records nothing.
    consent(&app, false).await;
    app.record_analytics(&event("sync_pass"));
    let after_withdrawal = rows(&app);
    assert_eq!(after_withdrawal.len(), 1, "the withdrawn call lands no row");

    // And a consent granted again records again: the gate reads the live
    // preference on every call, not a state latched at boot.
    consent(&app, true).await;
    app.record_analytics(&event("app_launch"));
    assert_eq!(rows(&app).len(), 2, "the re-granted consent records again");
}

#[tokio::test]
async fn an_app_with_no_ledger_installed_records_nothing() {
    let app = App::new(
        Arc::new(engine_api::Engine::open_in_memory().unwrap()),
        Vec::new(),
        None,
        Arc::new(RecordingObserver {
            surfaces: Arc::new(Mutex::new(Vec::new())),
        }),
    );
    consent(&app, true).await;

    // No ledger installed: the recorder records nothing and the call
    // cannot fail, the same shape an app with no account store renders
    // no rows with.
    app.record_analytics(&event("sync_pass"));
}
