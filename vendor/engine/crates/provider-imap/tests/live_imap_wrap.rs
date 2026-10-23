//! Gated live integration: the MIME-wrap seam against the Stalwart harness.
//!
//! Installs a scripted `MimeWrapper` on the account, submits a draft that asks
//! (`sign`/`encrypt`), and reads the filed copy back from the real IMAP server:
//! it must be the wrapper's output — ciphertext with the `Bcc` header kept, no
//! plaintext. A control send through the same account with the flags unset
//! must file plaintext (the pre-seam byte shape), proving the difference is
//! the seam and not the server. Skips with no `STALWART_IMAP_ADDR`, so the
//! offline `cargo test --workspace` stays green.
//!
//! **Why a real server is not optional.** The offline suite proves what the
//! seam assembles; it cannot prove that a real MTA accepts the wrapped `DATA`
//! or that a real IMAP server round-trips those bytes unchanged.
//!
//! ## What this does not prove, and why
//!
//! The DELIVERED variant is read nowhere: this Stalwart fixture accepts a
//! locally-addressed message (`250 … queued`) and never delivers it to the
//! other account's INBOX (the limitation `live_imip.rs` documents, verified
//! there with a plain control message). The wire variant's relationship to
//! the filed one — same ciphertext minus the `Bcc` header — is the offline
//! suite's to pin; the live half here is that a real server accepted and
//! round-tripped what the seam produced.

use std::sync::{Arc, Mutex};

use engine_core::{
    ids::{AccountId, MailboxId, MessageIdHeader},
    mail::{EmailAddress, MailboxRole, Message},
    sync::SyncUpdate,
};
use engine_provider::{Draft, MimeWrapOutcome, MimeWrapper, Provider, ProviderError};
use provider_imap::{Credentials, ImapAccount, ImapConfig, ImapProvider};
use stalwart_harness::Harness;
use tokio_rustls::{TlsConnector, client::TlsStream};

/// Builds a TLS connector that accepts the harness's self-signed certificate.
/// Test-only and deliberately insecure; it never touches a host trust store.
fn no_verify_connector() -> TlsConnector {
    engine_tls::TlsClientConfig::dangerous_accept_any().connector()
}

/// The scripted wrapper: seals the body to `CIPHERTEXT`, derives the wire
/// variant by stripping the `Bcc` header — the contract a crypto wrapper
/// honours.
struct Seal {
    seen: Mutex<Vec<Vec<u8>>>,
}

#[async_trait::async_trait]
impl MimeWrapper for Seal {
    async fn wrap(
        &self,
        _draft: &Draft,
        filed_message: Vec<u8>,
    ) -> Result<MimeWrapOutcome, ProviderError> {
        self.seen
            .lock()
            .expect("seal lock")
            .push(filed_message.clone());
        let filed = String::from_utf8(filed_message)
            .expect("utf-8")
            .replace(PLAINTEXT_BODY, "CIPHERTEXT");
        let mut wire = String::new();
        let mut in_bcc = false;
        for line in filed.split("\r\n") {
            let name = line.split(':').next().unwrap_or("").to_ascii_lowercase();
            if !line.starts_with(' ') && !line.starts_with('\t') {
                in_bcc = name == "bcc";
            }
            if !in_bcc {
                wire.push_str(line);
                wire.push_str("\r\n");
            }
        }
        Ok(MimeWrapOutcome {
            wire: wire.into_bytes(),
            filed: filed.into_bytes(),
        })
    }
}

const PLAINTEXT_BODY: &str = "the live wrap secret body";

/// Connects an account with the wrapper installed and SMTP submission enabled,
/// returning the provider bound to the real Sent folder and the wrapper.
async fn connect_wrapped(
    harness: &Harness,
    sent_mailbox: &str,
) -> (ImapProvider<TlsStream<tokio::net::TcpStream>>, Arc<Seal>) {
    let host = harness
        .imap_addr
        .rsplit_once(':')
        .map_or("localhost", |(host, _)| host);
    let seal = Arc::new(Seal {
        seen: Mutex::new(Vec::new()),
    });
    let config = ImapConfig::new(
        harness.imap_addr.as_str(),
        host,
        Credentials::password(harness.account.as_str(), harness.password.as_str()),
    )
    .with_smtp(harness.smtp_addr.as_str())
    .with_mime_wrapper(seal.clone());
    let account = ImapAccount::connect(&config, no_verify_connector())
        .await
        .expect("connect IMAP+SMTP");
    (
        account.provider(MailboxId::try_from(sent_mailbox).unwrap()),
        seal,
    )
}

/// Resolves the account's real folder for `role` (Stalwart's Sent is "Sent
/// Items"), falling back to `default_name`.
async fn resolve_role_mailbox(harness: &Harness, role: MailboxRole, default_name: &str) -> String {
    let host = harness
        .imap_addr
        .rsplit_once(':')
        .map_or("localhost", |(host, _)| host);
    let config = ImapConfig::new(
        harness.imap_addr.as_str(),
        host,
        Credentials::password(harness.account.as_str(), harness.password.as_str()),
    );
    let account = AccountId::try_from("imap-wrap-resolve").unwrap();
    let provider = ImapAccount::connect(&config, no_verify_connector())
        .await
        .map(|account| account.provider(MailboxId::try_from("INBOX").unwrap()))
        .expect("connect IMAP");
    let sync = provider
        .sync_mailboxes(&account, None)
        .await
        .expect("list folders");
    let SyncUpdate::Snapshot { objects, .. } = sync.update else {
        return default_name.to_owned();
    };
    objects
        .into_iter()
        .find(|mailbox| mailbox.role.as_ref() == Some(&role))
        .map_or_else(|| default_name.to_owned(), |mailbox| mailbox.name)
}

