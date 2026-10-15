// SPDX-License-Identifier: GPL-3.0-only
//! The completion's refusal bookkeeping: what a dialled grant leaves
//! standing in the refusal map the surfaces read. A withheld calendar
//! records the reconnect state; a healthy grant clears whatever stood;
//! a refused completion leaves the standing state exactly as it was
//! (the rollback rule the registration follows). Split from the graph
//! add tests for the 500-line cap.

use super::*;

/// The dial seam answering mail alone and a withheld calendar, the
/// 403 probe's outcome.
fn withheld_dial(saw: Arc<DialSaw>) -> GraphDial {
    dial_seam(saw, |_| async {
        Ok(astra_account::MicrosoftProviders {
            mail: vec![FakeGraph::mail()],
            calendar: None,
            calendar_error: Some("calendar access denied: the probe was refused".to_owned()),
            calendar_reauth_required: true,
        })
    })
}

#[test]
fn a_completed_sign_in_records_what_the_new_grant_bears() {
    let host = Host::new("graph-add-refusal-withheld");
    let id = graph_id("alice@example.test");

    let added = complete(
        &host,
        lookup_seam("alice@example.test"),
        withheld_dial(Arc::new(DialSaw::default())),
        production_source(),
    )
    .expect("the mail half stands");

    assert!(added.calendar_withheld, "the outcome carries the state");
    assert_eq!(
        host.refusals.of(&id),
        Some(astra_viewmodel::GrantState::CalendarWithheld),
        "the standing refusal state is recorded for the surfaces"
    );
}

#[test]
fn a_healthy_reconsent_clears_the_standing_refusal() {
    let host = Host::new("graph-add-refusal-cleared");
    register_stored(&host, stored_config(), "alice@example.test");
    let id = graph_id("alice@example.test");
    host.refusals
        .record(&id, astra_viewmodel::GrantState::CalendarWithheld);

    complete(
        &host,
        lookup_seam("alice@example.test"),
        ok_dial(Arc::new(DialSaw::default())),
        production_source(),
    )
    .expect("the re-grant lands");

    assert_eq!(
        host.refusals.of(&id),
        None,
        "the fresh grant bears the calendar, so no refusal stands"
    );
}

#[test]
fn a_refused_completion_leaves_the_standing_refusal_as_it_was() {
    let host = Host::new("graph-add-refusal-stands");
    register_stored(&host, stored_config(), "alice@example.test");
    let id = graph_id("alice@example.test");
    host.refusals
        .record(&id, astra_viewmodel::GrantState::SignInExpired);

    complete(
        &host,
        lookup_seam("alice@example.test"),
        failing_dial(Arc::new(DialSaw::default()), "listing mailboxes: refused"),
        production_source(),
    )
    .expect_err("the dial refuses");

    assert_eq!(
        host.refusals.of(&id),
        Some(astra_viewmodel::GrantState::SignInExpired),
        "a refused completion changes nothing about the standing state"
    );
}
