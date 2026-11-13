// SPDX-License-Identifier: GPL-3.0-only
//! The seeding helpers the collected-key review proof drives: the store seed,
//! the real send gate's source builder, the raw SMTP submission, and the product
//! reading open. Split from the proof file so that file stays under the line
//! cap. Seeding only; every assertion in the proof reads through the product.

use std::sync::Arc;

use astra_crypto_core::{
    apply_crypto, AddressSpec, CryptoBackend, CryptoPolicy, DefaultSigningKey, KeyGenParams,
    KeyStore, SendDraft, Standard,
};
use astra_crypto_openpgp::OpenpgpBackend;
use astra_crypto_store::{
    autocrypt, get_crypto_key_full, set_default_signing_key, upsert_crypto_key, CryptoKeyRecord,
    CryptoKeyRow, CryptoStore, SqliteKeyStore, StoreCryptoPolicy,
};
use engine_api::{AccountId, Provider};

use crate::tests_live_harness::{harness_smtp_port, inbox_key, proof, row_by_subject};
use crate::{App, FolderRef, Intent};

/// The peer whose key is staged and promoted: the first harness user.
pub(super) const SENDER: &str = "astra1@example.test";
/// The subject the proof sends: unique among the harness fixtures.
pub(super) const SUBJECT: &str = "Harness collected review proof";
/// The words the encrypted body carries; the reading snapshot asserts them.
pub(super) const WORDS: &str =
    "Sealed for the collected review proof, admitted by the promote alone.";

/// Seeds an account's own OpenPGP key (private material at rest, default signer).
pub(super) async fn seed_own(store: &Arc<CryptoStore>, account: &str, email: &str) -> String {
    let backend = OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), account)),
        CryptoPolicy::default(),
    );
    let handle = backend
        .generate_key(KeyGenParams {
            standard: Standard::OpenPgp,
            user_id: format!("Harness <{email}>"),
            algorithm: "default".to_owned(),
            passphrase: None,
        })
        .await
        .expect("generate the account's key");
    set_default_signing_key(store, account, "openpgp", handle.fingerprint.as_str()).unwrap();
    handle.fingerprint.as_str().to_owned()
}

/// The stored public blob of an account's key.
pub(super) fn public_blob(store: &Arc<CryptoStore>, account: &str, fingerprint: &str) -> Vec<u8> {
    let record = get_crypto_key_full(store, account, "openpgp", fingerprint)
        .unwrap()
        .expect("the key row exists");
    hex::decode(record.public_data).expect("the public blob hex")
}

/// Imports a public cert into an account as a contact (no private material).
pub(super) fn import_contact(
    store: &Arc<CryptoStore>,
    account: &str,
    email: &str,
    blob: &[u8],
    fp: &str,
) {
    upsert_crypto_key(
        store,
        &CryptoKeyRecord {
            row: CryptoKeyRow {
                account_id: account.to_owned(),
                standard: "openpgp".to_owned(),
                key_type: "cert".to_owned(),
                email: Some(email.to_owned()),
                fingerprint: fp.to_owned(),
                origin: "contact".to_owned(),
                ..Default::default()
            },
            public_data: hex::encode(blob),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
}

/// The sender's minimal-export blob: the exact bytes an Autocrypt header carries.
pub(super) fn minimal_blob(store: &Arc<CryptoStore>, account: &str, fingerprint: &str) -> Vec<u8> {
    let blob = public_blob(store, account, fingerprint);
    let (_, cert) = autocrypt::parse_first_cert(&blob).expect("parse the sender cert");
    autocrypt::minimal_export(&cert).expect("the minimal export")
}

/// Drives the real send gate over a plain draft, answering the RFC5322 source it
/// wraps (sign + encrypt, protected subject, the gate's gossip emission when two
/// non-sender recipients resolve).
pub(super) async fn gate_source(
    store: &Arc<CryptoStore>,
    sender_id: &str,
    sender_fp: &str,
    to: &[&str],
) -> Vec<u8> {
    let keystore: Arc<dyn KeyStore> = Arc::new(SqliteKeyStore::new(store.clone(), sender_id));
    let backend = OpenpgpBackend::new(keystore.clone(), CryptoPolicy::default());
    let policy = StoreCryptoPolicy::new(store, sender_id);
    let default_key = DefaultSigningKey {
        standard: "openpgp".to_owned(),
        fingerprint: sender_fp.to_owned(),
        email: Some(SENDER.to_owned()),
        token_serial: None,
        token_key_id: None,
    };
    let draft = SendDraft {
        to: to
            .iter()
            .map(|email| AddressSpec {
                name: None,
                email: (*email).to_owned(),
            })
            .collect(),
        sign: true,
        encrypt: true,
        ..Default::default()
    };
    let mime = format!(
        "From: {SENDER}\r\nTo: {}\r\nSubject: {SUBJECT}\r\nMIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\n{WORDS}\r\n",
        to.join(", ")
    );
    apply_crypto(
        &backend,
        Standard::OpenPgp,
        keystore.as_ref(),
        mime.as_bytes(),
        &draft,
        SENDER,
        Some(&default_key),
        &policy,
    )
    .await
    .expect("the send gate wraps the source")
}

/// Submits a raw source from astra1 to astra2 through the harness's SMTP
/// submission listener (curl over implicit TLS). Seeding only; every assertion
/// that follows reads through the product.
pub(super) fn submit_raw(host: &str, password: &str, source: &[u8]) {
    let path =
        std::env::temp_dir().join(format!("astra-collected-proof-{}.eml", std::process::id()));
    std::fs::write(&path, source).expect("the raw submission file writes");
    let output = std::process::Command::new("curl")
        .arg("-sk")
        .arg("--user")
        .arg(format!("{SENDER}:{password}"))
        .arg("--mail-from")
        .arg(SENDER)
        .arg("--mail-rcpt")
        .arg(proof::RECIPIENT)
        .arg("--url")
        .arg(format!("smtps://{host}:{}", harness_smtp_port()))
        .arg("-T")
        .arg(&path)
        .output()
        .expect("the raw submission spawns");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "the raw submission delivered: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Opens the one delivered placeholder row in the recipient's INBOX through the
/// product's reading pipeline, answering the reading snapshot.
pub(super) async fn open_current(
    app: &App<Box<dyn Provider>>,
    recipient: &AccountId,
) -> astra_viewmodel::ReadingSnapshot {
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox =
        FolderRef::from_parts(recipient.as_str(), inbox_key(app, recipient)).expect("the INBOX");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, "...");
    let astra_viewmodel::RowKey::Message { key, .. } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::OpenMessage {
        message: crate::reference::MessageRef {
            account: recipient.clone(),
            key,
        },
    })
    .await
    .unwrap();
    app.reading()
        .expect("the open publishes a reading snapshot")
}
