// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the SqliteKeyStore bridge: put/get/find_by_email/remove round
//! trips, the canonical `standard|fingerprint` KeyId encoding, the OpenPGP
//! subkey-fingerprint fallback, the fetchability filter, and token handles.

use super::*;
use crate::store::CryptoStore;
use astra_crypto_core::{KeyHandle, KeyId};
use sequoia_openpgp::cert::amalgamation::key::PrimaryKey;

fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store.register_account("a2", "a2@x.com").unwrap();
    store
}

struct PgpFixture {
    primary_fp: String,
    subkey_fp: String,
    public_blob: Vec<u8>,
    secret_blob: Vec<u8>,
}

fn pgp_fixture(user_id: &str) -> PgpFixture {
    let cert = astra_crypto_openpgp::engine::generate(user_id).expect("generate cert");
    let primary_fp = astra_crypto_openpgp::keymap::fingerprint_of(&cert)
        .as_str()
        .to_string();
    let subkey_fp = cert
        .keys()
        .find(|ka| !ka.primary())
        .map(|ka| ka.key().fingerprint().to_hex())
        .expect("the generated cert carries subkeys");
    PgpFixture {
        primary_fp,
        subkey_fp,
        public_blob: astra_crypto_openpgp::keymap::cert_to_public_blob(&cert).unwrap(),
        secret_blob: astra_crypto_openpgp::keymap::cert_to_secret_blob(&cert).unwrap(),
    }
}

fn stored_key(fixture: &PgpFixture) -> StoredKey {
    StoredKey {
        handle: KeyHandleRef {
            // put replaces this with the canonical standard|fingerprint id.
            handle: KeyHandle::Software(KeyId("transient".into())),
            standard: Standard::OpenPgp,
            fingerprint: Fingerprint::new(&fixture.primary_fp),
            usage: KeyUsage::SignAndEncrypt,
            algorithm: "EdDSA".into(),
        },
        public_data: fixture.public_blob.clone(),
        private_data: Some(astra_crypto_core::SecretBox::new(Box::new(
            fixture.secret_blob.clone(),
        ))),
    }
}

#[tokio::test]
async fn put_then_get_round_trips_material_and_metadata() {
    let store = test_store();
    let ks = SqliteKeyStore::from_store(&store, "a1");
    let fixture = pgp_fixture("A One <a1@x.com>");

    let canonical = ks.put(stored_key(&fixture)).await.expect("put");
    let KeyHandle::Software(key_id) = &canonical.handle else {
        panic!("software handle expected");
    };
    assert_eq!(
        key_id.0,
        format!("openpgp|{}", fixture.primary_fp),
        "the canonical KeyId embeds standard|fingerprint"
    );
    assert_eq!(canonical.algorithm, "EdDSA");

    let got = ks
        .get(&canonical.handle)
        .await
        .expect("get")
        .expect("key present");
    assert_eq!(got.public_data, fixture.public_blob);
    let secret = got.private_data.expect("private material round-trips");
    assert_eq!(
        secrecy::ExposeSecret::expose_secret(&secret).as_slice(),
        fixture.secret_blob.as_slice()
    );
    assert_eq!(
        got.handle.usage,
        KeyUsage::SignAndEncrypt,
        "usage survives via policy_json"
    );
    assert_eq!(
        got.handle.algorithm, "EdDSA",
        "algorithm survives via policy_json"
    );

    // A miss is a clean None.
    let miss = KeyHandle::Software(KeyId("openpgp|deadbeef".into()));
    assert!(ks.get(&miss).await.unwrap().is_none());
    // A malformed KeyId (no separator) is a typed error, not a panic.
    let bad = KeyHandle::Software(KeyId("no-separator".into()));
    assert!(ks.get(&bad).await.is_err());
}

