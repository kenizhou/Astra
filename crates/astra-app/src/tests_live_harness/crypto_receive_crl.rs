// SPDX-License-Identifier: GPL-3.0-only
//! The gated S/MIME CRL-fetching receive proof (SP4c-3b D3): astra1 sends an
//! S/MIME encrypted+signed mail whose signer leaf carries a CRL distribution
//! point naming a test-local HTTP server on 127.0.0.1 (a TcpListener thread,
//! NOT a proxy) serving a CRL revoking the sender leaf's serial. The recipient
//! opens it through a CryptoDecoder wired to a bounded fetcher: the chain
//! hard-fails the revoked signer, the banner names the revocation. The negative
//! arm's leaf DP names a closed port: the fetch fails, revocation reads
//! unchecked, the open still succeeds. Every copy is withdrawn.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use astra_crypto_core::{CryptoBackend, CryptoMethod, CryptoPolicy};
use astra_crypto_smime::testing::{
    build_crl, build_leaf_signed_by, build_leaf_signed_by_with_dp, build_self_signed_ca, BuiltCert,
};
use astra_crypto_smime::{fingerprint_of_cert_der, SmimeBackend};
use astra_crypto_store::{
    crl_metadata_of_der, put_trust_decision, set_default_signing_key, upsert_crypto_key,
    CrlFetcher, CryptoKeyRecord, CryptoKeyRow, CryptoStore, FetchedCrl, SqliteKeyStore,
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

/// The sender's address, the first harness user.
const SENDER: &str = "astra1@example.test";
/// The subject the revoked arm sends: unique among the harness fixtures.
const SUBJECT: &str = "Harness S/MIME CRL revoked proof";
/// The fetch-failure arm's subject: a second send whose leaf DP is unreachable.
const NEG_SUBJECT: &str = "Harness S/MIME CRL unchecked proof";
/// The words the encrypted body carries; the reading snapshot asserts them.
const WORDS: &str = "Sealed for the CRL fetching proof, opened by the built-in decoder.";

/// A test-local HTTP server serving the CRL DER on the loopback port it binds
/// (an empty holder answers 404). NOT a proxy; the ephemeral port never collides.
fn serve_crl(crl: Arc<Mutex<Vec<u8>>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("the loopback CRL server binds");
    let port = listener.local_addr().expect("the bound port").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let body = crl.lock().unwrap().clone();
            let head = if body.is_empty() {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_owned()
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/pkix-crl\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
            };
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
    port
}

/// The bounded fetcher the proof wires into the decoder: the host
/// `HttpCrlFetcher`'s shape. Any failure returns `Err`, degrading to unchecked.
#[derive(Debug)]
struct LocalCrlFetcher {
    client: reqwest::Client,
}

impl LocalCrlFetcher {
    fn new() -> Self {
        let tls = engine_tls::client_config(&engine_tls::TlsPolicy::bundled_and_system())
            .expect("the CRL TLS config");
        let client = tls
            .reqwest_builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(5))
            .build()
            .expect("the CRL HTTP client");
        Self { client }
    }
}

#[async_trait::async_trait]
impl CrlFetcher for LocalCrlFetcher {
    async fn fetch(&self, crl_url: &str) -> Result<FetchedCrl, String> {
        let url = url::Url::parse(crl_url).map_err(|e| e.to_string())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("CRL URL is not http(s)".to_string());
        }
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("CRL fetch status {}", response.status()));
        }
        let crl_der = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
        let meta = crl_metadata_of_der(&crl_der).map_err(|e| e.to_string())?;
        Ok(FetchedCrl {
            crl_der,
            issuer_dn: meta.issuer_dn,
            next_update: meta.next_update,
        })
    }
}

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

/// Import a cert + private key (PEM bundle) into an account, answering its fp.
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

