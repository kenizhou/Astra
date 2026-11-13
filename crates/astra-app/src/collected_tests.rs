// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the collected-key review surface (SP4d-2 D1): the list answers the
//! staged rows newest first, the promote lands the key AND records the trust
//! decision so it is send-usable (the load-bearing pairing), a verified promote
//! upgrades while an unverified promote never downgrades a standing decision,
//! the reject removes the staging row and records rejected, and a no-email
//! promote still lands the key.

use std::sync::{Arc, Mutex};

use astra_crypto_core::{RecipientTrustState, Standard};
use astra_crypto_openpgp::keymap::{cert_to_public_blob, fingerprint_of};
use astra_crypto_store::CryptoStore;
use engine_api::AccountId;
use sequoia_openpgp::cert::prelude::*;

use crate::{dispatch_fakes as fakes, App, Surface};

const ACCOUNT: &str = "work";
const PEER: &str = "peer@elsewhere.local";

fn test_store() -> Arc<CryptoStore> {
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account(ACCOUNT, "me@work.local").unwrap();
    store
        .register_account("other", "other@elsewhere.local")
        .unwrap();
    Arc::new(store)
}

fn app(store: Option<Arc<CryptoStore>>) -> App<fakes::FakeProvider> {
    let surfaces = Arc::new(Mutex::new(Vec::<Surface>::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let app = fakes::app(vec![fakes::account(ACCOUNT, provider)], &surfaces);
    if let Some(store) = store {
        app.install_crypto_store(store);
    }
    app
}

fn account() -> AccountId {
    AccountId::try_from(ACCOUNT).expect("an account id")
}

/// A fresh peer cert: a user id, a transport-encryption subkey, and a signing
/// subkey, the shape an Autocrypt header carries.
fn gen(user_id: &str) -> sequoia_openpgp::Cert {
    let (cert, _rev) = CertBuilder::new()
        .add_userid(user_id)
        .add_transport_encryption_subkey()
        .add_signing_subkey()
        .generate()
        .expect("CertBuilder::generate");
    cert
}

/// Stage a peer cert the way the Autocrypt consume does: source `autocrypt`,
/// the binary public blob, the peer address and fingerprint named.
fn stage(store: &CryptoStore, peer: &str, cert: &sequoia_openpgp::Cert) -> i64 {
    let blob = cert_to_public_blob(cert).expect("the public blob");
    let fp = fingerprint_of(cert).as_str().to_owned();
    astra_crypto_store::stage_collected_key(
        store,
        Some(ACCOUNT),
        Some(peer),
        Some("openpgp"),
        Some(&fp),
        &blob,
        Some("autocrypt"),
    )
    .expect("the staging row lands")
}

fn latest_decision(store: &CryptoStore, fp: &str) -> Option<String> {
    astra_crypto_store::get_latest_trust_decision(store, ACCOUNT, PEER, "openpgp", fp)
        .expect("the trust read runs")
        .map(|row| row.decision)
}

#[tokio::test]
async fn list_collected_answers_the_staged_rows_newest_first() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let first = gen(&format!("First <{PEER}>"));
    let second = gen(&format!("Second <{PEER}>"));
    let first_id = stage(&store, PEER, &first);
    let second_id = stage(&store, PEER, &second);

    let rows = app.list_collected(&account()).expect("the list answers");
    assert_eq!(rows.len(), 2, "both staged rows answer");
    // Same-second inserts tie-break on id DESC: the newest staged row leads.
    assert_eq!(rows[0].id, second_id);
    assert_eq!(rows[1].id, first_id);
    assert!(rows[0].id > rows[1].id, "newest first");
    assert_eq!(rows[0].source.as_deref(), Some("autocrypt"));
    assert_eq!(rows[0].peer_email.as_deref(), Some(PEER));

    // Account scoping: another account sees nothing of this one's staging.
    let other = AccountId::try_from("other").expect("an account id");
    assert_eq!(app.list_collected(&other).unwrap(), []);
}

#[tokio::test]
async fn promote_lands_the_key_and_the_trust_decision_so_it_is_send_usable() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let cert = gen(&format!("Peer <{PEER}>"));
    let fp = fingerprint_of(&cert).as_str().to_owned();
    stage(&store, PEER, &cert);

    let row = app
        .promote_collected(&account(), Standard::OpenPgp, &fp, Some(PEER), false)
        .expect("the promote lands");
    assert_eq!(row.fingerprint, fp);
    assert_eq!(row.key_type, "public");
    assert_eq!(
        row.origin, "autocrypt",
        "the provenance survives the promote"
    );
    assert!(
        !row.has_private,
        "a collected promote never flags a private key"
    );

    // The load-bearing pairing: the promote records the acceptance, so the
    // resolver answers Ready (send-usable) rather than Undecided.
    assert_eq!(latest_decision(&store, &fp).as_deref(), Some("unverified"));
    let status =
        astra_crypto_store::resolve_recipient_status(&store, ACCOUNT, Standard::OpenPgp, PEER);
    assert_eq!(
        status.state,
        RecipientTrustState::Ready,
        "the promoted key is send-usable by the promote alone"
    );

    // The staging drains: the review list no longer offers the key.
    assert_eq!(app.list_collected(&account()).unwrap(), []);
}

