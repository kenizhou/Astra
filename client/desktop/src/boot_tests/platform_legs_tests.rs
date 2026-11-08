// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the platform legs' boot wiring over the real assembly:
//! the analytics ledger the assembly installs (the consent gate
//! holding over the real core, the row landing only once consent
//! stands) and the boot zone report publishing its offer on the
//! settings snapshot.

use std::sync::Arc;

use astra_app::{Intent, SettingsIntent};

use super::{QuietDraftSink, QuietObserver};

/// One assembly over a scratch directory of its own, with the data
/// directory handed back beside the state for the probes that read
/// the ledger file directly.
fn assembled(name: &str) -> (crate::app_state::AppState, std::path::PathBuf) {
    let data_dir = std::env::temp_dir().join(format!(
        "astra-desktop-boot-platform-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).expect("the scratch directory creates");
    let (_relay, agent_sink, _data) = crate::agent::testkit::stub_relay(name, "echo", 3, 50);
    let state = super::super::assemble_state(
        &data_dir,
        Arc::new(QuietObserver),
        agent_sink,
        Arc::new(QuietDraftSink),
        crate::oauth::wiring::offline_surface(),
    )
    .expect("the assembly boots over an empty store");
    (state, data_dir)
}

#[test]
fn the_assembly_installs_the_analytics_ledger_behind_the_consent_gate() {
    let (state, data_dir) = assembled("analytics");
    let ledger = astra_ai::AiStore::open(&data_dir.join("ai.sqlite")).expect("the ledger opens");

    // Without consent the boot's own event records nothing: no row
    // lands, and the flow never fails over the decline.
    let accounts = state.connections.ids().len();
    state
        .app
        .record_analytics(&crate::platform::app_open_event(accounts));
    assert!(
        ledger
            .recent_analytics_events(10)
            .expect("the ledger reads")
            .is_empty(),
        "no consent stands, so no row lands"
    );

    // Consent lands through the settings intent, and the boot's own
    // event records: the gate reads the preference live.
    state.runtime.block_on(|| {
        let app = Arc::clone(&state.app);
        async move {
            app.dispatch(Intent::Settings(SettingsIntent::SetAnalyticsConsented {
                consented: true,
            }))
            .await
            .expect("the consent dispatch lands");
        }
    });
    state
        .app
        .record_analytics(&crate::platform::app_open_event(accounts));

    let rows = ledger
        .recent_analytics_events(10)
        .expect("the ledger reads");
    assert_eq!(rows.len(), 1, "exactly the consented row lands");
    assert_eq!(rows[0].shape, "app_open");
    assert_eq!(
        rows[0].counts.get("accounts").copied(),
        Some(accounts as u64),
        "the boot's own count rides the row"
    );

    // The one-click withdrawal is the same write turned off, and the
    // next record answers nothing: the flow stops at the next call.
    state.runtime.block_on(|| {
        let app = Arc::clone(&state.app);
        async move {
            app.dispatch(Intent::Settings(SettingsIntent::SetAnalyticsConsented {
                consented: false,
            }))
            .await
            .expect("the withdrawal dispatch lands");
        }
    });
    state
        .app
        .record_analytics(&crate::platform::app_open_event(accounts));
    let after = ledger
        .recent_analytics_events(10)
        .expect("the ledger reads");
    assert_eq!(after.len(), 1, "the withdrawn call lands no row: {after:?}");
}

#[test]
fn the_boot_zone_report_publishes_the_offer_over_the_real_core() {
    let (state, _data_dir) = assembled("zone");

    crate::platform::report_zone_with(&state, || Some("Europe/Berlin".to_owned()));

    assert_eq!(
        state.app.settings().device_zone_offer,
        Some("Europe/Berlin".to_owned()),
        "the boot report's zone validates, latches, and publishes the offer"
    );
}
