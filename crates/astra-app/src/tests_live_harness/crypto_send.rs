// SPDX-License-Identifier: GPL-3.0-only
//! The gated crypto send proof against the live harness: the whole chain end
//! to end, now through the engine's MIME-wrap seam (ECS). astra1's account
//! dials with the store-bound wrapper installed, so its submit rides the
//! engine's own outbox: the wrapper wraps inside the drain, the delivered
//! message proves the protected subject (`...` in the clear), the Sent copy
//! is filed by the ENGINE's own Sent placement, and the recipient's copy
//! decrypts back to the composer's words with astra2's key. A second send to
//! a recipient with no key is refused by the wrapper pre-dial, so the engine
//! op settles FAILED with the trust words and nothing reaches the server;
//! once the key exists, the queue's own Send Now re-wraps and the send gets
//! as far as the transport (which refuses the unknown local user: the proof
//! the retry re-ran the wrap).
//!
//! The harness dials mirror the other send proofs: the same accepted
//! self-signed certificate for IMAP and SMTP alike (Stalwart presents the
//! one certificate on both listeners).

use std::sync::Arc;

use base64::Engine as _;

use astra_crypto_core::{CryptoBackend, CryptoMethod, CryptoPolicy, KeyGenParams, Standard};
use astra_crypto_store::{CryptoStore, SqliteKeyStore};
use astra_viewmodel::FolderRole;
use engine_api::{AccountId, EmailAddress, Engine};

use crate::crypto::CryptoFlags;
use crate::tests_live_harness::CountingObserver;
use crate::{Account, App, AppObserver, Intent};

use super::{
    account as harness_config, dial_with_accepted_certificate, harness, inbox_key,
    proof::{self, document},
    row_by_subject,
};
use astra_account::{connect_mail_providers, CertificateException, ImapConnections};

/// The subject the proof sends: unique among the harness fixtures.
const SUBJECT: &str = "Harness crypto send proof";

/// The words the encrypted body carries; the decrypt asserts them back.
const WORDS: &str = "Sealed by the built-in gate through the harness outbox.";

/// The sender dial, with the store-bound MIME wrapper installed at connect
/// time: the boot's own ordering (the crypto store opens before the dial).
async fn dial_wrapped(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
    wrapper: Arc<dyn engine_api::MimeWrapper>,
) -> (AccountId, Vec<Box<dyn engine_api::Provider>>) {
    let config = harness_config(host, port, user, password, Vec::new());
    let account_id = config.account_id().expect("the account id");
    let connections = ImapConnections::new();
    let Err(error) = connect_mail_providers(&connections, &config, &account_id, None).await else {
        panic!("the harness certificate was trusted without an accepted exception");
    };
    let astra_account::AccountError::CertificateRejected { rejected, .. } = error else {
        panic!("the self-signed certificate was not offered as a rejection: {error}");
    };
    let accepted = CertificateException::new(rejected.server_name(), rejected.certificate());
    let config = harness_config(host, port, user, password, vec![accepted]);
    let providers = connect_mail_providers(&connections, &config, &account_id, Some(wrapper))
        .await
        .expect("the dial with the accepted certificate");
    (account_id, providers)
}

