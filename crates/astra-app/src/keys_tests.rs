// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the key import surface (SP4d-1 D2): the import dispatches public
//! material to the public-cert store path and secret material to the backend
//! private path (passphrase-gated), the list answers the account's rows, and
//! the passphrase never leaks into an answer or a refusal.

use std::sync::{Arc, Mutex};

use astra_crypto_core::SecretBox;
use astra_crypto_openpgp::keymap::{cert_to_armored_public, cert_to_secret_blob};
use astra_crypto_store::CryptoStore;
use engine_api::AccountId;
use sequoia_openpgp::cert::prelude::*;

use crate::{dispatch_fakes as fakes, App, Surface};

const ACCOUNT: &str = "work";

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

/// A fresh cert with secret material: the shape a person's own key file holds.
fn gen(user_id: &str) -> sequoia_openpgp::Cert {
    let (cert, _rev) = CertBuilder::new()
        .add_userid(user_id)
        .add_transport_encryption_subkey()
        .add_signing_subkey()
        .generate()
        .expect("CertBuilder::generate");
    cert
}

/// A TSK whose every secret is encrypted under `password`.
fn gen_encrypted(user_id: &str, password: &str) -> sequoia_openpgp::Cert {
    let (cert, _rev) = CertBuilder::new()
        .add_userid(user_id)
        .add_transport_encryption_subkey()
        .add_signing_subkey()
        .set_password(Some(password.into()))
        .generate()
        .expect("CertBuilder::generate with password");
    cert
}

fn passphrase(s: &str) -> SecretBox<String> {
    SecretBox::new(Box::new(s.to_owned()))
}

#[tokio::test]
async fn a_public_cert_imports_through_the_public_path() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let cert = gen("Peer <peer@elsewhere.local>");
    let bytes = cert_to_armored_public(&cert).expect("armored public");

    let row = app
        .import_key(&account(), &bytes, None)
        .await
        .expect("the public import lands");

    assert_eq!(row.standard, "openpgp");
    assert_eq!(row.key_type, "public");
    assert_eq!(row.origin, "imported");
    assert!(
        !row.has_private,
        "a public import never flags a private key"
    );
    assert_eq!(row.email.as_deref(), Some("peer@elsewhere.local"));

    let listed = app.list_keys(&account()).expect("the list answers");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].fingerprint, row.fingerprint);
}

#[tokio::test]
async fn a_private_key_imports_through_the_private_path() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let cert = gen("Own <me@work.local>");
    let bytes = cert_to_secret_blob(&cert).expect("secret blob");

    let row = app
        .import_key(&account(), &bytes, None)
        .await
        .expect("the private import lands");

    assert!(row.has_private, "the private path records the secret blob");
    assert_eq!(row.standard, "openpgp");
    let full =
        astra_crypto_store::get_crypto_key_full(&store, ACCOUNT, "openpgp", &row.fingerprint)
            .unwrap()
            .expect("the record stands");
    assert!(full.private_data.is_some(), "the private blob is stored");

    // The first private key of a standard becomes the default signer, so the
    // account can sign straight after the import.
    let default = astra_crypto_store::get_default_signing_key(
        &store,
        ACCOUNT,
        astra_crypto_core::Standard::OpenPgp,
    )
    .unwrap()
    .expect("a default signing key stands");
    assert_eq!(default.fingerprint, row.fingerprint);
}

#[tokio::test]
async fn an_encrypted_private_key_imports_with_its_passphrase() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let cert = gen_encrypted("Own <me@work.local>", "correct horse");
    let bytes = cert_to_secret_blob(&cert).expect("encrypted secret blob");

    let row = app
        .import_key(&account(), &bytes, Some(passphrase("correct horse")))
        .await
        .expect("the passphrase unlocks the import");
    assert!(row.has_private);

    // A wrong passphrase refuses, and the refusal never names the probe.
    let probe = "wrong-passphrase-probe-9d21";
    let err = app
        .import_key(&account(), &bytes, Some(passphrase(probe)))
        .await
        .expect_err("a wrong passphrase refuses");
    assert!(
        !err.to_string().contains(probe),
        "the refusal never echoes the passphrase: {err}"
    );

    // No passphrase at all refuses an encrypted key.
    assert!(app.import_key(&account(), &bytes, None).await.is_err());
}

#[tokio::test]
async fn a_second_private_import_keeps_the_standing_default() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let first = gen("First <me@work.local>");
    let second = gen("Second <me@work.local>");

    let first_row = app
        .import_key(&account(), &cert_to_secret_blob(&first).unwrap(), None)
        .await
        .expect("the first import lands");
    let _second_row = app
        .import_key(&account(), &cert_to_secret_blob(&second).unwrap(), None)
        .await
        .expect("the second import lands");

    let default = astra_crypto_store::get_default_signing_key(
        &store,
        ACCOUNT,
        astra_crypto_core::Standard::OpenPgp,
    )
    .unwrap()
    .expect("the default stands");
    assert_eq!(
        default.fingerprint, first_row.fingerprint,
        "a later import never steals the standing default"
    );
}

#[tokio::test]
async fn list_keys_answers_the_accounts_rows_across_standards() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let cert = gen("Peer <peer@elsewhere.local>");
    app.import_key(&account(), &cert_to_armored_public(&cert).unwrap(), None)
        .await
        .expect("the openpgp import lands");

    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Keys Test CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("peer@elsewhere.local", &ca);
    app.import_key(&account(), &leaf.cert_der, None)
        .await
        .expect("the smime import lands");

    let listed = app.list_keys(&account()).expect("the list answers");
    assert_eq!(listed.len(), 2, "both standards answer");
    let standards: Vec<&str> = listed.iter().map(|row| row.standard.as_str()).collect();
    assert!(standards.contains(&"openpgp"));
    assert!(standards.contains(&"smime"));

    // Account scoping: another account sees nothing of this one's keys.
    let other = AccountId::try_from("other").expect("an account id");
    assert_eq!(app.list_keys(&other).unwrap(), []);
}

#[tokio::test]
async fn an_import_without_a_store_is_refused() {
    let app = app(None);
    let cert = gen("Peer <peer@elsewhere.local>");
    let err = app
        .import_key(&account(), &cert_to_armored_public(&cert).unwrap(), None)
        .await
        .expect_err("no store, no import");
    assert!(matches!(err, crate::crypto::keys::KeyImportError::NoStore));
    assert!(app.list_keys(&account()).is_err());
}

#[tokio::test]
async fn malformed_input_is_refused_and_stages_nothing() {
    let store = test_store();
    let app = app(Some(store.clone()));
    assert!(app
        .import_key(&account(), b"not a key at all", None)
        .await
        .is_err());
    assert_eq!(app.list_keys(&account()).unwrap(), []);
}

#[tokio::test]
async fn the_passphrase_never_lands_in_a_row_or_a_refusal() {
    let store = test_store();
    let app = app(Some(store.clone()));
    let probe = "passphrase-leak-probe-4b7e";
    let cert = gen_encrypted("Own <me@work.local>", probe);
    let bytes = cert_to_secret_blob(&cert).expect("encrypted secret blob");

    let row = app
        .import_key(&account(), &bytes, Some(passphrase(probe)))
        .await
        .expect("the import lands");
    let rendered = format!("{row:?}");
    assert!(
        !rendered.contains(probe),
        "the row never carries the passphrase"
    );

    // Nothing in the store's public rows names the passphrase either.
    for listed in app.list_keys(&account()).unwrap() {
        assert!(!format!("{listed:?}").contains(probe));
    }
}
