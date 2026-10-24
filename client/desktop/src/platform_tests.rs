// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the platform wiring: the boot zone report's dispatch
//! through the command surface's seam (the resolved id crossing on
//! the report arm, an unresolvable answer crossing nothing) and the
//! first product event's shape.

use std::sync::Arc;

use astra_app::{Intent, SettingsIntent};

use super::{app_open_event, report_zone_with};
use crate::commands_fakes::{state, FakeApp, FakeStore};

#[test]
fn the_boot_report_dispatches_the_resolved_zone() {
    let app = FakeApp::new();
    let state = state("zone-report", Arc::clone(&app), FakeStore::accepting());

    report_zone_with(&state, || Some("Europe/Berlin".to_owned()));

    let dispatched = app.dispatched.lock().unwrap();
    assert!(
        matches!(
            dispatched.as_slice(),
            [Intent::Settings(SettingsIntent::ReportDeviceZone { zone })]
                if zone == "Europe/Berlin"
        ),
        "the resolved zone crosses on the report arm: {dispatched:?}"
    );
}

#[test]
fn an_unresolvable_zone_reports_nothing() {
    let app = FakeApp::new();
    let state = state(
        "zone-unresolvable",
        Arc::clone(&app),
        FakeStore::accepting(),
    );

    report_zone_with(&state, || None);

    assert!(
        app.dispatched.lock().unwrap().is_empty(),
        "a resolution that answers nothing dispatches nothing"
    );
}

#[test]
fn the_app_open_event_carries_the_account_count() {
    let event = app_open_event(2);
    assert_eq!(event.shape, "app_open");
    assert_eq!(
        event.counts.get("accounts").copied(),
        Some(2),
        "the count is the standing accounts: {:?}",
        event.counts
    );
}
