// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the public-cert import path: a public cert round-trips with no
//! private material, malformed input stages nothing and never panics, the
//! fingerprint and user-id are extracted, a cross-standard import is
//! refused, and the shared trust-write helper records a decision.

use super::*;
use crate::default::{get_default_signing_key, set_default_signing_key};
use crate::keys::{
    get_crypto_key_full, get_crypto_key_public, get_crypto_key_public_with_blob,
    list_crypto_keys_for_account,
};
use crate::store::CryptoStore;
use crate::trust::{get_latest_trust_decision, list_decisions_for_email, put_trust_decision};
use astra_crypto_openpgp::keymap::{
    cert_to_armored_public, cert_to_public_blob, cert_to_secret_blob, fingerprint_of, parse_certs,
};
use sequoia_openpgp::cert::prelude::*;

fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store.register_account("a2", "a2@x.com").unwrap();
    store
}

/// A fresh cert with secret material (the CertBuilder shape the keymap tests
/// use): a user id, a transport-encryption subkey, and a signing subkey.
fn gen(user_id: &str) -> sequoia_openpgp::Cert {
    let (cert, _rev) = CertBuilder::new()
        .add_userid(user_id)
        .add_transport_encryption_subkey()
        .add_signing_subkey()
        .generate()
        .expect("CertBuilder::generate");
    cert
}

/// The armored public form of `cert`: what a peer hands over.
fn armored_public(cert: &sequoia_openpgp::Cert) -> Vec<u8> {
    cert_to_armored_public(cert).expect("armored public serializes")
}

