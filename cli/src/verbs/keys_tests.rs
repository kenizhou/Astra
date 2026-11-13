// SPDX-License-Identifier: GPL-3.0-only
//! The key verbs' read and import tests: the shaped public reads over
//! crypto.sqlite and the public-only import with its default unverified
//! trust, the one-source discipline, and the secret-material refusal
//! pointing at the desktop. The promote/reject review actions, the
//! previews, and the audit rows live in `keys_review_tests`; the fixture
//! helpers here are shared with that file.

use astra_crypto_openpgp::keymap::{
    cert_to_armored_public, cert_to_public_blob, cert_to_secret_blob, fingerprint_of,
};
use astra_crypto_store::CryptoStore;
use sequoia_openpgp::cert::prelude::*;
use serde_json::Value;

use super::fakes::{temp_dir, write_index};
use super::run;

/// The peer address the seeded certs bind.
pub(super) const PEER: &str = "peer@elsewhere.local";

/// One command line as the `run` layer takes it.
pub(super) fn line(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// A data dir whose index names `alpha` and whose crypto store holds the
/// registered account row the keystore's foreign keys need, under the fixed
/// test master key (the suite never touches the OS keyring).
pub(super) fn seeded() -> std::path::PathBuf {
    let dir = temp_dir();
    write_index(&dir, &["alpha"]);
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open(&dir.join("crypto.sqlite")).unwrap();
    store.register_account("alpha", "me@work.local").unwrap();
    dir
}

/// A fresh peer cert: a user id, a transport-encryption subkey, and a
/// signing subkey, the shape an Autocrypt header carries.
pub(super) fn gen(user_id: &str) -> sequoia_openpgp::Cert {
    let (cert, _rev) = CertBuilder::new()
        .add_userid(user_id)
        .add_transport_encryption_subkey()
        .add_signing_subkey()
        .generate()
        .expect("CertBuilder::generate");
    cert
}

/// The cert's armored public form, the material `keys import` takes.
pub(super) fn armored(cert: &sequoia_openpgp::Cert) -> String {
    String::from_utf8(cert_to_armored_public(cert).unwrap()).unwrap()
}

/// The cert's fingerprint hex.
pub(super) fn fp(cert: &sequoia_openpgp::Cert) -> String {
    fingerprint_of(cert).as_str().to_owned()
}

/// Stage a peer cert the way the Autocrypt consume does, answering the
/// staging row's id.
pub(super) fn stage(dir: &std::path::Path, peer: &str, cert: &sequoia_openpgp::Cert) -> i64 {
    let store = CryptoStore::open(&dir.join("crypto.sqlite")).unwrap();
    let blob = cert_to_public_blob(cert).unwrap();
    astra_crypto_store::stage_collected_key(
        &store,
        Some("alpha"),
        Some(peer),
        Some("openpgp"),
        Some(&fp(cert)),
        &blob,
        Some("autocrypt"),
    )
    .unwrap()
}

/// The latest trust decision for the peer key, read straight from the
/// store the verbs wrote.
pub(super) fn decision(dir: &std::path::Path, email: &str, fingerprint: &str) -> Option<String> {
    let store = CryptoStore::open(&dir.join("crypto.sqlite")).unwrap();
    astra_crypto_store::get_latest_trust_decision(&store, "alpha", email, "openpgp", fingerprint)
        .unwrap()
        .map(|row| row.decision)
}

/// One run's envelope data, asserting success.
pub(super) fn data(outcome: &super::Run) -> Value {
    assert_eq!(
        outcome.code.exit(),
        0,
        "{}",
        outcome.envelope.render(false).unwrap()
    );
    outcome.envelope.data().unwrap().clone()
}

#[test]
fn keys_list_answers_the_shaped_public_rows_across_both_standards() {
    let dir = seeded();
    let cert = gen(&format!("Peer <{PEER}>"));
    let store = CryptoStore::open(&dir.join("crypto.sqlite")).unwrap();
    astra_crypto_store::import_public_cert(&store, "alpha", armored(&cert).as_bytes()).unwrap();
    // A second standard's row, so the merge across standards is exercised.
    astra_crypto_store::upsert_crypto_key(
        &store,
        &astra_crypto_store::CryptoKeyRecord {
            row: astra_crypto_store::CryptoKeyRow {
                account_id: "alpha".to_owned(),
                standard: "smime".to_owned(),
                key_type: "cert".to_owned(),
                email: Some(PEER.to_owned()),
                fingerprint: "AA01".to_owned(),
                origin: "imported".to_owned(),
                ..Default::default()
            },
            public_data: hex::encode(b"der bytes"),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();

    let answer = data(&run(
        &line(&["keys", "list", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    ));
    let keys = answer["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 2, "both standards merge: {answer}");
    assert_eq!(keys[0]["standard"], "openpgp", "openpgp rows lead");
    assert_eq!(keys[0]["fingerprint"], fp(&cert));
    assert_eq!(keys[0]["email"], PEER);
    assert_eq!(keys[0]["key_type"], "public");
    assert_eq!(keys[0]["origin"], "imported");
    assert_eq!(keys[0]["has_private"], false);
    assert_ne!(keys[0]["created_at"].as_str().unwrap(), "");
    assert_eq!(keys[1]["standard"], "smime");
    for key in keys {
        assert!(key.get("public_data").is_none(), "no blob crosses: {key}");
        assert!(
            key.get("private_data").is_none(),
            "no secret crosses: {key}"
        );
    }
}

#[test]
fn keys_collected_answers_the_staged_rows_newest_first() {
    let dir = seeded();
    let first = gen(&format!("First <{PEER}>"));
    let second = gen(&format!("Second <{PEER}>"));
    let first_id = stage(&dir, PEER, &first);
    let second_id = stage(&dir, PEER, &second);

    let answer = data(&run(
        &line(&["keys", "collected", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    ));
    let keys = answer["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 2);
    // Same-second inserts tie-break on id DESC: the newest staged row leads.
    assert_eq!(keys[0]["id"], second_id);
    assert_eq!(keys[1]["id"], first_id);
    assert_eq!(keys[0]["peer_email"], PEER);
    assert_eq!(keys[0]["standard"], "openpgp");
    assert_eq!(keys[0]["fingerprint"], fp(&second));
    assert_eq!(keys[0]["source"], "autocrypt");
    assert_ne!(keys[0]["seen_at"].as_str().unwrap(), "");
    assert!(keys[0].get("public_data").is_none(), "no blob crosses");
}

#[test]
fn keys_import_lands_the_public_cert_and_the_unverified_trust() {
    let dir = seeded();
    let cert = gen(&format!("Peer <{PEER}>"));
    let answer = data(&run(
        &line(&[
            "keys",
            "import",
            "--account",
            "alpha",
            "--plain",
            &armored(&cert),
        ]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(answer["state"], "imported");
    assert_eq!(answer["key"]["fingerprint"], fp(&cert));
    assert_eq!(answer["key"]["key_type"], "public");
    assert_eq!(answer["key"]["has_private"], false);
    // The default acceptance rides with the import: the send resolver
    // treats unverified as encryption-qualified.
    assert_eq!(
        decision(&dir, PEER, &fp(&cert)).as_deref(),
        Some("unverified")
    );
    // The landed row answers through the read verb too.
    let listed = data(&run(
        &line(&["keys", "list", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(listed["keys"].as_array().unwrap().len(), 1);
}

#[test]
fn keys_import_reads_the_file_and_stdin_sources_and_pins_the_standard() {
    let dir = seeded();
    let cert = gen(&format!("Peer <{PEER}>"));
    let path = dir.join("peer.asc");
    std::fs::write(&path, armored(&cert)).unwrap();
    let from_file = data(&run(
        &line(&[
            "keys",
            "import",
            "--account",
            "alpha",
            "--file",
            path.to_str().unwrap(),
            "--standard",
            "openpgp",
        ]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(from_file["key"]["fingerprint"], fp(&cert));

    let second = gen(&format!("Second <{PEER}>"));
    let mut stdin = std::io::Cursor::new(armored(&second));
    let from_stdin = data(&run(
        &line(&["keys", "import", "--account", "alpha"]),
        &dir,
        &mut stdin,
    ));
    assert_eq!(from_stdin["key"]["fingerprint"], fp(&second));

    // The pinned standard refuses material of the other one.
    let cross = run(
        &line(&[
            "keys",
            "import",
            "--account",
            "alpha",
            "--plain",
            &armored(&cert),
            "--standard",
            "smime",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(cross.code.exit(), 1);
}

#[test]
fn keys_import_wants_one_source_and_a_known_standard() {
    let dir = seeded();
    let cert = gen(&format!("Peer <{PEER}>"));
    let two = run(
        &line(&[
            "keys",
            "import",
            "--account",
            "alpha",
            "--plain",
            &armored(&cert),
            "--file",
            "peer.asc",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(two.code.exit(), 2);
    let text = two.envelope.render(false).unwrap();
    assert!(text.contains("one source"), "{text}");

    let unknown = run(
        &line(&[
            "keys",
            "import",
            "--account",
            "alpha",
            "--plain",
            &armored(&cert),
            "--standard",
            "pgp",
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(unknown.code.exit(), 2);
    let text = unknown.envelope.render(false).unwrap();
    assert!(text.contains("--standard"), "{text}");
}

#[test]
fn keys_import_refuses_secret_material_pointing_at_the_desktop() {
    let dir = seeded();
    let tsk = gen(&format!("Peer <{PEER}>"));
    let path = dir.join("secret.tsk");
    std::fs::write(&path, cert_to_secret_blob(&tsk).unwrap()).unwrap();
    let outcome = run(
        &line(&[
            "keys",
            "import",
            "--account",
            "alpha",
            "--file",
            path.to_str().unwrap(),
        ]),
        &dir,
        &mut std::io::empty(),
    );
    assert_eq!(outcome.code.exit(), 1, "the secret never lands");
    let text = outcome.envelope.render(false).unwrap();
    assert!(text.contains("secret"), "{text}");
    assert!(text.contains("desktop"), "{text}");
    let listed = data(&run(
        &line(&["keys", "list", "--account", "alpha"]),
        &dir,
        &mut std::io::empty(),
    ));
    assert_eq!(listed["keys"], serde_json::json!([]));
}