#[tokio::test]
async fn get_resolves_a_subkey_fingerprint_to_the_owning_cert() {
    let store = test_store();
    let ks = SqliteKeyStore::from_store(&store, "a1");
    let fixture = pgp_fixture("A One <a1@x.com>");
    ks.put(stored_key(&fixture)).await.unwrap();

    // A detached signature's IssuerFingerprint names the signing SUBKEY: the
    // handle by subkey fingerprint must resolve to the owning cert, returned
    // under its canonical primary-fingerprint handle.
    let subkey_handle = KeyHandle::Software(KeyId(format!("openpgp|{}", fixture.subkey_fp)));
    let got = ks
        .get(&subkey_handle)
        .await
        .expect("get")
        .expect("subkey fingerprint resolves");
    assert_eq!(
        got.handle.fingerprint.as_str(),
        fixture.primary_fp,
        "the returned handle is canonical (primary fingerprint)"
    );
    assert_eq!(got.public_data, fixture.public_blob);

    // A fingerprint no imported cert owns stays a miss (fail-closed).
    let unknown = KeyHandle::Software(KeyId(
        "openpgp|0000000000000000000000000000000000000000".into(),
    ));
    assert!(ks.get(&unknown).await.unwrap().is_none());

    // S/MIME has no subkeys: the fallback never fires for it.
    let smime = KeyHandle::Software(KeyId("smime|anything".into()));
    assert!(ks.get(&smime).await.unwrap().is_none());
}

#[tokio::test]
async fn find_by_email_binds_the_account_email_and_filters_unfetchable() {
    let store = test_store();
    let ks = SqliteKeyStore::from_store(&store, "a1");
    let fixture = pgp_fixture("A One <a1@x.com>");
    ks.put(stored_key(&fixture)).await.unwrap();

    // put binds the registered account email, so find_by_email locates it.
    let found = ks
        .find_by_email(Standard::OpenPgp, "a1@x.com")
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].fingerprint.as_str(), fixture.primary_fp);

    // The email->fingerprint discovery is global, but a row this account
    // cannot serve must be filtered out: account a2 sees a clean miss, not a
    // handle that would die downstream.
    let other = SqliteKeyStore::from_store(&store, "a2");
    assert!(
        other
            .find_by_email(Standard::OpenPgp, "a1@x.com")
            .await
            .unwrap()
            .is_empty(),
        "unfetchable handles are dropped, never surfaced"
    );
}

#[tokio::test]
async fn remove_is_account_scoped() {
    let store = test_store();
    let ks = SqliteKeyStore::from_store(&store, "a1");
    let fixture = pgp_fixture("A One <a1@x.com>");
    let canonical = ks.put(stored_key(&fixture)).await.unwrap();

    // Account a2 presenting a1's handle deletes 0 rows.
    let other = SqliteKeyStore::from_store(&store, "a2");
    other.remove(&canonical.handle).await.unwrap();
    assert!(ks.get(&canonical.handle).await.unwrap().is_some());

    ks.remove(&canonical.handle).await.unwrap();
    assert!(ks.get(&canonical.handle).await.unwrap().is_none());
    // Idempotent.
    ks.remove(&canonical.handle).await.unwrap();
}

#[tokio::test]
async fn token_handles_resolve_public_material_only() {
    let store = test_store();
    // Register a token-backed row directly (the way token registration will).
    crate::upsert_crypto_key(
        &store,
        &crate::CryptoKeyRecord {
            row: crate::CryptoKeyRow {
                account_id: "a1".to_string(),
                standard: "smime".to_string(),
                key_type: "cert".to_string(),
                email: Some("a1@x.com".to_string()),
                fingerprint: "fpTok".to_string(),
                origin: "imported".to_string(),
                token_serial: Some("SER123".to_string()),
                token_key_id: Some("key-1".to_string()),
                ..Default::default()
            },
            public_data: hex::encode(b"token-public-der"),
            private_data: None,
            policy_json: Some(r#"{"algorithm":"ECDSA-P256","usage":"signandencrypt"}"#.to_string()),
        },
    )
    .unwrap();

    let ks = SqliteKeyStore::from_store(&store, "a1");
    let handle = KeyHandle::Token {
        token_serial: "SER123".to_string(),
        key_id: astra_crypto_core::TokenKeyId("key-1".to_string()),
    };
    let got = ks.get(&handle).await.unwrap().expect("token row resolves");
    assert_eq!(got.public_data, b"token-public-der");
    assert!(
        got.private_data.is_none(),
        "the private half stays on the device"
    );
    assert_eq!(
        got.handle.fingerprint.as_str(),
        "fptok",
        "Fingerprint::new normalises to lowercase"
    );

    // find_by_email surfaces the Token handle shape for token rows.
    let found = ks.find_by_email(Standard::Smime, "a1@x.com").await.unwrap();
    assert_eq!(found.len(), 1);
    assert!(
        matches!(found[0].handle, KeyHandle::Token { .. }),
        "token rows surface Token handles"
    );

    // remove on a token handle is a no-op (token rows are managed by the
    // token registry, not the software path).
    ks.remove(&handle).await.unwrap();
    assert!(ks.get(&handle).await.unwrap().is_some());
}