#[test]
fn public_cert_round_trips_with_has_private_false() {
    let store = test_store();
    let cert = gen("Peer <peer@x.com>");
    let row = import_public_cert(&store, "a1", &armored_public(&cert)).expect("import ok");

    assert_eq!(row.standard, "openpgp");
    assert_eq!(row.key_type, "public");
    assert_eq!(row.origin, "imported");
    assert!(
        !row.has_private,
        "a public import never flags a private key"
    );
    assert_eq!(row.fingerprint, fingerprint_of(&cert).as_str());
    assert_eq!(row.account_id, "a1");
    assert!(!row.id.is_empty(), "the upsert generated a uuid");

    // The stored row agrees, and the full record carries no private blob.
    let stored = get_crypto_key_public(&store, "a1", "openpgp", &row.fingerprint)
        .unwrap()
        .expect("the row stands");
    assert_eq!(stored, row);
    let full = get_crypto_key_full(&store, "a1", "openpgp", &row.fingerprint)
        .unwrap()
        .expect("the record stands");
    assert_eq!(full.private_data, None, "no private blob is stored");

    // The raw column is NULL: nothing was wrapped at rest.
    let enc: Option<String> = store
        .conn()
        .query_row(
            "SELECT private_data_enc FROM crypto_keys WHERE account_id = 'a1' AND fingerprint = ?1",
            rusqlite::params![row.fingerprint],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(enc, None, "the private column stays NULL");
}

#[test]
fn the_public_blob_is_public_only_material() {
    let store = test_store();
    let cert = gen("Peer <peer@x.com>");
    let row = import_public_cert(&store, "a1", &armored_public(&cert)).expect("import ok");

    let (_row, blob) = get_crypto_key_public_with_blob(&store, "a1", "openpgp", &row.fingerprint)
        .unwrap()
        .expect("the blob stands");
    let parsed = parse_certs(&hex::decode(&blob).expect("the blob is hex")).expect("parses");
    assert_eq!(parsed.len(), 1);
    assert!(
        !parsed[0].is_tsk(),
        "the stored blob carries no secret material"
    );
    assert_eq!(parsed[0].fingerprint(), cert.fingerprint());
}

#[test]
fn binary_public_input_round_trips() {
    let store = test_store();
    let cert = gen("Peer <peer@x.com>");
    let binary = cert_to_public_blob(&cert).expect("binary public blob");
    let row = import_public_cert(&store, "a1", &binary).expect("import ok");
    assert_eq!(row.fingerprint, fingerprint_of(&cert).as_str());
    assert!(!row.has_private);
}

#[test]
fn the_fingerprint_and_user_id_are_extracted() {
    let store = test_store();
    let cert = gen("Peer Name <peer@x.com>");
    let row = import_public_cert(&store, "a1", &armored_public(&cert)).expect("import ok");
    assert_eq!(
        row.fingerprint,
        cert.fingerprint().to_hex().to_ascii_lowercase(),
        "the fingerprint is the lowercase hex of the primary key"
    );
    assert_eq!(
        row.email.as_deref(),
        Some("peer@x.com"),
        "the email comes from the cert's user id"
    );
}

#[test]
fn malformed_input_stages_nothing_and_never_panics() {
    let store = test_store();
    for bad in [
        &b""[..],
        b"not a key at all",
        b"-----BEGIN PGP PUBLIC KEY BLOCK-----\ngarbage\n-----END PGP PUBLIC KEY BLOCK-----",
        &[0x00, 0x01, 0x02, 0xff],
    ] {
        assert!(
            import_public_cert(&store, "a1", bad).is_err(),
            "malformed input is refused: {bad:?}"
        );
    }
    assert_eq!(
        list_crypto_keys_for_account(&store, "a1", "openpgp").unwrap(),
        []
    );
    assert_eq!(
        list_crypto_keys_for_account(&store, "a1", "smime").unwrap(),
        []
    );
}

#[test]
fn secret_bearing_input_is_refused_on_the_public_path() {
    let store = test_store();
    let cert = gen("Own <own@x.com>");
    let tsk = cert_to_secret_blob(&cert).expect("secret blob");
    assert!(
        import_public_cert(&store, "a1", &tsk).is_err(),
        "a private key belongs to the private import path, not the public one"
    );
    assert_eq!(
        list_crypto_keys_for_account(&store, "a1", "openpgp").unwrap(),
        []
    );
}

#[test]
fn a_cross_standard_import_is_rejected() {
    let store = test_store();
    let cert = gen("Peer <peer@x.com>");
    let pgp = armored_public(&cert);
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Import Test CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("leaf@x.com", &ca);

    // OpenPGP material declared as S/MIME: refused, nothing staged.
    assert!(import_public_cert_as(&store, "a1", "smime", &pgp).is_err());
    // S/MIME material declared as OpenPGP: refused, nothing staged.
    assert!(import_public_cert_as(&store, "a1", "openpgp", &leaf.cert_der).is_err());
    // An unknown standard is refused outright.
    assert!(import_public_cert_as(&store, "a1", "sm", &pgp).is_err());

    assert_eq!(
        list_crypto_keys_for_account(&store, "a1", "openpgp").unwrap(),
        []
    );
    assert_eq!(
        list_crypto_keys_for_account(&store, "a1", "smime").unwrap(),
        []
    );

    // The matching standard imports cleanly.
    let row = import_public_cert_as(&store, "a1", "openpgp", &pgp).expect("matching standard");
    assert_eq!(row.standard, "openpgp");
}

#[test]
fn smime_public_cert_round_trips_with_display_metadata() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Import Test CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("leaf@x.com", &ca);
    let fp = astra_crypto_smime::fingerprint_of_cert_der(&leaf.cert_der).unwrap();

    let row = import_public_cert(&store, "a1", &leaf.cert_der).expect("import ok");
    assert_eq!(row.standard, "smime");
    assert_eq!(row.key_type, "cert");
    assert_eq!(row.origin, "imported");
    assert!(!row.has_private);
    assert_eq!(row.fingerprint, fp);
    assert_eq!(row.email.as_deref(), Some("leaf@x.com"));
    assert!(row.subject_cn.is_some(), "the subject CN is extracted");
    assert!(row.not_before.is_some(), "the validity start is extracted");
    assert!(row.expires_at.is_some(), "the validity end is extracted");

    let full = get_crypto_key_full(&store, "a1", "smime", &fp)
        .unwrap()
        .expect("the record stands");
    assert_eq!(full.private_data, None);
}

#[test]
fn smime_pem_public_cert_round_trips() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Import Test CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("leaf@x.com", &ca);
    let pem = format!(
        "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
        base64::engine::general_purpose::STANDARD.encode(&leaf.cert_der)
    );
    let row = import_public_cert(&store, "a1", pem.as_bytes()).expect("import ok");
    assert_eq!(row.standard, "smime");
    assert_eq!(
        row.fingerprint,
        astra_crypto_smime::fingerprint_of_cert_der(&leaf.cert_der).unwrap()
    );
    assert!(!row.has_private);
}

