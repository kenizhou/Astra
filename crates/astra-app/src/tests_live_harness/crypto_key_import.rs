// SPDX-License-Identifier: GPL-3.0-only
//! The gated key import proof (SP4d-1): both keys arrive through the
//! product's own import surface (`App::import_key`), never through
//! programmatic upsert seeding. astra1 imports its own private key and the
//! recipient's public cert (the import records the unverified acceptance);
//! astra2 imports its own private key and the sender's public cert. astra1 then sends an
//! encrypted+signed mail through the engine's own outbox, and the built-in
//! crypto decoder opens the delivered row at astra2 to the composer's words,
//! the verified banner, and the restored protected subject. The proof pins
//! the has_private guard along the way: the peer's public import never flags
//! a private key, while each account's own import does. Every copy is
//! withdrawn at the end.
//!
//! The harness dials mirror the receive proof: the same accepted self-signed
//! certificate for IMAP and SMTP alike.

use std::sync::Arc;

use astra_crypto_core::CryptoMethod;
use astra_crypto_openpgp::keymap::{cert_to_armored_public, cert_to_secret_blob, fingerprint_of};
use astra_crypto_store::{get_latest_trust_decision, CryptoStore};
use engine_api::{EmailAddress, Engine};
use sequoia_openpgp::cert::prelude::*;

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
const SUBJECT: &str = "Harness key import proof";

/// The words the encrypted body carries; the reading snapshot asserts them.
const WORDS: &str = "Sealed for the key import proof, opened by the imported key.";

/// A fresh keypair a person holds in a key file: a primary, a transport
/// encryption subkey, and a signing subkey, with secret material present.
fn keypair(user_id: &str) -> sequoia_openpgp::Cert {
    let (cert, _rev) = CertBuilder::new()
        .add_userid(user_id)
        .add_transport_encryption_subkey()
        .add_signing_subkey()
        .generate()
        .expect("CertBuilder::generate");
    cert
}

/// The message key a flat list row carries, the ref an open addresses.
fn row_key(row: &astra_viewmodel::MessageRowView) -> engine_api::ProviderKey {
    let astra_viewmodel::RowKey::Message { key, .. } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    key
}

/// The gated run: keys imported through the product surface decrypt and
/// verify a delivered sealed mail; the public imports never flag a private
/// key. Every copy is withdrawn.
#[tokio::test]
async fn the_imported_keys_decrypt_and_verify_the_delivered_mail() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    // The fixed test master key keeps the proof off the OS keyring, the
    // offline crypto suite's own discipline.
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);

    // The key files the two people hold: generated here as fixtures, then
    // handed to the product's import surface (never upserted into the store).
    let sender_cert = keypair("Harness <astra1@example.test>");
    let recipient_cert = keypair(&format!("Harness <{}>", proof::RECIPIENT));
    let sender_fp = fingerprint_of(&sender_cert).as_str().to_owned();
    let recipient_fp = fingerprint_of(&recipient_cert).as_str().to_owned();
    let sender_tsk = cert_to_secret_blob(&sender_cert).expect("the sender's private blob");
    let recipient_tsk = cert_to_secret_blob(&recipient_cert).expect("the recipient's private blob");
    let sender_public = cert_to_armored_public(&sender_cert).expect("the sender's public armor");
    let recipient_public =
        cert_to_armored_public(&recipient_cert).expect("the recipient's public armor");

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

    // The send proof's front half: the wrapped sender dial (the wrapper reads
    // the store at drain time, so the keys may land after the dial), the
    // recipient's ordinary dial, the app over both with the built-in crypto
    // decoder installed over the shared store.
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
    app.install_crypto_store(Arc::clone(&store));

    // The import, through the product surface only. Each account's own
    // private key (the private path), then the peer's public cert (the
    // public path), which itself records the unverified acceptance the send
    // gate admits the encryption by.
    let sender_own = app
        .import_key(&sender, &sender_tsk, None)
        .await
        .expect("the sender's own private import lands");
    assert!(
        sender_own.has_private,
        "the own import records the private blob"
    );
    assert_eq!(sender_own.fingerprint, sender_fp);
    let recipient_own = app
        .import_key(&recipient, &recipient_tsk, None)
        .await
        .expect("the recipient's own private import lands");
    assert!(recipient_own.has_private);
    assert_eq!(recipient_own.fingerprint, recipient_fp);

    let sender_peer = app
        .import_key(&sender, &recipient_public, None)
        .await
        .expect("the recipient's public import lands on the sender");
    assert!(
        !sender_peer.has_private,
        "the has_private guard: a public import never flags a private key"
    );
    assert_eq!(sender_peer.key_type, "public");
    assert_eq!(sender_peer.origin, "imported");
    // The import surface itself records the acceptance: no programmatic trust
    // seeding. The default is unverified, which the send resolver treats as
    // encryption-qualified, so the send below is admitted by the import alone.
    let decision = get_latest_trust_decision(
        &store,
        sender.as_str(),
        proof::RECIPIENT,
        "openpgp",
        &recipient_fp,
    )
    .expect("the trust read runs")
    .expect("the public import recorded a decision");
    assert_eq!(decision.decision, "unverified");

    let recipient_peer = app
        .import_key(&recipient, &sender_public, None)
        .await
        .expect("the sender's public import lands on the recipient");
    assert!(
        !recipient_peer.has_private,
        "the has_private guard holds on the recipient too"
    );
    assert_eq!(recipient_peer.fingerprint, sender_fp);

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

    // The delivery: the recipient's INBOX holds the sealed mail.
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

    // The receive: the product's own open, the imported keys answering.
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

    // The cleanup: the proof mail leaves both accounts through the engine's
    // own delete path (the recipient's INBOX and the sender's Sent alike).
    proof::delete_subject_mail(&app, &sender, &["..."]).await;
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
