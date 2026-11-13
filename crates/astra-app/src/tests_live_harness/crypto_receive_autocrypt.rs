// SPDX-License-Identifier: GPL-3.0-only
//! The gated Autocrypt consume proof against the live harness: the receive
//! half of SP4c-3a driven through the product's own reading pipeline. The send
//! gate (`apply_crypto`) builds each wire source, so the protected-headers
//! layout and the `Autocrypt-Gossip:` emission are the gate's own; the sources
//! reach astra2 through the harness's raw SMTP submission listener (the
//! reading-proof discipline), because the gate emits no cleartext `Autocrypt:`
//! header of its own and the two-user harness cannot deliver a gossip mail to
//! two non-sender recipients. `Intent::OpenMessage` on each delivered row runs
//! the built-in crypto decoder, and the silent consume stages the observed keys
//! into `collected_keys` (source `autocrypt`) after the verdict is persisted.
//!
//! Four arms, each withdrawn before the next so the protected-subject
//! placeholder row stays unambiguous: an addr that is not From stages nothing;
//! malformed keydata stages nothing; a cleartext Autocrypt header whose addr
//! matches From stages the sender's key; and a gossip mail encrypted and signed
//! to two recipients stages the peer keys from inside the ciphertext. The
//! recipient holds no signer cert for the sender, so the staging is silent: it
//! never changes the message's own decrypt verdict.

use std::sync::Arc;

use astra_crypto_core::{
    apply_crypto, AddressSpec, CryptoBackend, CryptoPolicy, DefaultSigningKey, KeyGenParams,
    KeyStore, SendDraft, Standard,
};
use astra_crypto_openpgp::OpenpgpBackend;
use astra_crypto_store::{
    autocrypt, crypto_policy, get_crypto_key_full, list_collected_keys_for_account,
    put_trust_decision, set_default_signing_key, upsert_crypto_key, CollectedKeyRow,
    CryptoKeyRecord, CryptoKeyRow, CryptoStore, SqliteKeyStore, StoreCryptoPolicy,
};
use engine_api::{AccountId, EmailAddress, Engine, Provider};

use crate::crypto_decode::CryptoDecoder;
use crate::{Account, App, AppObserver, FolderRef, Intent};

use super::{
    account as harness_config, dial_with_accepted_certificate, harness, inbox_key, proof,
    row_by_subject, CountingObserver,
};

/// The sender's address, the first harness user.
const SENDER: &str = "astra1@example.test";
/// A third peer the gossip arm encrypts to: a contact key on the sender, not a
/// harness user (the raw submission delivers only to the recipient).
const ASTRA3: &str = "astra3@example.test";
/// The subject the proof sends: unique among the harness fixtures.
const SUBJECT: &str = "Harness Autocrypt consume proof";
/// The words the encrypted body carries; the reading snapshot asserts them.
const WORDS: &str = "Sealed for the Autocrypt consume proof, opened by the built-in decoder.";

