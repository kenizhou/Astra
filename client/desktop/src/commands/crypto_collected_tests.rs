// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the collected-key review commands (SP4d-2 D2): the webview's
//! account, standard, fingerprint, id, email, and verified flag hand through
//! to the core, the core's row is the answer, and the list answers the staged
//! rows. Collected keys are public material: no secret crosses this surface.
//! The fake app and the state builder live in `commands_fakes`, one compiled
//! copy for every command test module.

use std::sync::Arc;

use astra_crypto_core::Standard;

use super::{list_collected_of, promote_collected_of, reject_collected_of};
use crate::commands_fakes::{state, FakeApp, FakeStore};

/// The account the review names: the same id the reading fixtures use.
fn account() -> engine_api::AccountId {
    engine_api::AccountId::try_from("work").expect("an account id")
}

/// A staged row the fake answers with.
fn staged(id: i64, fingerprint: &str) -> astra_crypto_store::CollectedKeyRow {
    astra_crypto_store::CollectedKeyRow {
        id,
        account_id: Some("work".to_owned()),
        peer_email: Some("peer@elsewhere.local".to_owned()),
        standard: Some("openpgp".to_owned()),
        fingerprint: Some(fingerprint.to_owned()),
        public_data: vec![1, 2, 3],
        source: Some("autocrypt".to_owned()),
        seen_at: "1700000000".to_owned(),
    }
}

/// A promoted public-view row the fake answers with.
fn promoted(fingerprint: &str) -> astra_crypto_store::CryptoKeyRow {
    astra_crypto_store::CryptoKeyRow {
        id: "row-1".to_owned(),
        account_id: "work".to_owned(),
        standard: "openpgp".to_owned(),
        key_type: "public".to_owned(),
        email: Some("peer@elsewhere.local".to_owned()),
        fingerprint: fingerprint.to_owned(),
        origin: "autocrypt".to_owned(),
        created_at: "1700000000".to_owned(),
        ..Default::default()
    }
}

#[test]
fn the_list_answers_the_staged_rows_and_hands_the_account_through() {
    let app = FakeApp::new();
    app.keys.lock().unwrap().collected = Some(Ok(vec![staged(1, "fpA"), staged(2, "fpB")]));
    let state = state("collected-list", Arc::clone(&app), FakeStore::accepting());

    let outcome = list_collected_of(&state, &account()).expect("the list answers");

    assert_eq!(outcome.len(), 2);
    assert_eq!(outcome[0].fingerprint.as_deref(), Some("fpA"));
    assert_eq!(
        app.keys.lock().unwrap().collected_into.as_slice(),
        ["work".to_owned()],
        "the command hands the webview's account through to the core"
    );
}

#[test]
fn the_promote_hands_the_fields_through_and_answers_the_row() {
    let app = FakeApp::new();
    app.keys.lock().unwrap().promote = Some(Ok(promoted("fpPromote")));
    let state = state(
        "collected-promote",
        Arc::clone(&app),
        FakeStore::accepting(),
    );

    let outcome = promote_collected_of(
        &state,
        &account(),
        Standard::OpenPgp,
        "fpPromote",
        Some("peer@elsewhere.local"),
        true,
    )
    .expect("the promote answers the row");

    assert_eq!(outcome.fingerprint, "fpPromote");
    assert_eq!(
        app.keys.lock().unwrap().promote_into.as_slice(),
        [(
            "work".to_owned(),
            "openpgp".to_owned(),
            "fpPromote".to_owned(),
            Some("peer@elsewhere.local".to_owned()),
            true,
        )],
        "the command hands the account, standard, fingerprint, email, and verified flag through"
    );
}

#[test]
fn the_promote_carries_an_absent_email_and_the_unverified_choice() {
    let app = FakeApp::new();
    app.keys.lock().unwrap().promote = Some(Ok(promoted("fpNoEmail")));
    let state = state(
        "collected-promote-noemail",
        Arc::clone(&app),
        FakeStore::accepting(),
    );

    promote_collected_of(
        &state,
        &account(),
        Standard::OpenPgp,
        "fpNoEmail",
        None,
        false,
    )
    .expect("the promote answers");

    assert_eq!(
        app.keys.lock().unwrap().promote_into.as_slice(),
        [(
            "work".to_owned(),
            "openpgp".to_owned(),
            "fpNoEmail".to_owned(),
            None,
            false,
        )],
        "an absent email and the unverified choice cross unchanged"
    );
}

#[test]
fn the_reject_hands_the_id_and_fields_through() {
    let app = FakeApp::new();
    app.keys.lock().unwrap().reject = Some(Ok(()));
    let state = state("collected-reject", Arc::clone(&app), FakeStore::accepting());

    reject_collected_of(
        &state,
        &account(),
        7,
        Standard::OpenPgp,
        "fpReject",
        Some("peer@elsewhere.local"),
    )
    .expect("the reject lands");

    assert_eq!(
        app.keys.lock().unwrap().reject_into.as_slice(),
        [(
            "work".to_owned(),
            7,
            "openpgp".to_owned(),
            "fpReject".to_owned(),
            Some("peer@elsewhere.local".to_owned()),
        )],
        "the command hands the account, id, standard, fingerprint, and email through"
    );
}

#[test]
fn a_refusal_answers_the_cores_words() {
    let app = FakeApp::new();
    app.keys.lock().unwrap().promote = Some(Err(
        "the key store refused the review action: no collected key".to_owned(),
    ));
    let state = state("collected-refuse", Arc::clone(&app), FakeStore::accepting());

    let outcome = promote_collected_of(
        &state,
        &account(),
        Standard::OpenPgp,
        "fpMissing",
        Some("peer@elsewhere.local"),
        false,
    );

    assert_eq!(
        outcome.expect_err("the refusal crosses"),
        "the key store refused the review action: no collected key"
    );
}