#[test]
fn a_reimport_updates_the_same_row() {
    let store = test_store();
    let cert = gen("Peer <peer@x.com>");
    let first = import_public_cert(&store, "a1", &armored_public(&cert)).expect("first import");
    let second = import_public_cert(&store, "a1", &armored_public(&cert)).expect("re-import");
    assert_eq!(first.id, second.id, "the upsert keeps the row identity");
    assert_eq!(
        list_crypto_keys_for_account(&store, "a1", "openpgp")
            .unwrap()
            .len(),
        1,
        "one row per fingerprint"
    );
}

#[test]
fn the_trust_write_helper_records_a_decision() {
    let store = test_store();
    record_import_trust(&store, "a1", "peer@x.com", "openpgp", "fpTrust", true).unwrap();
    let latest = get_latest_trust_decision(&store, "a1", "peer@x.com", "openpgp", "fpTrust")
        .unwrap()
        .expect("the decision stands");
    assert_eq!(latest.decision, "verified");

    record_import_trust(&store, "a1", "peer@x.com", "openpgp", "fpTrust", false).unwrap();
    let latest = get_latest_trust_decision(&store, "a1", "peer@x.com", "openpgp", "fpTrust")
        .unwrap()
        .expect("the second decision stands");
    assert_eq!(latest.decision, "unverified");

    // The audit history keeps both rows.
    let history = list_decisions_for_email(&store, "a1", "openpgp", "peer@x.com").unwrap();
    assert_eq!(history.len(), 2);
}

#[test]
fn a_public_import_records_an_unverified_decision() {
    let store = test_store();
    let cert = gen("Peer <peer@x.com>");
    let row = import_public_cert(&store, "a1", &armored_public(&cert)).expect("import ok");

    // The import itself lands the acceptance: an imported peer key is
    // unverified by default, which the send resolver treats as
    // encryption-qualified, so the import is send-usable straight away.
    let latest = get_latest_trust_decision(&store, "a1", "peer@x.com", "openpgp", &row.fingerprint)
        .unwrap()
        .expect("the import recorded a decision");
    assert_eq!(latest.decision, "unverified");
}

#[test]
fn a_reimport_never_downgrades_a_standing_decision() {
    let store = test_store();
    let cert = gen("Peer <peer@x.com>");
    let row = import_public_cert(&store, "a1", &armored_public(&cert)).expect("first import");

    // The person verifies the fingerprint out of band.
    record_import_trust(
        &store,
        "a1",
        "peer@x.com",
        "openpgp",
        &row.fingerprint,
        true,
    )
    .unwrap();

    // A re-import of the same cert (an updated copy) must not reset the
    // standing verified decision back to unverified.
    import_public_cert(&store, "a1", &armored_public(&cert)).expect("re-import");
    let latest = get_latest_trust_decision(&store, "a1", "peer@x.com", "openpgp", &row.fingerprint)
        .unwrap()
        .expect("the decision stands");
    assert_eq!(latest.decision, "verified", "a re-import never downgrades");
}

#[test]
fn a_public_reimport_preserves_the_standing_default_signer() {
    let store = test_store();
    let cert = gen("Own <own@x.com>");
    let fp = fingerprint_of(&cert).as_str().to_owned();

    // A privately-held key stands as the default signer (the private import
    // path arms it). The private blob is dummy material: the test never
    // decrypts it, only asserts it survives.
    let record = CryptoKeyRecord {
        row: CryptoKeyRow {
            account_id: "a1".to_owned(),
            standard: "openpgp".to_owned(),
            key_type: "cert".to_owned(),
            email: Some("own@x.com".to_owned()),
            fingerprint: fp.clone(),
            origin: "generated".to_owned(),
            ..Default::default()
        },
        public_data: hex::encode(cert_to_public_blob(&cert).unwrap()),
        private_data: Some("dummy-secret-material".to_owned()),
        policy_json: None,
    };
    upsert_crypto_key(&store, &record).unwrap();
    set_default_signing_key(&store, "a1", "openpgp", &fp).unwrap();

    // Importing the same key's public cert (key files ship in pairs) must not
    // wipe the default flag, demote the key type, or drop the private blob.
    let row = import_public_cert(&store, "a1", &armored_public(&cert)).expect("public re-import");
    assert!(row.is_default_sign, "the standing default survives");
    assert!(row.has_private, "the private blob survives");
    assert_eq!(
        row.key_type, "cert",
        "a private key is not demoted to public"
    );
    let standing = get_default_signing_key(&store, "a1", astra_crypto_core::Standard::OpenPgp)
        .unwrap()
        .expect("the default signer still stands");
    assert_eq!(standing.fingerprint, fp);
}