/// The gated run: an encrypted send rides the engine's outbox wrapped and is
/// delivered sealed, the Sent copy is filed by the engine, a missing-key
/// recipient fails the op pre-dial, and Send Now re-wraps once the key exists.
#[tokio::test]
async fn the_crypto_send_rides_the_engine_queue_and_decrypts_for_the_recipient() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };

    // The crypto store first (the boot's ordering): the sender's own key
    // (default signer) and the recipient's public cert with an explicit
    // verified decision. The account id is the config-derived one, so the
    // store row keys match the dial.
    let sender_id = harness_config(&host, &port, &user, &password, Vec::new())
        .account_id()
        .expect("the sender's account id");
    let store = Arc::new(CryptoStore::open_in_memory().expect("the in-memory store opens"));
    store
        .register_account(sender_id.as_str(), "astra1@example.test")
        .unwrap();
    let backend = astra_crypto_openpgp::OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), sender_id.as_str())),
        CryptoPolicy::default(),
    );
    let own = backend
        .generate_key(KeyGenParams {
            standard: Standard::OpenPgp,
            user_id: "Astra One <astra1@example.test>".to_owned(),
            algorithm: "default".to_owned(),
            passphrase: None,
        })
        .await
        .expect("generate the sender's key");
    astra_crypto_store::set_default_signing_key(
        &store,
        sender_id.as_str(),
        "openpgp",
        own.fingerprint.as_str(),
    )
    .unwrap();
    let recipient_cert = astra_crypto_openpgp::engine::generate("Astra Two <astra2@example.test>")
        .expect("generate the recipient's key");
    let recipient_fp = astra_crypto_openpgp::keymap::fingerprint_of(&recipient_cert)
        .as_str()
        .to_owned();
    astra_crypto_store::upsert_crypto_key(
        &store,
        &astra_crypto_store::CryptoKeyRecord {
            row: astra_crypto_store::CryptoKeyRow {
                account_id: sender_id.as_str().to_owned(),
                standard: "openpgp".to_owned(),
                key_type: "cert".to_owned(),
                email: Some(proof::RECIPIENT.to_owned()),
                fingerprint: recipient_fp.clone(),
                origin: "contact".to_owned(),
                ..Default::default()
            },
            public_data: hex::encode(
                astra_crypto_openpgp::keymap::cert_to_public_blob(&recipient_cert)
                    .expect("the recipient's public blob"),
            ),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
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

    // The wrapper, built by the store's factory exactly as the host builds it
    // at dial time, installed at connect.
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

    // The app over the product's own dial; no Astra-side crypto installation
    // exists anymore: the engine's queue owns the send.
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
    );

    // A known state: sweep any earlier run's proof mail first.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[SUBJECT, "..."]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT, "..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The send: the composer's gesture. The flags stamp onto the engine's
    // draft; the wrapper wraps inside the engine's drain.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: document(WORDS),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
    })
    .await
    .unwrap();

    assert!(
        app.sending().rows.is_empty(),
        "a delivered crypto send leaves nothing in the queue: {:?}",
        app.sending()
    );
    assert!(
        app.sending().held.is_empty(),
        "the engine path holds no Astra-side gate row: {:?}",
        app.sending()
    );

    // The delivery: one refresh and the recipient's INBOX holds the mail.
    // The subject in the clear is the protected placeholder: the harness
    // sees no subject at all, the protected-headers proof end to end.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = crate::FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, "...");
    assert_eq!(
        row.sender_address, "astra1@example.test",
        "the delivered sealed mail is from the sender"
    );

    // The Sent copy: filed by the ENGINE's own Sent placement, placeholder
    // subject and all.
    let sent = crate::FolderRef::from_parts(
        sender.as_str(),
        proof::role_key(&app, &sender, FolderRole::Sent),
    )
    .expect("the sender's Sent folder reference");
    app.dispatch(Intent::SelectFolder { folder: sent })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let filed = row_by_subject(&snapshot, "...");
    assert_eq!(
        filed.sender_address, "astra1@example.test",
        "the filed copy is the sender's own sealed mail"
    );

    // The strongest proof: the delivered bytes on the server decrypt back
    // to the composer's words with the recipient's key, and nowhere carry
    // them in the clear. Fetch the source through a fresh verification
    // dial of the recipient, resolving the message through the app's own
    // engine store.
    let (_, verify_providers, _, _) =
        dial_with_accepted_certificate(&host, &port, proof::RECIPIENT, &password).await;
    let message = crate::reading::resolve(
        &engine,
        &crate::reference::MessageRef {
            account: recipient.clone(),
            key: {
                let astra_viewmodel::RowKey::Message { key, .. } = row.key.clone() else {
                    panic!("a flat list row is keyed by its message");
                };
                key
            },
        },
    )
    .await
    .expect("the delivered message resolves");
    let source = engine
        .message_source(
            verify_providers.first().expect("a provider"),
            &recipient,
            &message,
        )
        .await
        .expect("the delivered source fetches");
    let source_bytes: &[u8] = source.as_bytes();
    let source_text = String::from_utf8_lossy(source_bytes).to_string();
    assert!(
        source_text.contains("multipart/encrypted"),
        "the delivered message is PGP/MIME: {source_text}"
    );
    assert!(
        source_text.contains("Subject: ..."),
        "the outer subject is the placeholder on the server too"
    );
    assert!(
        !source_text.contains(WORDS),
        "the words never touch the wire in the clear"
    );

    // Decrypt with the recipient's key: the inner entity carries the
    // protected-headers wrapper with the real subject and the words.
    let message_bytes = {
        let marker = "Content-Disposition: inline; filename=\"encrypted.asc\"\r\n\r\n";
        let start = source_text.find(marker).expect("the encrypted part") + marker.len();
        let end = source_text[start..]
            .find("\r\n------=_astra_pgp_encrypted_0001--")
            .expect("the closing boundary")
            + start;
        let b64: String = source_text[start..end]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap()
    };
    let recipient_store = Arc::new(CryptoStore::open_in_memory().expect("the recipient store"));
    recipient_store
        .register_account("verify", "astra2@example.test")
        .unwrap();
    let recipient_backend = astra_crypto_openpgp::OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(recipient_store.clone(), "verify")),
        CryptoPolicy::default(),
    );
    let recipient_handle = recipient_backend
        .import_key(
            &astra_crypto_openpgp::keymap::cert_to_secret_blob(&recipient_cert)
                .expect("the recipient's secret blob"),
            None,
        )
        .await
        .expect("import the recipient's key");
    let decrypted = recipient_backend
        .decrypt(astra_crypto_core::DecryptOp {
            envelope: &astra_crypto_core::EncryptedEnvelope {
                standard: Standard::OpenPgp,
                serialization: astra_crypto_core::SerializationStrategy::SingleMimeBlob,
                parts: vec![astra_crypto_core::EncryptedPart {
                    id: astra_crypto_core::PartId("body".into()),
                    kind: astra_crypto_core::PartKind::Body,
                    ciphertext: message_bytes,
                    signature: None,
                }],
                recipients: Vec::new(),
            },
            decryption_key: recipient_handle,
        })
        .await
        .expect("the recipient decrypts");
    let inner = String::from_utf8(decrypted.parts[0].data.clone()).unwrap();
    assert!(
        inner.contains(&format!("Subject: {SUBJECT}")),
        "the real subject rides inside: {inner}"
    );
    assert!(
        inner.contains(WORDS),
        "the composer's words survive: {inner}"
    );

    // The refused half: a recipient with no key is refused by the wrapper
    // pre-dial, so the ENGINE's op settles Failed (permanent) carrying the
    // trust words: no held row, and nothing reaches the server.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: "ghost@example.test".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: SUBJECT.to_owned(),
        document: document(WORDS),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
    })
    .await
    .unwrap();
    let rows = app.sending().rows;
    let failed_rows: Vec<_> = rows
        .iter()
        .filter(|row| row.state == astra_viewmodel::QueuedState::Failed)
        .collect();
    assert_eq!(
        failed_rows.len(),
        1,
        "the engine's failed row stands: {rows:?}"
    );
    assert!(
        app.sending().held.is_empty(),
        "a trust refusal is the engine's failed op, never a held row"
    );
    let op = engine
        .outbox(&sender)
        .await
        .expect("the engine outbox reads")
        .into_iter()
        .find(|op| op.state == engine_api::PendingOpState::Failed)
        .expect("the failed op is recorded");
    let detail = op.detail.clone().unwrap_or_default();
    assert!(
        detail.contains("not every recipient can be encrypted to"),
        "the op carries the gate's reason: {detail}"
    );
    assert!(
        detail.contains("ghost@example.test"),
        "the op names the keyless recipient: {detail}"
    );

    // The retry half: the user fixes the keys (ghost's cert lands with a
    // verified decision), the queue's own Send Now re-runs the whole wrap :
    // and the send now gets as far as the transport, which refuses the
    // unknown local user. The op's detail changing from the gate's words to
    // SMTP's is the proof the retry re-wrapped.
    let ghost_cert = astra_crypto_openpgp::engine::generate("Ghost <ghost@example.test>")
        .expect("generate ghost's key");
    let ghost_fp = astra_crypto_openpgp::keymap::fingerprint_of(&ghost_cert)
        .as_str()
        .to_owned();
    astra_crypto_store::upsert_crypto_key(
        &store,
        &astra_crypto_store::CryptoKeyRecord {
            row: astra_crypto_store::CryptoKeyRow {
                account_id: sender.as_str().to_owned(),
                standard: "openpgp".to_owned(),
                key_type: "cert".to_owned(),
                email: Some("ghost@example.test".to_owned()),
                fingerprint: ghost_fp.clone(),
                origin: "contact".to_owned(),
                ..Default::default()
            },
            public_data: hex::encode(
                astra_crypto_openpgp::keymap::cert_to_public_blob(&ghost_cert)
                    .expect("ghost's public blob"),
            ),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
    astra_crypto_store::put_trust_decision(
        &store,
        sender.as_str(),
        "ghost@example.test",
        "openpgp",
        &ghost_fp,
        "verified",
        None,
    )
    .unwrap();
    app.dispatch(Intent::Outbox(crate::OutboxIntent::SendNow {
        queued: crate::reference::QueuedRef::from_parts(
            failed_rows[0].queued.account.as_str(),
            failed_rows[0].queued.op,
        )
        .expect("the row's key routes"),
    }))
    .await
    .unwrap();
    let op = engine
        .outbox(&sender)
        .await
        .expect("the engine outbox reads")
        .into_iter()
        .find(|op| op.state == engine_api::PendingOpState::Failed)
        .expect("the retried op is recorded");
    let retried = op.detail.clone().unwrap_or_default();
    assert!(
        !retried.contains("not every recipient can be encrypted to"),
        "the retry cleared the gate: {retried}"
    );
    assert!(
        retried.contains("SMTP"),
        "the retry re-wrapped and reached the transport, which refused the unknown user: {retried}"
    );

    // The cleanup: the failed op withdraws, and the proof mail leaves both
    // accounts through the engine's own delete path.
    let rows = app.sending().rows;
    if let Some(row) = rows.first() {
        app.dispatch(Intent::Outbox(crate::OutboxIntent::Cancel {
            queued: crate::reference::QueuedRef::from_parts(
                row.queued.account.as_str(),
                row.queued.op,
            )
            .expect("the row's key routes"),
        }))
        .await
        .unwrap();
    }
    proof::delete_subject_mail(&app, &sender, &["..."]).await;
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
