// SPDX-License-Identifier: GPL-3.0-only
//! The gated S/MIME receive proof against the live harness: astra1 sends an
//! S/MIME encrypted+signed mail to astra2 through the engine's own outbox (the
//! send proof's wrapped dial); the built-in crypto decoder over the recipient's
//! seeded CA chain opens the delivered row to the words, the chain-verified
//! banner, the restored subject, and a `message_crypto_results` row, with no
//! ciphertext in the legs or chips. Three negative arms: a keyless store (no
//! key), a store with no trust anchor (Invalid), and a second send whose leaf
//! SAN does not match the From address (Mismatch). Every copy is withdrawn.
//! The dials mirror the send proof: one accepted self-signed certificate.

use std::sync::Arc;

use astra_crypto_core::{CryptoBackend, CryptoMethod, CryptoPolicy};
use astra_crypto_smime::testing::{
    build_intermediate_signed_by, build_leaf_signed_by, build_self_signed_ca, BuiltCert,
};
use astra_crypto_smime::{fingerprint_of_cert_der, SmimeBackend};
use astra_crypto_store::{
    put_trust_decision, set_default_signing_key, upsert_crypto_key, upsert_intermediate_cert,
    CryptoKeyRecord, CryptoKeyRow, CryptoStore, SqliteKeyStore,
};
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
const SUBJECT: &str = "Harness S/MIME receive proof";
/// The mismatch arm's subject: a second send whose leaf SAN ≠ From.
const MISMATCH_SUBJECT: &str = "Harness S/MIME mismatch proof";
/// The words the encrypted body carries; the reading snapshot asserts them.
const WORDS: &str = "Sealed for the S/MIME reading pipeline, opened by the built-in decoder.";

/// Wrap DER as a single PEM block (base64 body, 64-col wrapped).
fn pem_block(label: &str, der: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut out = format!("-----BEGIN {label}-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).unwrap());
        out.push('\n');
    }
    out.push_str("-----END ");
    out.push_str(label);
    out.push_str("-----\n");
    out
}

/// Import a cert + private key (PEM bundle) into an account, returning its
/// fingerprint. Mirrors the p12/PEM import the key manager performs.
async fn import_identity(store: &Arc<CryptoStore>, account: &str, cert: &BuiltCert) -> String {
    let backend = SmimeBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), account)),
        CryptoPolicy::default(),
    );
    let pem = format!(
        "{}{}",
        pem_block("CERTIFICATE", &cert.cert_der),
        pem_block("PRIVATE KEY", &cert.priv_pkcs8_der),
    );
    backend
        .import_key(pem.as_bytes(), None)
        .await
        .expect("import identity");
    fingerprint_of_cert_der(&cert.cert_der).unwrap()
}