/// Open one key through a fresh recipient app over `store` (empty CRL cache).
async fn open_key_with(
    store: Arc<CryptoStore>,
    fetcher: Arc<dyn CrlFetcher>,
    host: &str,
    port: &str,
    password: &str,
    key: engine_api::ProviderKey,
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
    .with_decoder(Some(Arc::new(CryptoDecoder::with_crl_fetcher(
        store, fetcher,
    ))));
    app.dispatch(Intent::RefreshMail).await.unwrap();
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

/// The gated run: the fetched CRL revokes the signer (chain invalid, the
/// revoked banner); an unreachable DP reads unchecked, the open still succeeds.
#[tokio::test]
async fn the_smime_receive_checks_revocation_through_a_fetched_crl() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);

    // The test-local CRL server first: the sender leaf's distribution point
    // names its loopback port, so the port must exist before the leaf is built.
    let crl_bytes = Arc::new(Mutex::new(Vec::new()));
    let crl_port = serve_crl(crl_bytes.clone());
    let dp_url = format!("http://127.0.0.1:{crl_port}/crl.der");

    // The chain: a root anchor, the sender leaf with the DP, the recipient leaf.
    let root = build_self_signed_ca("CN=Harness CRL Root");
    let leaf = build_leaf_signed_by_with_dp(SENDER, &root, &[&dp_url]);
    let recipient = build_leaf_signed_by(proof::RECIPIENT, &root);
    let leaf_fp = fingerprint_of_cert_der(&leaf.cert_der).unwrap();
    let recip_fp = fingerprint_of_cert_der(&recipient.cert_der).unwrap();

    // The CRL revoking the sender leaf's serial, signed by the root (cRLSign).
    *crl_bytes.lock().unwrap() = build_crl(&root, &[&leaf]);

    // The crypto store first (the boot's ordering): both accounts' identities.
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

    // The recipient: its decrypt identity + the root anchor + the sender trust.
    import_identity(&store, recipient_id.as_str(), &recipient).await;
    insert_public(
        &store,
        recipient_id.as_str(),
        None,
        &root.cert_der,
        "imported",
    );
    put_trust_decision(
        &store,
        recipient_id.as_str(),
        SENDER,
        "smime",
        &leaf_fp,
        "verified",
        None,
    )
    .unwrap();

    // The wrapped sender dial + the recipient dial, the decoder on the fetcher.
    let wrapper = astra_crypto_store::mime_wrapper_for(
        &store,
        sender_id.as_str(),
        SENDER,
        CryptoMethod::Smime,
    )
    .expect("a crypto method installs a wrapper");
    let (sender, sender_providers) = dial_wrapped(&host, &port, &user, &password, wrapper).await;
    assert_eq!(sender, sender_id, "the dial answers the config-derived id");
    let (recipient_acct, recipient_providers, _, _) =
        dial_with_accepted_certificate(&host, &port, proof::RECIPIENT, &password).await;
    assert_eq!(recipient_acct, recipient_id);
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let fetcher: Arc<dyn CrlFetcher> = Arc::new(LocalCrlFetcher::new());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![
            Account {
                id: sender.clone(),
                providers: sender_providers,
                identity: EmailAddress::new(SENDER),
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
    .with_decoder(Some(Arc::new(CryptoDecoder::with_crl_fetcher(
        store.clone(),
        fetcher.clone(),
    ))));

    // A known state: sweep any earlier run's proof mail first.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[SUBJECT, NEG_SUBJECT, "..."]).await;
    proof::delete_subject_mail(&app, &recipient_acct, &[SUBJECT, NEG_SUBJECT, "..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The send: the revoked sender seals a mail to the recipient.
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

    // The receive: the fetched CRL revokes the signer; the chain hard-fails.
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
        "the words render despite the revoked chain: {:?}",
        reading.plain
    );
    let banner = reading
        .security_banner
        .as_ref()
        .expect("the revoked banner rides the snapshot");
    assert_eq!(banner.level, astra_viewmodel::SecurityLevelView::Warning);
    assert!(
        banner.text.contains("certificate revoked"),
        "the banner names the revocation: {}",
        banner.text
    );
    let result = astra_crypto_store::get_message_crypto_result(
        &store,
        recipient_acct.as_str(),
        key.as_str(),
    )
    .expect("the result row lands keyed by the account and provider key");
    assert_eq!(result.decrypt_state, "ok");
    assert_eq!(result.signature_state, "invalid");
    assert_eq!(result.chain_valid, Some(false));
    assert_eq!(result.revocation_state, "revoked");

    // The fetch-failure negative: a second leaf whose DP names a closed port. A
    // fresh store starts empty; the fetch fails, revocation reads unchecked.
    let neg_leaf = build_leaf_signed_by_with_dp(SENDER, &root, &["http://127.0.0.1:1/crl.der"]);
    let neg_fp = import_identity(&store, sender_id.as_str(), &neg_leaf).await;
    set_default_signing_key(&store, sender_id.as_str(), "smime", &neg_fp).unwrap();
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: NEG_SUBJECT.to_owned(),
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
    // The negative mail is the placeholder row that is not the revoked key.
    let neg_key = app
        .mailbox_list()
        .rows
        .iter()
        .find(|row| row.subject == "..." && row_key(row) != key)
        .map(row_key)
        .expect("the negative mail is the other placeholder row");

    let neg_store = Arc::new(CryptoStore::open_in_memory().expect("the fresh store opens"));
    neg_store
        .register_account(recipient_id.as_str(), proof::RECIPIENT)
        .unwrap();
    import_identity(&neg_store, recipient_id.as_str(), &recipient).await;
    insert_public(
        &neg_store,
        recipient_id.as_str(),
        None,
        &root.cert_der,
        "imported",
    );
    let neg_reading = Box::pin(open_key_with(
        neg_store.clone(),
        fetcher.clone(),
        &host,
        &port,
        &password,
        neg_key.clone(),
    ))
    .await;
    assert!(
        neg_reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "a failed fetch still opens the words: {:?}",
        neg_reading.plain
    );
    let neg_result = astra_crypto_store::get_message_crypto_result(
        &neg_store,
        recipient_id.as_str(),
        neg_key.as_str(),
    )
    .expect("the negative verdict lands its row");
    assert_eq!(neg_result.decrypt_state, "ok");
    assert_eq!(neg_result.chain_valid, Some(true));
    assert_eq!(neg_result.revocation_state, "unchecked");

    // The cleanup: every proof mail leaves both accounts.
    proof::delete_subject_mail(&app, &sender, &[SUBJECT, NEG_SUBJECT, "..."]).await;
    proof::delete_subject_mail(&app, &recipient_acct, &[SUBJECT, NEG_SUBJECT, "..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
