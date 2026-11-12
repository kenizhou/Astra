// SPDX-License-Identifier: GPL-3.0-only
//! The gated crypto receive proof against the live harness: the send proof's
//! hand-rolled extraction-and-decrypt tail automated through the product's
//! reading pipeline. astra1 sends an encrypted+signed mail to astra2 through
//! the engine's own outbox (the send proof's front half, the same wrapped
//! dial); the built-in crypto decoder is installed over the recipient's
//! seeded key, and `Intent::OpenMessage` on the delivered row answers the
//! reading snapshot carrying the composer's words, the verified banner, and
//! the restored protected subject, with a `message_crypto_results` row and
//! no ciphertext anywhere in the legs or the attachment chips. A second
//! app over a keyless store proves the no-key negative arm: the banner
//! alone, no plaintext. Every copy is withdrawn at the end.
//!
//! The harness dials mirror the send proof: the same accepted self-signed
//! certificate for IMAP and SMTP alike.

use std::sync::Arc;

use astra_crypto_core::{CryptoBackend, CryptoMethod, CryptoPolicy, KeyGenParams, Standard};
use astra_crypto_store::{CryptoStore, SqliteKeyStore};
use engine_api::{EmailAddress, Engine};

use crate::crypto::CryptoFlags;
use crate::crypto_decode::CryptoDecoder;
use crate::{Account, App, AppObserver, Intent};

use super::{
    account as harness_config,
    crypto_send::dial_wrapped,
    dial_with_accepted_certificate, harness, inbox_key,
    proof::{self, document},
    row_by_subject, CountingObserver,
};

/// The subject the proof sends: unique among the harness fixtures.
const SUBJECT: &str = "Harness crypto receive proof";

/// The words the encrypted body carries; the reading snapshot asserts them.
const WORDS: &str = "Sealed for the reading pipeline, opened by the built-in decoder.";