/// Insert a public-only cert row (a contact / trust anchor).
fn insert_public(
    store: &Arc<CryptoStore>,
    account: &str,
    email: Option<&str>,
    der: &[u8],
    origin: &str,
) {
    let fp = fingerprint_of_cert_der(der).unwrap();
    upsert_crypto_key(
        store,
        &CryptoKeyRecord {
            row: CryptoKeyRow {
                account_id: account.to_string(),
                standard: "smime".to_string(),
                key_type: "cert".to_string(),
                email: email.map(str::to_string),
                fingerprint: fp,
                origin: origin.to_string(),
                ..Default::default()
            },
            public_data: hex::encode(der),
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

/// Open the main delivered mail through a fresh recipient app over `store`,
/// answering the reading snapshot. The negative arms reuse it.
async fn open_main_with(
    store: Arc<CryptoStore>,
    host: &str,
    port: &str,
    password: &str,
) -> astra_viewmodel::ReadingSnapshot {
    let (recipient, providers, _, _) =
        dial_with_accepted_certificate(host, port, proof::RECIPIENT, password).await;
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![Account {
            id: recipient.clone(),
            providers,
            identity: EmailAddress::new(proof::RECIPIENT),
        }],
        None,
        observer,
    )
    .with_decoder(Some(Arc::new(CryptoDecoder::new(store))));
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox =
        crate::FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient)).unwrap();
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let key = row_key(row_by_subject(&app.mailbox_list(), "..."));
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

/// The gated run: the delivered sealed mail opens through the reading pipeline
/// to the words, the chain-verified banner, and the restored subject; the
/// keyless / untrusted-anchor / From-SAN-mismatch arms answer their banners.
#[tokio::test]
async fn the_smime_receive_opens_the_delivered_mail_through_the_reading_pipeline() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);

    // The CA chain: root → intermediate → sender leaf (SAN = From). The
    // recipient leaf is signed directly by the root so the send gate's
    // recipient-cert validation links it with no intermediate.
    let root = build_self_signed_ca("CN=Harness Root");
    let inter = build_intermediate_signed_by("CN=Harness Inter", &root);
    let leaf = build_leaf_signed_by("astra1@example.test", &inter);
    let recipient = build_leaf_signed_by(proof::RECIPIENT, &root);
    let leaf_fp = fingerprint_of_cert_der(&leaf.cert_der).unwrap();
    let recip_fp = fingerprint_of_cert_der(&recipient.cert_der).unwrap();

    // The crypto store first (the boot's ordering): both accounts' identities.
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

    // The sender: its signing leaf + the recipient cert it encrypts to.
    import_identity(&store, sender_id.as_str(), &leaf).await;
    set_default_signing_key(&store, sender_id.as_str(), "smime", &leaf_fp).unwrap();
    insert_public(
        &store,
        sender_id.as_str(),
        Some(proof::RECIPIENT),
        &recipient.cert_der,
        "contact",
    );
    insert_public(&store, sender_id.as_str(), None, &root.cert_der, "imported");
    put_trust_decision(
        &store,
        sender_id.as_str(),
        proof::RECIPIENT,
        "smime",
        &recip_fp,
        "verified",
        None,
    )
    .unwrap();

    // The recipient: its decrypt identity + the root anchor + intermediate.
    import_identity(&store, recipient_id.as_str(), &recipient).await;
    insert_public(
        &store,
        recipient_id.as_str(),
        None,
        &root.cert_der,
        "imported",
    );
    upsert_intermediate_cert(&store, recipient_id.as_str(), &inter.cert_der).unwrap();
    put_trust_decision(
        &store,
        recipient_id.as_str(),
        "astra1@example.test",
        "smime",
        &leaf_fp,
        "verified",
        None,
    )
    .unwrap();

    // The send proof's front half: the wrapped sender dial, the app over both.
    let wrapper = astra_crypto_store::mime_wrapper_for(
        &store,
        sender_id.as_str(),
        "astra1@example.test",
        CryptoMethod::Smime,
    )
    .expect("a crypto method installs a wrapper");
    let (sender, sender_providers) = dial_wrapped(&host, &port, &user, &password, wrapper).await;
    assert_eq!(sender, sender_id, "the dial answers the config-derived id");
    let (recipient_acct, recipient_providers, _, _) =
        dial_with_accepted_certificate(&host, &port, proof::RECIPIENT, &password).await;
    assert_eq!(recipient_acct, recipient_id);
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
                id: recipient_acct.clone(),
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
    proof::delete_subject_mail(&app, &sender, &[SUBJECT, MISMATCH_SUBJECT, "..."]).await;
    proof::delete_subject_mail(&app, &recipient_acct, &[SUBJECT, MISMATCH_SUBJECT, "..."]).await;
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
        "a delivered send leaves nothing in the queue"
    );

    // The delivery: the recipient's INBOX holds the sealed mail.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox =
        crate::FolderRef::from_parts(recipient_acct.as_str(), inbox_key(&app, &recipient_acct))
            .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder {
        folder: inbox.clone(),
    })
    .await
    .unwrap();
    let key = row_key(row_by_subject(&app.mailbox_list(), "..."));

    // The receive: the product's own open, the built-in decoder answering.
    app.dispatch(Intent::OpenMessage {
        message: crate::reference::MessageRef {
            account: recipient_acct.clone(),
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
        .expect("the chain-verified banner rides the snapshot");
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
            !leg.contains("BEGIN CERTIFICATE") && !leg.contains("smime.p7m"),
            "the ciphertext never renders: {leg}"
        );
    }
    assert!(
        !reading
            .attachments
            .iter()
            .any(|chip| chip.file_name.eq_ignore_ascii_case("smime.p7m")),
        "the ciphertext chip hides: {:?}",
        reading.attachments
    );
    let result = astra_crypto_store::get_message_crypto_result(
        &store,
        recipient_acct.as_str(),
        key.as_str(),
    )
    .expect("the result row lands keyed by the account and provider key");
    assert_eq!(result.crypto_kind, "encrypted-signed");
    assert_eq!(result.decrypt_state, "ok");
    assert_eq!(result.signature_state, "valid-verified");
    assert_eq!(result.standard.as_deref(), Some("smime"));
    assert_eq!(result.chain_valid, Some(true));
    assert_eq!(result.signer_fingerprint.as_deref(), Some(leaf_fp.as_str()));

    // The no-key arm: a keyless store over the same mail. Banner alone.
    let keyless_store = Arc::new(CryptoStore::open_in_memory().expect("the keyless store opens"));
    keyless_store
        .register_account(recipient_id.as_str(), proof::RECIPIENT)
        .unwrap();
    let closed = Box::pin(open_main_with(
        keyless_store.clone(),
        &host,
        &port,
        &password,
    ))
    .await;
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
        recipient_id.as_str(),
        key.as_str(),
    )
    .expect("the keyless verdict lands its row");
    assert_eq!(closed_result.decrypt_state, "no-key");

    // The untrusted-anchor arm: the recipient key but no root anchor. Invalid.
    let untrusted_store =
        Arc::new(CryptoStore::open_in_memory().expect("the untrusted store opens"));
    untrusted_store
        .register_account(recipient_id.as_str(), proof::RECIPIENT)
        .unwrap();
    import_identity(&untrusted_store, recipient_id.as_str(), &recipient).await;
    upsert_intermediate_cert(&untrusted_store, recipient_id.as_str(), &inter.cert_der).unwrap();
    let invalid = Box::pin(open_main_with(untrusted_store, &host, &port, &password)).await;
    let invalid_banner = invalid
        .security_banner
        .as_ref()
        .expect("the invalid banner rides the snapshot");
    assert_eq!(
        invalid_banner.level,
        astra_viewmodel::SecurityLevelView::Warning
    );
    assert!(
        invalid_banner.text.contains("signature invalid"),
        "the banner states the invalid chain: {}",
        invalid_banner.text
    );
    assert!(
        invalid
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "an untrusted chain still decrypts the words"
    );

    // The From-SAN mismatch arm: a second send whose leaf SAN ≠ From.
    let mismatch_leaf = build_leaf_signed_by("other@example.test", &inter);
    let mismatch_fp = import_identity(&store, sender_id.as_str(), &mismatch_leaf).await;
    set_default_signing_key(&store, sender_id.as_str(), "smime", &mismatch_fp).unwrap();
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: MISMATCH_SUBJECT.to_owned(),
        document: document(WORDS),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
        composition: None,
    })
    .await
    .unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    // Both proof mails carry the outer "..." placeholder; the mismatch mail is
    // the placeholder row that is not the main mail's key (order-independent).
    let mismatch_key = app
        .mailbox_list()
        .rows
        .iter()
        .find(|row| row.subject == "..." && row_key(row) != key)
        .map(row_key)
        .expect("the mismatch mail is the other placeholder row");
    app.dispatch(Intent::OpenMessage {
        message: crate::reference::MessageRef {
            account: recipient_acct.clone(),
            key: mismatch_key,
        },
    })
    .await
    .unwrap();
    let mismatch = app
        .reading()
        .expect("the mismatch open publishes a reading snapshot");
    let mismatch_banner = mismatch
        .security_banner
        .as_ref()
        .expect("the mismatch banner rides the snapshot");
    assert_eq!(
        mismatch_banner.level,
        astra_viewmodel::SecurityLevelView::Warning
    );
    assert!(
        mismatch_banner.text.contains("From address does not match"),
        "the banner states the From-SAN mismatch: {}",
        mismatch_banner.text
    );

    // The cleanup: every proof mail leaves both accounts.
    proof::delete_subject_mail(&app, &sender, &[MISMATCH_SUBJECT, "..."]).await;
    proof::delete_subject_mail(&app, &recipient_acct, &[SUBJECT, MISMATCH_SUBJECT, "..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
