// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the standing grant refusals: what each writer leaves in the
//! map the refusal surfaces read.

use astra_viewmodel::GrantState;

use super::GrantRefusals;

#[test]
fn a_dead_grant_records_the_expired_sign_in() {
    let refusals = GrantRefusals::new();

    refusals.record_dead("alice@example.test@graph");

    assert_eq!(
        refusals.of("alice@example.test@graph"),
        Some(GrantState::SignInExpired)
    );
}

#[test]
fn a_dialled_grant_records_what_it_bears_and_a_healthy_one_clears() {
    let refusals = GrantRefusals::new();
    refusals.record("alice@example.test@graph", GrantState::CalendarWithheld);

    refusals.record_dialled("bob@example.test@graph", true);
    refusals.record_dialled("alice@example.test@graph", false);

    assert_eq!(
        refusals.of("bob@example.test@graph"),
        Some(GrantState::CalendarWithheld),
        "the withheld calendar records the reconnect state"
    );
    assert_eq!(
        refusals.of("alice@example.test@graph"),
        None,
        "a dial that answered clears the standing state it disproved"
    );
}

#[test]
fn an_unknown_account_carries_no_state() {
    let refusals = GrantRefusals::new();

    assert_eq!(refusals.of("nobody@graph"), None);
}