#[test]
fn a_smime_public_reimport_preserves_the_standing_default_signer() {
    let store = test_store();
    let ca = astra_crypto_smime::testing::build_self_signed_ca("CN=Import Test CA");
    let leaf = astra_crypto_smime::testing::build_leaf_signed_by("leaf@x.com", &ca);
    let fp = astra_crypto_smime::fingerprint_of_cert_der(&leaf.cert_der).unwrap();

    let record = CryptoKeyRecord {
        row: CryptoKeyRow {
            account_id: "a1".to_owned(),
            standard: "smime".to_owned(),
            key_type: "cert".to_owned(),
            email: Some("leaf@x.com".to_owned()),
            fingerprint: fp.clone(),
            origin: "generated".to_owned(),
            ..Default::default()
        },
        public_data: hex::encode(&leaf.cert_der),
        private_data: Some("dummy-secret-material".to_owned()),
        policy_json: None,
    };
    upsert_crypto_key(&store, &record).unwrap();
    set_default_signing_key(&store, "a1", "smime", &fp).unwrap();

    let row = import_public_cert(&store, "a1", &leaf.cert_der).expect("public re-import");
    assert!(row.is_default_sign, "the standing default survives");
    assert!(row.has_private, "the private blob survives");
    assert_eq!(row.key_type, "cert", "a private key is not demoted");
}

#[test]
fn the_promote_helper_records_an_unverified_acceptance_when_none_stands() {
    let store = test_store();
    record_promote_trust(
        &store,
        "a1",
        Some("peer@x.com"),
        "openpgp",
        "fpPromote",
        false,
    )
    .unwrap();
    let latest = get_latest_trust_decision(&store, "a1", "peer@x.com", "openpgp", "fpPromote")
        .unwrap()
        .expect("the acceptance stands");
    assert_eq!(latest.decision, "unverified");
}

#[test]
fn a_verified_promote_upgrades_a_standing_unverified_decision() {
    let store = test_store();
    record_promote_trust(
        &store,
        "a1",
        Some("peer@x.com"),
        "openpgp",
        "fpPromote",
        false,
    )
    .unwrap();
    record_promote_trust(
        &store,
        "a1",
        Some("peer@x.com"),
        "openpgp",
        "fpPromote",
        true,
    )
    .unwrap();
    let latest = get_latest_trust_decision(&store, "a1", "peer@x.com", "openpgp", "fpPromote")
        .unwrap()
        .expect("the decision stands");
    assert_eq!(
        latest.decision, "verified",
        "a verified choice always upgrades"
    );
}

#[test]
fn an_unverified_promote_never_downgrades_a_standing_verified_decision() {
    let store = test_store();
    record_import_trust(&store, "a1", "peer@x.com", "openpgp", "fpPromote", true).unwrap();
    record_promote_trust(
        &store,
        "a1",
        Some("peer@x.com"),
        "openpgp",
        "fpPromote",
        false,
    )
    .unwrap();
    let latest = get_latest_trust_decision(&store, "a1", "peer@x.com", "openpgp", "fpPromote")
        .unwrap()
        .expect("the decision stands");
    assert_eq!(
        latest.decision, "verified",
        "an unverified promote never downgrades a standing decision"
    );
}

#[test]
fn an_unverified_promote_never_downgrades_a_standing_personal_decision() {
    let store = test_store();
    put_trust_decision(
        &store,
        "a1",
        "peer@x.com",
        "openpgp",
        "fpPromote",
        "personal",
        None,
    )
    .unwrap();
    record_promote_trust(
        &store,
        "a1",
        Some("peer@x.com"),
        "openpgp",
        "fpPromote",
        false,
    )
    .unwrap();
    let latest = get_latest_trust_decision(&store, "a1", "peer@x.com", "openpgp", "fpPromote")
        .unwrap()
        .expect("the decision stands");
    assert_eq!(latest.decision, "personal");
}

#[test]
fn a_no_email_promote_records_no_decision() {
    let store = test_store();
    record_promote_trust(&store, "a1", None, "openpgp", "fpPromote", false).unwrap();
    record_promote_trust(&store, "a1", None, "openpgp", "fpPromote", true).unwrap();
    // No address to decide against: the audit holds nothing for this key.
    let history = list_decisions_for_email(&store, "a1", "openpgp", "").unwrap();
    assert!(
        history.is_empty(),
        "a no-email promote decides against nothing"
    );
}