/// Seeds an account's own OpenPGP key (private material at rest, default
/// signer) and returns its fingerprint.
async fn seed_own(store: &Arc<CryptoStore>, account: &str, email: &str) -> String {
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
fn public_blob(store: &Arc<CryptoStore>, account: &str, fingerprint: &str) -> Vec<u8> {
    let record = get_crypto_key_full(store, account, "openpgp", fingerprint)
        .unwrap()
        .expect("the key row exists");
    hex::decode(record.public_data).expect("the public blob hex")
}

/// Imports a public cert into an account as a contact (no private material).
fn import_contact(store: &Arc<CryptoStore>, account: &str, email: &str, blob: &[u8], fp: &str) {
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
fn minimal_blob(store: &Arc<CryptoStore>, account: &str, fingerprint: &str) -> Vec<u8> {
    let blob = public_blob(store, account, fingerprint);
    let (_, cert) = autocrypt::parse_first_cert(&blob).expect("parse the sender cert");
    autocrypt::minimal_export(&cert).expect("the minimal export")
}

/// Drives the real send gate over a plain draft, answering the full RFC5322
/// source it wraps (sign + encrypt, protected subject, and the gate's own
/// gossip emission when two non-sender recipients resolve).
async fn gate_source(
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
/// submission listener: curl over implicit TLS, the reading-proof discipline.
/// Seeding only; every assertion that follows reads through the product.
fn submit_raw(host: &str, password: &str, source: &[u8]) {
    let path =
        std::env::temp_dir().join(format!("astra-autocrypt-proof-{}.eml", std::process::id()));
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
        .arg(format!("smtps://{host}:{}", super::harness_smtp_port()))
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
async fn open_current(
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

/// The recipient's staged OpenPGP keys, newest first.
fn staged(store: &Arc<CryptoStore>, recipient: &str) -> Vec<CollectedKeyRow> {
    list_collected_keys_for_account(store, recipient, "openpgp").unwrap()
}

/// The gated run: the consume stages the sender key from a cleartext Autocrypt
/// header and the peer keys from inside a gossip ciphertext, while an addr that
/// is not From and malformed keydata stage nothing. Every copy is withdrawn.
#[tokio::test]
async fn the_autocrypt_consume_stages_inbound_keys_through_the_reading_pipeline() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    // The fixed test master key keeps the proof off the OS keyring.
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);

    // The crypto store first (the boot's ordering). The recipient holds its own
    // decrypt key but NO signer cert for the sender, so the consume's staging is
    // silent: it never changes a message's own verdict. astra3 is a store-only
    // peer the gossip arm encrypts to.
    let sender_id = harness_config(&host, &port, &user, &password, Vec::new())
        .account_id()
        .expect("the sender's account id");
    let recipient_id = harness_config(&host, &port, proof::RECIPIENT, &password, Vec::new())
        .account_id()
        .expect("the recipient's account id");
    let store = Arc::new(CryptoStore::open_in_memory().expect("the in-memory store opens"));
    store.register_account(sender_id.as_str(), SENDER).unwrap();
    store
        .register_account(recipient_id.as_str(), proof::RECIPIENT)
        .unwrap();
    store.register_account("astra3", ASTRA3).unwrap();
    let sender_fp = seed_own(&store, sender_id.as_str(), SENDER).await;
    let recipient_fp = seed_own(&store, recipient_id.as_str(), proof::RECIPIENT).await;
    let astra3_fp = seed_own(&store, "astra3", ASTRA3).await;

    // The sender encrypts to the recipient and to astra3: both contacts carry a
    // verified decision so the gate's trust gate admits them.
    crypto_policy::set_policy_flag(&store, sender_id.as_str(), "send_autocrypt_headers", true)
        .unwrap();
    let recipient_blob = public_blob(&store, recipient_id.as_str(), &recipient_fp);
    import_contact(
        &store,
        sender_id.as_str(),
        proof::RECIPIENT,
        &recipient_blob,
        &recipient_fp,
    );
    put_trust_decision(
        &store,
        sender_id.as_str(),
        proof::RECIPIENT,
        "openpgp",
        &recipient_fp,
        "verified",
        None,
    )
    .unwrap();
    let astra3_blob = public_blob(&store, "astra3", &astra3_fp);
    import_contact(&store, sender_id.as_str(), ASTRA3, &astra3_blob, &astra3_fp);
    put_trust_decision(
        &store,
        sender_id.as_str(),
        ASTRA3,
        "openpgp",
        &astra3_fp,
        "verified",
        None,
    )
    .unwrap();
    let minimal = minimal_blob(&store, sender_id.as_str(), &sender_fp);

    // Both accounts dial ordinarily (the sender's dial files and withdraws the
    // Sent copies); the built-in crypto decoder rides the seeded store.
    let (sender, sender_providers, _, _) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;
    assert_eq!(sender, sender_id, "the dial answers the config-derived id");
    let (recipient, recipient_providers, _, _) =
        dial_with_accepted_certificate(&host, &port, proof::RECIPIENT, &password).await;
    assert_eq!(recipient, recipient_id);
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![
            Account {
                id: sender.clone(),
                providers: sender_providers,
                identity: EmailAddress::new(SENDER),
            },
            Account {
                id: recipient.clone(),
                providers: recipient_providers,
                identity: EmailAddress::new(proof::RECIPIENT),
            },
        ],
        None,
        observer,
    )
    .with_decoder(Some(Arc::new(CryptoDecoder::new(store.clone()))));

    // A known state: sweep any earlier run's proof mail first.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &["..."]).await;
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // Arm 1: an Autocrypt addr that is not From stages nothing (the Level 1
    // binding discards the mismatched header).
    let source = gate_source(&store, sender_id.as_str(), &sender_fp, &[proof::RECIPIENT]).await;
    let header = autocrypt::emit_autocrypt_header("intruder@example.test", &minimal);
    submit_raw(
        &host,
        &password,
        &autocrypt::inject_headers(&source, &[header]),
    );
    let reading = open_current(&app, &recipient).await;
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "the mismatched-arm mail still decrypts: {:?}",
        reading.plain
    );
    assert!(
        staged(&store, recipient.as_str()).is_empty(),
        "the addr-to-From binding discards the mismatched header"
    );
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;

    // Arm 2: malformed keydata stages nothing and never panics (the addr matches
    // From, but the blob holds no cert).
    let source = gate_source(&store, sender_id.as_str(), &sender_fp, &[proof::RECIPIENT]).await;
    let header = autocrypt::emit_autocrypt_header(SENDER, b"this is not an openpgp cert");
    submit_raw(
        &host,
        &password,
        &autocrypt::inject_headers(&source, &[header]),
    );
    let reading = open_current(&app, &recipient).await;
    assert!(reading
        .plain
        .as_deref()
        .is_some_and(|body| body.contains(WORDS)));
    assert!(
        staged(&store, recipient.as_str()).is_empty(),
        "malformed keydata stages nothing"
    );
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;

    // Arm 3: a cleartext Autocrypt header whose addr matches From stages the
    // sender's key, silently (the recipient holds no signer cert, so this
    // message's own verdict is unchanged by the staged key).
    let source = gate_source(&store, sender_id.as_str(), &sender_fp, &[proof::RECIPIENT]).await;
    let header = autocrypt::emit_autocrypt_header(SENDER, &minimal);
    submit_raw(
        &host,
        &password,
        &autocrypt::inject_headers(&source, &[header]),
    );
    let reading = open_current(&app, &recipient).await;
    assert!(reading
        .plain
        .as_deref()
        .is_some_and(|body| body.contains(WORDS)));
    let rows = staged(&store, recipient.as_str());
    assert_eq!(rows.len(), 1, "the sender key stages once");
    assert_eq!(rows[0].source.as_deref(), Some("autocrypt"));
    assert_eq!(rows[0].peer_email.as_deref(), Some(SENDER));
    assert_eq!(rows[0].fingerprint.as_deref(), Some(sender_fp.as_str()));
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;

    // Arm 4: the gate's own gossip emission (encrypted + signed to two
    // non-sender recipients) rides inside the ciphertext; the consume stages the
    // peer keys from the decrypted inner header block.
    let source = gate_source(
        &store,
        sender_id.as_str(),
        &sender_fp,
        &[proof::RECIPIENT, ASTRA3],
    )
    .await;
    submit_raw(&host, &password, &source);
    let reading = open_current(&app, &recipient).await;
    assert!(reading
        .plain
        .as_deref()
        .is_some_and(|body| body.contains(WORDS)));
    let rows = staged(&store, recipient.as_str());
    let peers: Vec<&str> = rows
        .iter()
        .filter_map(|row| row.peer_email.as_deref())
        .collect();
    assert!(
        peers.contains(&proof::RECIPIENT) && peers.contains(&ASTRA3),
        "both gossip peer keys stage: {peers:?}"
    );
    for row in &rows {
        assert_eq!(row.source.as_deref(), Some("autocrypt"));
    }
    let fps: Vec<&str> = rows
        .iter()
        .filter_map(|row| row.fingerprint.as_deref())
        .collect();
    assert!(
        fps.contains(&recipient_fp.as_str()) && fps.contains(&astra3_fp.as_str()),
        "the gossip fingerprints match the peers: {fps:?}"
    );

    // The cleanup: every proof mail leaves both accounts (the recipient's INBOX
    // and the sender's Sent alike).
    proof::delete_subject_mail(&app, &sender, &["..."]).await;
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