/// The raw source of the one message in `provider`'s mailbox whose envelope
/// names `message_id`.
async fn source_of<S>(provider: &ImapProvider<S>, account: &AccountId, message_id: &str) -> String
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
{
    let sync = provider
        .sync_email(account, None)
        .await
        .expect("sync mailbox");
    let SyncUpdate::Snapshot { objects, .. } = sync.update else {
        panic!("a cursorless sync is a snapshot");
    };
    let message: Message = objects
        .into_iter()
        .find(|m| {
            m.envelope
                .message_id
                .iter()
                .any(|id| id.as_str() == message_id)
        })
        .unwrap_or_else(|| panic!("{message_id} is in the mailbox"));
    let raw = provider
        .fetch_message_source(account, &message)
        .await
        .expect("fetch source");
    String::from_utf8(raw.as_bytes().to_vec()).expect("utf-8 source")
}

fn draft(harness: &Harness, message_id: &str, ask: bool) -> Draft {
    let draft = Draft::new(
        MessageIdHeader::new(message_id).unwrap(),
        EmailAddress::new(harness.account.as_str()),
        vec![EmailAddress::new("bob@test.local")],
        "the live wrap probe",
        PLAINTEXT_BODY,
    )
    .with_bcc(vec![EmailAddress::new("carol@test.local")]);
    if ask {
        draft.with_crypto(true, true)
    } else {
        draft
    }
}

/// The seam on a real server: a wrapped send is accepted by the real MTA and
/// its Sent copy round-trips as the wrapper's output (ciphertext, `Bcc` kept,
/// no plaintext); a flags-unset send through the same wrapped account files
/// plaintext — the control arm that proves the seam, not the server, made the
/// difference.
#[tokio::test]
async fn live_wrapped_send_files_the_wrapped_copy_and_the_control_files_plaintext() {
    let Some(harness) = Harness::from_env() else {
        eprintln!(
            "skipping live_wrapped_send_files_the_wrapped_copy_and_the_control_files_plaintext: STALWART_IMAP_ADDR unset"
        );
        return;
    };
    harness
        .wait_until_ready(std::time::Duration::from_secs(30))
        .expect("harness ready");

    let sent_mailbox = resolve_role_mailbox(&harness, MailboxRole::Sent, "Sent").await;
    let (sent, seal) = connect_wrapped(&harness, &sent_mailbox).await;
    let account = AccountId::try_from("imap-live-wrap").unwrap();

    // The harness is shared and Stalwart de-duplicates a filed copy by
    // Message-ID, so each run mints fresh ones — a stale copy from an earlier
    // run must never stand in for this one.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after the epoch")
        .as_millis();
    // --- The wrapped send: accepted by the real MTA, filed wrapped ---
    let wrapped_id = format!("live-wrap-wrapped-{nonce}@test.local");
    let wrapped_id = wrapped_id.as_str();
    sent.submit_email(
        &account,
        &draft(&harness, wrapped_id, true),
        &engine_provider::HandOver::new(&engine_provider::Unrecorded),
    )
    .await
    .expect("the wrapped send delivers");
    assert_eq!(
        seal.seen.lock().expect("seal lock").len(),
        1,
        "the wrapper ran exactly once"
    );
    let filed = source_of(&sent, &account, wrapped_id).await;
    assert!(filed.contains("CIPHERTEXT"), "{filed}");
    assert!(
        filed.contains("Bcc: carol@test.local"),
        "the filed copy records the blind list: {filed}"
    );
    assert!(
        !filed.contains(PLAINTEXT_BODY),
        "Sent never holds the plaintext of a wrapped send: {filed}"
    );

    // --- The control arm: same account, same wrapper, flags unset ---
    let plain_id = format!("live-wrap-plain-{nonce}@test.local");
    let plain_id = plain_id.as_str();
    sent.submit_email(
        &account,
        &draft(&harness, plain_id, false),
        &engine_provider::HandOver::new(&engine_provider::Unrecorded),
    )
    .await
    .expect("the plain send delivers");
    assert_eq!(
        seal.seen.lock().expect("seal lock").len(),
        1,
        "the wrapper is NOT consulted when the draft does not ask"
    );
    let filed = source_of(&sent, &account, plain_id).await;
    assert!(
        filed.contains(PLAINTEXT_BODY),
        "an unflagged send files byte-for-byte plain: {filed}"
    );
    assert!(
        filed.contains("Bcc: carol@test.local"),
        "the pre-seam shape keeps Bcc on the filed copy: {filed}"
    );
    assert!(!filed.contains("CIPHERTEXT"), "{filed}");
}