#[tokio::test]
async fn a_verified_promote_upgrades_a_standing_unverified_decision() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let cert = gen(&format!("Peer <{PEER}>"));
    let fp = fingerprint_of(&cert).as_str().to_owned();
    stage(&store, PEER, &cert);
    app.promote_collected(&account(), Standard::OpenPgp, &fp, Some(PEER), false)
        .expect("the unverified promote lands");
    assert_eq!(latest_decision(&store, &fp).as_deref(), Some("unverified"));

    // A re-staged copy, then the out-of-band verification: the verified choice
    // upgrades the standing decision (the keystore row wins untouched).
    stage(&store, PEER, &cert);
    app.promote_collected(&account(), Standard::OpenPgp, &fp, Some(PEER), true)
        .expect("the verified promote lands");
    assert_eq!(latest_decision(&store, &fp).as_deref(), Some("verified"));
}

#[tokio::test]
async fn an_unverified_promote_never_downgrades_a_standing_verified_decision() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let cert = gen(&format!("Peer <{PEER}>"));
    let fp = fingerprint_of(&cert).as_str().to_owned();
    stage(&store, PEER, &cert);
    app.promote_collected(&account(), Standard::OpenPgp, &fp, Some(PEER), true)
        .expect("the verified promote lands");
    assert_eq!(latest_decision(&store, &fp).as_deref(), Some("verified"));

    // A re-staged copy accepted without verification must not reset the
    // standing verified decision.
    stage(&store, PEER, &cert);
    app.promote_collected(&account(), Standard::OpenPgp, &fp, Some(PEER), false)
        .expect("the unverified promote lands");
    assert_eq!(
        latest_decision(&store, &fp).as_deref(),
        Some("verified"),
        "an unverified promote never downgrades"
    );
}

#[tokio::test]
async fn reject_removes_the_staging_row_and_records_rejected() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let cert = gen(&format!("Peer <{PEER}>"));
    let fp = fingerprint_of(&cert).as_str().to_owned();
    let id = stage(&store, PEER, &cert);

    app.reject_collected(&account(), id, Standard::OpenPgp, &fp, Some(PEER))
        .expect("the reject lands");

    // The staging row is gone and the rejected decision stands.
    assert_eq!(app.list_collected(&account()).unwrap(), []);
    assert_eq!(latest_decision(&store, &fp).as_deref(), Some("rejected"));

    // The decision is load-bearing: a re-staged copy promoted without
    // verification keeps the standing rejection (the unverified promote never
    // downgrades it), so the resolver refuses the key rather than admitting it.
    stage(&store, PEER, &cert);
    app.promote_collected(&account(), Standard::OpenPgp, &fp, Some(PEER), false)
        .expect("the re-staged copy promotes");
    assert_eq!(latest_decision(&store, &fp).as_deref(), Some("rejected"));
    let status =
        astra_crypto_store::resolve_recipient_status(&store, ACCOUNT, Standard::OpenPgp, PEER);
    assert_eq!(status.state, RecipientTrustState::Rejected);
}

#[tokio::test]
async fn a_no_email_promote_still_lands_the_key() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let cert = gen("Peer <peer@elsewhere.local>");
    let fp = fingerprint_of(&cert).as_str().to_owned();
    // A staged row carrying no peer email: nothing to decide against.
    let blob = cert_to_public_blob(&cert).expect("the public blob");
    astra_crypto_store::stage_collected_key(
        &store,
        Some(ACCOUNT),
        None,
        Some("openpgp"),
        Some(&fp),
        &blob,
        Some("autocrypt"),
    )
    .expect("the staging row lands");

    let row = app
        .promote_collected(&account(), Standard::OpenPgp, &fp, None, false)
        .expect("the no-email promote still lands the key");
    assert_eq!(row.fingerprint, fp);
    assert_eq!(row.email, None, "no address binds the row");
    assert_eq!(app.list_collected(&account()).unwrap(), []);
}

#[tokio::test]
async fn promote_binds_the_decision_to_the_given_email() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let cert = gen("Peer <peer@elsewhere.local>");
    let fp = fingerprint_of(&cert).as_str().to_owned();
    // The staged row names one address; the accept flow overrides it with the
    // recipient address being accepted.
    stage(&store, "staged@elsewhere.local", &cert);
    app.promote_collected(&account(), Standard::OpenPgp, &fp, Some(PEER), false)
        .expect("the promote lands");

    let decision =
        astra_crypto_store::get_latest_trust_decision(&store, ACCOUNT, PEER, "openpgp", &fp)
            .expect("the trust read runs")
            .expect("the decision binds the given email");
    assert_eq!(decision.decision, "unverified");
    let status =
        astra_crypto_store::resolve_recipient_status(&store, ACCOUNT, Standard::OpenPgp, PEER);
    assert_eq!(status.state, RecipientTrustState::Ready);
}

#[tokio::test]
async fn a_review_without_a_store_is_refused() {
    let app = app(None);
    assert!(app.list_collected(&account()).is_err());
    assert!(app
        .promote_collected(&account(), Standard::OpenPgp, "fp", Some(PEER), false)
        .is_err());
    assert!(app
        .reject_collected(&account(), 1, Standard::OpenPgp, "fp", Some(PEER))
        .is_err());
}
