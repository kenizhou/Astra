// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the built-in crypto send path at the submit seam: the
//! zero-cost default (no support, no method) keeps the engine's queue; a
//! crypto send assembles, gates, queues in the Astra-side outbox, and its
//! rows and verbs ride the same sending surface under the crypto op
//! namespace.

use std::sync::{Arc, Mutex};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use astra_crypto_core::{CryptoBackend, CryptoMethod, CryptoPolicy, KeyGenParams, Standard};
use astra_crypto_store::CryptoStore;
use engine_api::AccountId;

use super::{CryptoTransport, CryptoTransportResolver};
use crate::{crypto::CryptoFlags, dispatch_fakes as fakes, App, Intent, OutboxIntent, Surface};

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

fn document(text: &str) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![Block::Paragraph(Paragraph {
            content: vec![InlineContent::Text(TextRun {
                text: text.to_owned(),
                bold: false,
                italic: false,
                underline: false,
                font_size: None,
                color: None,
                highlight: None,
                link: None,
            })],
        })],
        attachments: Vec::new(),
    }
}

fn send(bcc: &str) -> Intent {
    Intent::SubmitRichMail {
        from: account("work"),
        to: "ada@example.test".to_owned(),
        cc: String::new(),
        bcc: bcc.to_owned(),
        subject: "Quarterly report".to_owned(),
        document: document("Agreed"),
        blobs: Vec::new(),
        crypto: CryptoFlags::default(),
    }
}

/// A scripted resolver: one method and (optionally) one dial table with an
/// unreachable server, so a due row's drain always lands transient.
struct FakeResolver {
    method: CryptoMethod,
    with_transport: bool,
}

#[async_trait::async_trait]
impl CryptoTransportResolver for FakeResolver {
    async fn crypto_method(&self, _account: &AccountId) -> CryptoMethod {
        self.method
    }

    async fn crypto_transport(&self, _account: &AccountId) -> Option<CryptoTransport> {
        if !self.with_transport {
            return None;
        }
        let connector = engine_tls::client_config(&engine_tls::TlsPolicy::bundled())
            .expect("bundled policy builds")
            .connector();
        // Nothing listens on this port, so every submit is a transient
        // failure and the row stays queued for the verbs to act on.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        Some(CryptoTransport {
            method: self.method,
            account_email: "work@example.test".to_string(),
            smtp: astra_crypto_store::SmtpDial {
                addr: format!("127.0.0.1:{port}"),
                server_name: "localhost".to_string(),
                security: astra_crypto_store::TransportSecurity::ImplicitTls,
                username: "work@example.test".to_string(),
                password: "pw".to_string(),
            },
            imap: astra_crypto_store::ImapDial {
                addr: format!("127.0.0.1:{port}"),
                server_name: "localhost".to_string(),
                security: astra_crypto_store::TransportSecurity::ImplicitTls,
                username: "work@example.test".to_string(),
                password: "pw".to_string(),
            },
            connector,
        })
    }
}

fn test_store() -> Arc<CryptoStore> {
    Arc::new(CryptoStore::open_in_memory().expect("the in-memory store opens"))
}

fn app_with(
    surfaces: &Arc<Mutex<Vec<Surface>>>,
    store: Arc<CryptoStore>,
    resolver: FakeResolver,
) -> (App<fakes::FakeProvider>, Arc<Mutex<Vec<engine_api::Draft>>>) {
    let provider = fakes::FakeProvider::with_mail(vec![fakes::inbox()], vec![])
        .submitting(vec![fakes::ScriptedSend::Delivered]);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], surfaces)
        .with_crypto_send(store, Arc::new(resolver));
    (app, submits)
}

/// Seed the store with the account's own OpenPGP key (default signer) and a
/// verified recipient cert for `email`.
async fn seed_openpgp_parties(store: &Arc<CryptoStore>, account: &str, email: &str) {
    store
        .register_account(account, "work@example.test")
        .unwrap();
    let backend = astra_crypto_openpgp::OpenpgpBackend::new(
        Arc::new(astra_crypto_store::SqliteKeyStore::new(
            store.clone(),
            account,
        )),
        CryptoPolicy::default(),
    );
    let own = backend
        .generate_key(KeyGenParams {
            standard: Standard::OpenPgp,
            user_id: "Work <work@example.test>".to_string(),
            algorithm: "default".to_string(),
            passphrase: None,
        })
        .await
        .expect("generate own key");
    astra_crypto_store::set_default_signing_key(
        store,
        account,
        "openpgp",
        own.fingerprint.as_str(),
    )
    .unwrap();
    let cert = astra_crypto_openpgp::engine::generate(&format!("Ada <{email}>")).unwrap();
    let fp = astra_crypto_openpgp::keymap::fingerprint_of(&cert)
        .as_str()
        .to_string();
    astra_crypto_store::upsert_crypto_key(
        store,
        &astra_crypto_store::CryptoKeyRecord {
            row: astra_crypto_store::CryptoKeyRow {
                account_id: account.to_string(),
                standard: "openpgp".to_string(),
                key_type: "cert".to_string(),
                email: Some(email.to_string()),
                fingerprint: fp.clone(),
                origin: "contact".to_string(),
                ..Default::default()
            },
            public_data: hex::encode(
                astra_crypto_openpgp::keymap::cert_to_public_blob(&cert).unwrap(),
            ),
            private_data: None,
            policy_json: None,
        },
    )
    .unwrap();
    astra_crypto_store::put_trust_decision(store, account, email, "openpgp", &fp, "verified", None)
        .unwrap();
}

