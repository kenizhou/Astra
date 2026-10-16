// SPDX-License-Identifier: GPL-3.0-only
//! The boot's graph refusal bookkeeping: what the stored-account dial
//! leaves in the refusal map for the surfaces, and the assembly's
//! settings priming that makes the first pull carry it. Split from the
//! boot tests for the 500-line cap, sharing their fixtures through
//! `super`.

use super::{graph_config, graph_dial, FakeDav, QuietDraftSink, QuietObserver};

/// Runs one boot's stored-account dial over `dials` with the given graph
/// dial and the refusal map the boot records into, on a test runtime.
fn boot_dials_recording(
    dials: &std::sync::Arc<crate::AccountDials>,
    dial: crate::oauth::graph_add::GraphDial,
    refusals: &std::sync::Arc<crate::oauth::grant_refusals::GrantRefusals>,
) -> Vec<super::super::stored::Dialled> {
    let runtime = crate::runtime::build_runtime().expect("the test runtime builds");
    let store: std::sync::Arc<dyn crate::credential_store::AccountCredentialStore> =
        crate::commands_fakes::FakeStore::accepting();
    let dials = std::sync::Arc::clone(dials);
    let refusals = std::sync::Arc::clone(refusals);
    runtime.block_on(move || async move {
        super::super::stored::dial_stored(
            &dials,
            &store,
            dial,
            super::google_boot::unused_dial(),
            &refusals,
        )
        .await
    })
}

/// A registered graph account under one scratch index, the fixture the
/// refusal-state tests share.
fn registered_graph_account(name: &str) -> std::sync::Arc<crate::AccountDials> {
    let dials = std::sync::Arc::new(crate::AccountDials::empty(&crate::commands_fakes::scratch(
        name,
    )));
    let account = engine_api::AccountId::try_from("alice@example.test@graph").unwrap();
    dials
        .register(
            &account,
            graph_config(),
            astra_account::ImapConnections::new(),
        )
        .expect("the graph account registers");
    dials
}

#[test]
fn a_dead_grant_at_boot_records_the_expired_sign_in_for_the_skipped_account() {
    let dials = registered_graph_account("boot-graph-dead");
    let refusals = std::sync::Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());
    let dead: crate::oauth::graph_add::GraphDial =
        std::sync::Arc::new(move |_account, _tokens, _config| {
            Box::pin(async move {
                Err(astra_account::AccountError::SigninRejected(
                    "the refresh token was refused".to_owned(),
                ))
            })
        });

    let dialled = boot_dials_recording(&dials, dead, &refusals);

    assert!(dialled.is_empty(), "the account never joins the app");
    assert_eq!(
        refusals.of("alice@example.test@graph"),
        Some(astra_viewmodel::GrantState::SignInExpired),
        "the refusal state names why the account is absent, never an outage"
    );
}

#[test]
fn a_withheld_calendar_at_boot_records_the_reconnect_state() {
    let dials = registered_graph_account("boot-graph-withheld");
    let refusals = std::sync::Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());
    let withheld: crate::oauth::graph_add::GraphDial =
        std::sync::Arc::new(move |_account, _tokens, _config| {
            Box::pin(async move {
                Ok(astra_account::MicrosoftProviders {
                    mail: vec![FakeDav::mail()],
                    calendar: None,
                    calendar_error: Some(
                        "calendar access denied: the probe was refused".to_owned(),
                    ),
                    calendar_reauth_required: true,
                })
            })
        });

    let dialled = boot_dials_recording(&dials, withheld, &refusals);

    assert_eq!(
        dialled.len(),
        1,
        "the mail half stands without the calendar"
    );
    assert_eq!(
        refusals.of("alice@example.test@graph"),
        Some(astra_viewmodel::GrantState::CalendarWithheld),
        "the standing refusal state rides the settings rows, not a log line"
    );
}

#[test]
fn a_healthy_graph_dial_records_no_refusal_state() {
    let dials = registered_graph_account("boot-graph-healthy");
    let refusals = std::sync::Arc::new(crate::oauth::grant_refusals::GrantRefusals::new());

    let dialled = boot_dials_recording(&dials, graph_dial(Ok(())), &refusals);

    assert_eq!(dialled.len(), 1);
    assert_eq!(
        refusals.of("alice@example.test@graph"),
        None,
        "a healthy grant crosses no refusal state"
    );
}

#[test]
fn the_assembly_primes_the_settings_snapshot_for_the_first_pull() {
    // The settings snapshot is pull-based: nothing republishes it until a
    // settings intent lands, so a boot that does not prime it leaves the
    // first pull answering the empty default, refusal states and all.
    let data_dir = crate::commands_fakes::scratch("boot-prime");
    std::fs::create_dir_all(&data_dir).expect("the scratch directory creates");
    let (_relay, agent_sink, _data) =
        crate::agent::testkit::stub_relay("boot-prime", "echo", 3, 50);
    let state = super::super::assemble_state(
        &data_dir,
        std::sync::Arc::new(QuietObserver),
        agent_sink,
        std::sync::Arc::new(QuietDraftSink),
        crate::oauth::wiring::offline_surface(),
    )
    .expect("the assembly boots over an empty store");

    let snapshot = state.app.settings();
    assert!(
        !snapshot.about.version.is_empty(),
        "the primed snapshot answers the about block, not the default's empty word"
    );
    assert!(
        snapshot.about.attributions.len() >= 3,
        "the primed snapshot answers the attributions"
    );
}
