// SPDX-License-Identifier: GPL-3.0-only
//! The dial failure's rollback rules: a new add's pre-registration goes
//! back whole (entry, index, credential, no join), and a re-consent's
//! standing entry is restored exactly as it stood, because a failed dial
//! must leave a live account's registration alone.

use super::*;

#[test]
fn a_failed_dial_rolls_a_new_add_back_whole() {
    let host = Host::new("google-add-dial-failed");

    let failure = complete(
        &host,
        lookup_seam("alice@example.test"),
        failing_dial(Arc::new(DialSaw::default()), "google: unreachable"),
        production_google_source(),
    )
    .expect_err("the dial's failure is the completion's");

    assert_eq!(failure.refusal, RefusalKind::Indeterminate);
    assert!(
        failure.detail.contains("unreachable"),
        "the dial's own sentence rides in the detail: {}",
        failure.detail
    );
    assert!(
        host.dials.ids().is_empty() && host.dials.indexed_ids().is_empty(),
        "the pre-registered entry is taken back"
    );
    assert!(
        host.store
            .stored(&google_id("alice@example.test"))
            .is_none(),
        "no credential survives a dial that never answered"
    );
    assert!(host.app.added.lock().unwrap().is_empty());
}

#[test]
fn a_failed_dial_on_a_reconsent_restores_the_standing_entry() {
    let host = Host::new("google-add-reconsent-dial-failed");
    register_stored(&host, stored_config(), "Alice@Example.com");
    let id = google_id("Alice@Example.com");

    complete(
        &host,
        lookup_seam("Alice@Example.com"),
        failing_dial(Arc::new(DialSaw::default()), "google: unreachable"),
        production_google_source(),
    )
    .expect_err("the failed dial refuses the re-consent");

    let entry = host.dials.of(&id).expect("the standing entry stays").0;
    assert_eq!(
        entry
            .google
            .expect("the google section stands")
            .refresh_token
            .expose(),
        "old-refresh",
        "the entry is put back as it stood, not left on the new grant"
    );
}
