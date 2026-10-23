// SPDX-License-Identifier: GPL-3.0-only
//! The gated crypto send proof against the live harness: the whole SP4b
//! chain end to end. astra1's account carries the `openpgp` method, so its
//! submit assembles, gates, and travels the Astra-side crypto outbox
//! instead of the engine's queue; the delivered message on the harness
//! proves the protected subject (`...` in the clear), the Sent copy is
//! filed by the bypass's own IMAP append, and the recipient's copy
//! decrypts back to the composer's words with astra2's key. A second send
//! to a recipient with no key is blocked by the trust gate with the held
//! row standing, and nothing ever reaches the server.
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

use crate::crypto::{CryptoFlags, CryptoTransport, CryptoTransportResolver};
use crate::tests_live_harness::CountingObserver;
use crate::{Account, App, AppObserver, Intent};

use super::{
    dial_with_accepted_certificate, harness, inbox_key,
    proof::{self, document},
    row_by_subject,
};

/// The subject the proof sends: unique among the harness fixtures.
const SUBJECT: &str = "Harness crypto send proof";

/// The words the encrypted body carries; the decrypt asserts them back.
const WORDS: &str = "Sealed by the built-in gate through the harness outbox.";

/// The harness resolver: the accepted configs (cert exceptions included),
/// with the sender's `[crypto] method` set, answering the app's port.
struct HarnessCryptoResolver {
    sender: astra_account::AccountConfig,
    recipient: astra_account::AccountConfig,
}

#[async_trait::async_trait]
impl CryptoTransportResolver for HarnessCryptoResolver {
    async fn crypto_method(&self, account: &AccountId) -> CryptoMethod {
        let config = [Some(&self.sender), Some(&self.recipient)]
            .into_iter()
            .flatten()
            .find(|config| config.account_id().ok().as_ref() == Some(account));
        config.map_or(CryptoMethod::None, |config| {
            config.crypto.as_ref().map_or(CryptoMethod::None, |crypto| {
                CryptoMethod::from_db_str(&crypto.method)
            })
        })
    }

    async fn crypto_transport(&self, account: &AccountId) -> Option<CryptoTransport> {
        let config = [Some(&self.sender), Some(&self.recipient)]
            .into_iter()
            .flatten()
            .find(|config| config.account_id().ok().as_ref() == Some(account))?;
        let method = config.crypto.as_ref().map_or(CryptoMethod::None, |crypto| {
            CryptoMethod::from_db_str(&crypto.method)
        });
        if method == CryptoMethod::None {
            return None;
        }
        let imap = config.imap.as_ref()?;
        let smtp = config.smtp.as_ref()?;
        let password = imap.password.as_ref()?.expose().to_owned();
        let connector = astra_account::account_tls(config).ok()?.connector();
        let security_of = |security| match security {
            astra_account::ConnectionSecurity::ImplicitTls => {
                astra_crypto_store::TransportSecurity::ImplicitTls
            }
            astra_account::ConnectionSecurity::StartTls => {
                astra_crypto_store::TransportSecurity::StartTls
            }
        };
        Some(CryptoTransport {
            method,
            account_email: imap.username.clone(),
            smtp: astra_crypto_store::SmtpDial {
                addr: smtp.addr.clone(),
                server_name: smtp.server_name.clone(),
                security: security_of(smtp.security),
                username: imap.username.clone(),
                password: password.clone(),
            },
            imap: astra_crypto_store::ImapDial {
                addr: imap.addr.clone(),
                server_name: imap.server_name.clone(),
                security: security_of(imap.security),
                username: imap.username.clone(),
                password,
            },
            connector,
        })
    }
}

/// The gated run: an encrypted send travels the crypto outbox and is
/// delivered sealed, the Sent copy is filed, and a missing-key recipient
/// is blocked with the held row standing.
#[tokio::test]
async fn the_crypto_send_travels_the_outbox_and_decrypts_for_the_recipient() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };

    // Dial both sides through the certificate acceptance walk, keeping the
    // accepted configs for the resolver and the verification fetch.
    let (sender, sender_providers, sender_config, _) =
        dial_with_accepted_certificate(&host, &port, &user, &password).await;
    let (recipient, recipient_providers, recipient_config, _) =
        dial_with_accepted_certificate(&host, &port, proof::RECIPIENT, &password).await;

    // The sender's method is openpgp; the recipient config stands for the
    // resolver's method lookup only (no crypto of its own this run).
    let mut sender_crypto_config = sender_config.clone();
    sender_crypto_config.crypto = Some(astra_account::CryptoAccount {
        method: "openpgp".to_owned(),
    });
    let resolver = HarnessCryptoResolver {
        sender: sender_crypto_config,
        recipient: recipient_config.clone(),
    };

    // The crypto store: the sender's own key (default signer) and the
    // recipient's public cert with an explicit verified decision.
    let store = Arc::new(CryptoStore::open_in_memory().expect("the in-memory store opens"));
    store
        .register_account(sender.as_str(), "astra1@example.test")
        .unwrap();
    let backend = astra_crypto_openpgp::OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), sender.as_str())),
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
        sender.as_str(),
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
                account_id: sender.as_str().to_owned(),
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
        sender.as_str(),
        proof::RECIPIENT,
        "openpgp",
        &recipient_fp,
        "verified",
        None,
    )
    .unwrap();

    // The app over the product's own dial, with the crypto support installed.
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
    .with_crypto_send(store.clone(), Arc::new(resolver));

    // A known state: sweep any earlier run's proof mail first.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[SUBJECT, "..."]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT, "..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The send: the composer's gesture. The crypto branch assembles, gates,
    // queues, and drains inside the dispatch.
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
        astra_crypto_store::list_crypto_outbox(&store)
            .expect("the outbox reads")
            .is_empty(),
        "the crypto outbox settled the row"
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

    // The Sent copy: filed by the bypass's own IMAP append, placeholder
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

    // The blocked half: a recipient with no key is refused by the trust
    // gate, the held row stands naming them, and nothing goes out.
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
    let held = app.sending().held;
    assert_eq!(held.len(), 1, "the held row stands");
    assert!(
        held[0].reason.contains("ghost@example.test"),
        "the row names the keyless recipient: {}",
        held[0].reason
    );
    assert!(
        astra_crypto_store::list_crypto_outbox(&store)
            .expect("the outbox reads")
            .is_empty(),
        "nothing was ever queued for the blocked send"
    );

    // The cleanup: the held row withdraws, and the proof mail leaves both
    // accounts through the engine's own delete path.
    let hold_id = held[0].hold.clone();
    app.dispatch(Intent::Outbox(crate::OutboxIntent::WithdrawHeld {
        hold: hold_id,
    }))
    .await
    .unwrap();
    proof::delete_subject_mail(&app, &sender, &["..."]).await;
    proof::delete_subject_mail(&app, &recipient, &["..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