#[tokio::test]
async fn no_crypto_method_keeps_the_engine_path() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(
        &surfaces,
        test_store(),
        FakeResolver {
            method: CryptoMethod::None,
            with_transport: true,
        },
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(send("")).await.unwrap();
    assert_eq!(submits.lock().unwrap().len(), 1, "the engine sent it");
    assert!(app.sending().rows.is_empty(), "no crypto rows stand");
}

#[tokio::test]
async fn a_method_without_a_dial_table_holds_with_the_typed_unavailable_answer() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let (app, submits) = app_with(
        &surfaces,
        test_store(),
        FakeResolver {
            method: CryptoMethod::Openpgp,
            with_transport: false,
        },
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(send("")).await.unwrap();
    assert_eq!(submits.lock().unwrap().len(), 0, "never silently dropped");
    let held = app.sending().held;
    assert_eq!(held.len(), 1);
    assert!(held[0].reason.contains("no IMAP/SMTP submission settings"));
}

#[tokio::test]
async fn a_recipient_without_a_key_holds_the_send_naming_them() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let store = test_store();
    store.register_account("work", "work@example.test").unwrap();
    let backend = astra_crypto_openpgp::OpenpgpBackend::new(
        Arc::new(astra_crypto_store::SqliteKeyStore::new(
            store.clone(),
            "work",
        )),
        CryptoPolicy::default(),
    );
    let own = backend
        .generate_key(KeyGenParams {
            standard: Standard::OpenPgp,
            user_id: "Work <work@example.test>".to_string(),
            algorithm: "default".to_string(),
            passphrase: None,
        })
        .await
        .unwrap();
    astra_crypto_store::set_default_signing_key(
        &store,
        "work",
        "openpgp",
        own.fingerprint.as_str(),
    )
    .unwrap();
    let (app, submits) = app_with(
        &surfaces,
        store,
        FakeResolver {
            method: CryptoMethod::Openpgp,
            with_transport: true,
        },
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(send("")).await.unwrap();
    assert_eq!(submits.lock().unwrap().len(), 0, "no ciphertext, no send");
    let held = app.sending().held;
    assert_eq!(held.len(), 1);
    assert!(
        held[0].reason.contains("ada@example.test"),
        "the held row names the recipient: {}",
        held[0].reason
    );
    assert!(held[0].reason.contains("no-key"));
}

#[tokio::test]
async fn a_gated_send_queues_in_the_crypto_outbox_and_rides_the_sending_surface() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let store = test_store();
    seed_openpgp_parties(&store, "work", "ada@example.test").await;
    seed_openpgp_parties(&store, "work", "hidden@example.test").await;
    let (app, submits) = app_with(
        &surfaces,
        store.clone(),
        FakeResolver {
            method: CryptoMethod::Openpgp,
            with_transport: true,
        },
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(send("hidden@example.test")).await.unwrap();

    assert_eq!(
        submits.lock().unwrap().len(),
        0,
        "the engine's queue never sees a crypto send"
    );
    let rows = app.sending().rows;
    assert_eq!(rows.len(), 1, "the crypto row stands in the same panel");
    assert!(
        rows[0].queued.op & crate::crypto_outbox::CRYPTO_OP_NAMESPACE != 0,
        "the row rides the crypto op namespace"
    );

    // The queued message: the wire variant carries no Bcc header, the filed
    // variant carries it, and the ciphertext is one and the same.
    let stored = astra_crypto_store::list_crypto_outbox(&store).unwrap();
    assert_eq!(stored.len(), 1);
    let wire = String::from_utf8_lossy(&stored[0].wire_bytes).to_string();
    let filed = String::from_utf8_lossy(&stored[0].filed_bytes).to_string();
    assert!(
        !wire.contains("Bcc:"),
        "the wire hides the Bcc list: {wire}"
    );
    assert!(
        filed.contains("Bcc: hidden@example.test"),
        "the filed copy records it"
    );
    assert_eq!(
        stored[0].state, "queued",
        "the unreachable server leaves it queued"
    );
    assert_eq!(
        stored[0].env_to.len(),
        2,
        "the envelope reaches every recipient"
    );

    // Withdraw: the row leaves and nothing ever goes out.
    let queued = app.sending().rows[0].queued.clone();
    app.dispatch(Intent::Outbox(OutboxIntent::Cancel {
        queued: crate::reference::QueuedRef {
            account: queued.account.clone(),
            op: engine_api::PendingOpId::new(queued.op),
        },
    }))
    .await
    .unwrap();
    assert!(app.sending().rows.is_empty());
    assert!(astra_crypto_store::list_crypto_outbox(&store)
        .unwrap()
        .is_empty());
}

#[test]
fn strip_bcc_handles_folding_and_leaves_body_lines_alone() {
    let wrapped =
        b"From: a@x\r\nBcc: one@x,\r\n two@x\r\nSubject: hi\r\n\r\nBcc: not a header\r\nbody";
    let out = super::strip_bcc_header(wrapped);
    let text = String::from_utf8(out).unwrap();
    assert_eq!(
        text, "From: a@x\r\nSubject: hi\r\n\r\nBcc: not a header\r\nbody",
        "the header and its folded continuation go; a body line starting Bcc: is content"
    );
}

#[test]
fn crypto_row_of_names_only_the_namespace() {
    assert!(App::<fakes::FakeProvider>::crypto_row_of(engine_api::PendingOpId::new(42)).is_none());
    let row = App::<fakes::FakeProvider>::crypto_row_of(engine_api::PendingOpId::new(
        crate::crypto_outbox::CRYPTO_OP_NAMESPACE | 0x2a,
    ));
    assert_eq!(row, Some(42));
}

#[test]
fn flags_default_to_sign_and_encrypt() {
    assert_eq!(CryptoFlags::default().resolved(), (true, true));
    assert_eq!(
        CryptoFlags {
            sign: Some(false),
            encrypt: None,
        }
        .resolved(),
        (false, true)
    );
}