/// Seeds an account's own OpenPGP key (private material at rest, default
/// signer) and returns its fingerprint.
async fn seed_own(store: &Arc<CryptoStore>, account: &str, email: &str) -> String {
    let backend = astra_crypto_openpgp::OpenpgpBackend::new(
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
    astra_crypto_store::set_default_signing_key(
        store,
        account,
        "openpgp",
        handle.fingerprint.as_str(),
    )
    .unwrap();
    handle.fingerprint.as_str().to_owned()
}

/// Imports `owner`'s public cert into `account` as a contact, so the account
/// can encrypt to the owner (the send gate) or verify the owner's signature
/// (the receive path).
fn import_contact(store: &Arc<CryptoStore>, account: &str, email: &str, owner: &str, fp: &str) {
    let record = astra_crypto_store::get_crypto_key_full(store, owner, "openpgp", fp)
        .unwrap()
        .expect("the owner's key row exists");
    astra_crypto_store::upsert_crypto_key(
        store,
        &astra_crypto_store::CryptoKeyRecord {
            row: astra_crypto_store::CryptoKeyRow {
                account_id: account.to_owned(),
                standard: "openpgp".to_owned(),
                key_type: "cert".to_owned(),
                email: Some(email.to_owned()),
                fingerprint: fp.to_owned(),
                origin: "contact".to_owned(),
                ..Default::default()
            },
            public_data: record.public_data,
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
}

/// The message key a flat list row carries, the ref an open addresses.
fn row_key(row: &astra_viewmodel::MessageRowView) -> engine_api::ProviderKey {
    let astra_viewmodel::RowKey::Message { key, .. } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    key
}

/// The gated run: the delivered sealed mail opens through the reading
/// pipeline to the words, the verified banner, and the restored subject;
/// the keyless app answers the no-key banner and no plaintext.
#[tokio::test]
async fn the_crypto_receive_opens_the_delivered_mail_through_the_reading_pipeline() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    // The fixed test master key keeps the proof off the OS keyring, the
    // offline crypto suite's own discipline.
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);

    // The crypto store first (the boot's ordering): both accounts' own keys,
    // the recipient's public cert with a verified decision on the sender
    // (the send gate), and the sender's public cert on the recipient (the
    // embedded signature verifies).
    let sender_id = harness_config(&host, &port, &user, &password, Vec::new())
        .account_id()
        .expect("the sender's account id");
    let recipient_id = harness_config(&host, &port, proof::RECIPIENT, &password, Vec::new())
        .account_id()
        .expect("the recipient's account id");
    let store = Arc::new(CryptoStore::open_in_memory().expect("the in-memory store opens"));
    store
        .register_account(sender_id.as_str(), "astra1@example.test")
        .unwrap();
    store
        .register_account(recipient_id.as_str(), proof::RECIPIENT)
        .unwrap();
    let sender_fp = seed_own(&store, sender_id.as_str(), "astra1@example.test").await;
    let recipient_fp = seed_own(&store, recipient_id.as_str(), proof::RECIPIENT).await;
    import_contact(
        &store,
        sender_id.as_str(),
        proof::RECIPIENT,
        recipient_id.as_str(),
        &recipient_fp,
    );
    astra_crypto_store::put_trust_decision(
        &store,
        sender_id.as_str(),
        proof::RECIPIENT,
        "openpgp",
        &recipient_fp,
        "verified",
        None,
    )
    .unwrap();
    import_contact(
        &store,
        recipient_id.as_str(),
        "astra1@example.test",
        sender_id.as_str(),
        &sender_fp,
    );

    // The send proof's front half: the wrapped sender dial, the recipient's
    // ordinary dial, the app over both with the built-in crypto decoder
    // installed over the seeded store.
    let wrapper = astra_crypto_store::mime_wrapper_for(
        &store,
        sender_id.as_str(),
        "astra1@example.test",
        CryptoMethod::Openpgp,
    )
    .expect("a crypto method installs a wrapper");
    let (sender, sender_providers) = dial_wrapped(&host, &port, &user, &password, wrapper).await;
    assert_eq!(sender, sender_id, "the dial answers the config-derived id");
    let (recipient, recipient_providers, _, _) =
        dial_with_accepted_certificate(&host, &port, proof::RECIPIENT, &password).await;
    assert_eq!(recipient, recipient_id);
    let engine = Arc::new(Engine::open_in_memory().expect("the engine store"));
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        engine.clone(),
        vec![
            Account {
                id: sender.clone(),
                providers: sender_providers,
                identity: EmailAddress::new("astra1@example.test"),
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
    proof::delete_subject_mail(&app, &sender, &[SUBJECT, "..."]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT, "..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The send: the composer's gesture, wrapped inside the engine's drain.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: document(WORDS),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
        composition: None,
    })
    .await
    .unwrap();
    assert!(
        app.sending().rows.is_empty(),
        "a delivered crypto send leaves nothing in the queue: {:?}",
        app.sending()
    );

    // The delivery: the recipient's INBOX holds the sealed mail, the outer
    // subject the protected placeholder.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = crate::FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, "...");
    assert_eq!(row.sender_address, "astra1@example.test");
    let key = row_key(row);

    // The receive: the product's own open, the built-in decoder answering.
    app.dispatch(Intent::OpenMessage {
        message: crate::reference::MessageRef {
            account: recipient.clone(),
            key: key.clone(),
        },
    })
    .await
    .unwrap();
    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert_eq!(reading.subject, SUBJECT, "the protected subject restores");
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "the composer's words render in the plain leg: {:?}",
        reading.plain
    );
    assert!(
        reading
            .html
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "the composer's words render in the html leg: {:?}",
        reading.html
    );
    let banner = reading
        .security_banner
        .as_ref()
        .expect("the verified banner rides the snapshot");
    assert_eq!(banner.level, astra_viewmodel::SecurityLevelView::Info);
    assert!(
        banner.text.contains("signature verified"),
        "the banner states the verified signature: {}",
        banner.text
    );
    for leg in [reading.plain.as_deref(), reading.html.as_deref()]
        .into_iter()
        .flatten()
    {
        assert!(
            !leg.contains("BEGIN PGP") && !leg.contains("encrypted.asc"),
            "the ciphertext never renders: {leg}"
        );
    }
    assert!(
        !reading
            .attachments
            .iter()
            .any(|chip| chip.file_name.eq_ignore_ascii_case("encrypted.asc")),
        "the ciphertext chip hides: {:?}",
        reading.attachments
    );
    let result =
        astra_crypto_store::get_message_crypto_result(&store, recipient.as_str(), key.as_str())
            .expect("the result row lands keyed by the account and provider key");
    assert_eq!(result.crypto_kind, "encrypted-signed");
    assert_eq!(result.decrypt_state, "ok");
    assert_eq!(result.signature_state, "valid-verified");
    assert_eq!(
        result.signer_fingerprint.as_deref(),
        Some(sender_fp.as_str())
    );

    // The no-key negative arm: a fresh app over a keyless store, the same
    // delivered mail. The banner alone, no plaintext, no restored subject.
    let keyless_store = Arc::new(CryptoStore::open_in_memory().expect("the keyless store opens"));
    keyless_store
        .register_account(recipient_id.as_str(), proof::RECIPIENT)
        .unwrap();
    let (keyless_recipient, keyless_providers, _, _) =
        dial_with_accepted_certificate(&host, &port, proof::RECIPIENT, &password).await;
    let observer2: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let keyless_app = App::new(
        Arc::new(Engine::open_in_memory().expect("the keyless engine store")),
        vec![Account {
            id: keyless_recipient.clone(),
            providers: keyless_providers,
            identity: EmailAddress::new(proof::RECIPIENT),
        }],
        None,
        observer2,
    )
    .with_decoder(Some(Arc::new(CryptoDecoder::new(keyless_store.clone()))));
    keyless_app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox2 = crate::FolderRef::from_parts(
        keyless_recipient.as_str(),
        inbox_key(&keyless_app, &keyless_recipient),
    )
    .expect("the keyless app's INBOX reference");
    keyless_app
        .dispatch(Intent::SelectFolder { folder: inbox2 })
        .await
        .unwrap();
    let keyless_snapshot = keyless_app.mailbox_list();
    let row2 = row_by_subject(&keyless_snapshot, "...");
    let key2 = row_key(row2);
    keyless_app
        .dispatch(Intent::OpenMessage {
            message: crate::reference::MessageRef {
                account: keyless_recipient.clone(),
                key: key2.clone(),
            },
        })
        .await
        .unwrap();
    let closed = keyless_app
        .reading()
        .expect("the keyless open publishes a reading snapshot");
    let closed_banner = closed
        .security_banner
        .as_ref()
        .expect("the no-key banner rides the snapshot");
    assert_eq!(
        closed_banner.level,
        astra_viewmodel::SecurityLevelView::Error
    );
    assert!(
        closed_banner.text.contains("cannot decrypt (no key)"),
        "the banner states the no-key refusal: {}",
        closed_banner.text
    );
    assert_eq!(closed.plain, None, "no key never yields plaintext");
    assert_eq!(closed.html, None, "no key never yields ciphertext");
    assert_eq!(closed.subject, "...", "no subject restores on a closed arm");
    let closed_result = astra_crypto_store::get_message_crypto_result(
        &keyless_store,
        keyless_recipient.as_str(),
        key2.as_str(),
    )
    .expect("the keyless verdict lands its row");
    assert_eq!(closed_result.decrypt_state, "no-key");

    // The cleanup: the proof mail leaves both accounts through the engine's
    // own delete path (the recipient's INBOX and the sender's Sent alike).
    proof::delete_subject_mail(&app, &sender, &["..."]).await;
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
