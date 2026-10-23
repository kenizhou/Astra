//! The MIME-wrap seam end to end, over mock streams: an installed wrapper on the
//! `submit()` / `refile()` paths (the unit tests for `prepare_message` /
//! `filed_bytes` themselves live in the parent module).
//!
//! The `Seal` double stands in for a crypto wrapper: it replaces the body with
//! `CIPHERTEXT` and derives the wire variant by stripping the `Bcc` header — the
//! exact "same ciphertext minus Bcc" contract the real wrapper honours.

use std::sync::{Arc, Mutex};

use engine_core::{
    ids::{MailboxId, MessageIdHeader},
    mail::EmailAddress,
};
use engine_provider::{Draft, MimeWrapOutcome, MimeWrapper, ProviderError};

use super::prepare_submission;
use crate::{
    ImapAccount, ImapProvider,
    filing::SmtpSender,
    mock::{MockStream, script, written},
    transport::Connection,
};

const GREETING: &str = "* OK ready\r\n";
const LOGIN_OK: &str = "a1 OK LOGIN ok\r\n";

fn draft() -> Draft {
    Draft::new(
        MessageIdHeader::new("wrapped-send@host").unwrap(),
        EmailAddress::new("alice@test.local"),
        vec![EmailAddress::new("bob@test.local")],
        "Hi",
        "the secret body",
    )
}

/// The wrapper double: seals the body, derives the wire variant by stripping
/// `Bcc`, records what it was asked to wrap, and can be told to refuse.
struct Seal {
    seen: Mutex<Vec<Vec<u8>>>,
    refuse: bool,
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
        if self.refuse {
            return Err(ProviderError::permanent(
                "gate: no trusted key for a recipient",
            ));
        }
        let filed = String::from_utf8(filed_message)
            .expect("utf-8")
            .replace("the secret body", "CIPHERTEXT");
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

fn seal(refuse: bool) -> Arc<Seal> {
    Arc::new(Seal {
        seen: Mutex::new(Vec::new()),
        refuse,
    })
}

/// A provider over a greeted, logged-in mock IMAP connection, with `wrapper`
/// installed on its account (and no SMTP transport unless `smtp` says otherwise).
async fn wrapped_provider(
    server: Vec<u8>,
    smtp: Option<SmtpSender>,
    wrapper: Arc<Seal>,
) -> (ImapProvider<MockStream>, Arc<Seal>) {
    let (stream, _) = MockStream::new(server);
    let mut conn = Connection::open(stream).await.unwrap();
    conn.login("alice", "pw").await.unwrap();
    let account = ImapAccount::offline_wrapped(conn, smtp, Some(wrapper.clone()));
    (
        account.provider(MailboxId::try_from("INBOX").unwrap()),
        wrapper,
    )
}

/// The whole send: the wire variant is the sealed ciphertext without `Bcc`, the
/// filed Sent copy is the same ciphertext with it — one wrap, two variants.
#[tokio::test]
async fn a_wrapped_send_sends_and_files_the_same_ciphertext_differing_only_in_bcc() {
    let (imap_stream, imap_recorded) = MockStream::new(script(&[
        GREETING,
        LOGIN_OK,
        "* LIST (\\HasNoChildren \\Sent) \"/\" \"Sent\"\r\na2 OK LIST done\r\n",
        "+ OK send literal\r\n",
        "a3 OK [APPENDUID 50 9] APPEND completed\r\n",
    ]));
    let mut conn = Connection::open(imap_stream).await.unwrap();
    conn.login("alice", "pw").await.unwrap();
    let wrapper = seal(false);
    let account = ImapAccount::offline_wrapped(conn, None, Some(wrapper.clone()));
    let provider = account.provider(MailboxId::try_from("INBOX").unwrap());

    // To + Bcc: two envelope recipients.
    let smtp = script(&[
        "220 mail\r\n",
        "250 OK\r\n",
        "250 2.1.0 OK\r\n",
        "250 2.1.5 OK\r\n",
        "250 2.1.5 OK\r\n",
        "354 go ahead\r\n",
        "250 2.0.0 queued\r\n",
        "221 bye\r\n",
    ]);
    let (smtp_stream, smtp_recorded) = MockStream::new(smtp);

    let draft = draft()
        .with_bcc(vec![EmailAddress::new("dave@test.local")])
        .with_crypto(true, true);
    let as_trait: Arc<dyn MimeWrapper> = wrapper.clone();
    let sub = prepare_submission(&draft, Some(&as_trait)).await.unwrap();
    provider
        .submit_over(
            smtp_stream,
            &sub,
            &draft,
            None,
            &engine_provider::HandOver::new(&engine_provider::Unrecorded),
        )
        .await
        .unwrap();

    // The wrapper ran exactly once, on the FILED assembly.
    assert_eq!(wrapper.seen.lock().expect("seal lock").len(), 1);

    // The wire: ciphertext, both envelope recipients, no Bcc header, no plaintext.
    let conversation = written(&smtp_recorded);
    assert!(
        conversation.contains("RCPT TO:<dave@test.local>"),
        "{conversation}"
    );
    assert!(conversation.contains("CIPHERTEXT"), "{conversation}");
    assert!(!conversation.contains("Bcc:"), "{conversation}");
    assert!(
        !conversation.contains("the secret body"),
        "plaintext never crosses the wire: {conversation}"
    );

    // The filed copy: the SAME ciphertext, Bcc header kept.
    let appended = written(&imap_recorded);
    assert!(appended.contains("CIPHERTEXT"), "{appended}");
    assert!(appended.contains("Bcc: dave@test.local\r\n"), "{appended}");
    assert!(
        !appended.contains("the secret body"),
        "Sent never holds the plaintext of a wrapped send: {appended}"
    );
}

/// A wrapper's refusal settles the send BEFORE any socket dials: the error is
/// the wrapper's own permanent one, not a dial failure (this SMTP address
/// refuses connections, so a dial that happened first would read differently).
#[tokio::test]
async fn a_refusal_settles_the_send_before_any_socket_dials() {
    let dead_end = SmtpSender::Plaintext {
        addr: "127.0.0.1:1".to_owned(),
    };
    let (provider, wrapper) =
        wrapped_provider(script(&[GREETING, LOGIN_OK]), Some(dead_end), seal(true)).await;

    let draft = draft().with_crypto(false, true);
    let err = provider
        .submit(
            &draft,
            &engine_provider::HandOver::new(&engine_provider::Unrecorded),
        )
        .await
        .expect_err("a refused send never delivers");

    assert!(
        !err.is_retryable(),
        "a gate the user must resolve is permanent: {err}"
    );
    assert!(
        err.to_string().contains("gate: no trusted key"),
        "the wrapper's own reason crosses verbatim: {err}"
    );
    assert_eq!(
        wrapper.seen.lock().expect("seal lock").len(),
        1,
        "the wrapper was consulted before the refusal"
    );
}

/// The Sent-copy repair of an `Unfiled` wrapped send must re-wrap: filing the
/// plaintext would leak the very message the crypto was meant to protect.
#[tokio::test]
async fn the_repair_re_wraps_the_copy_instead_of_filing_plaintext() {
    let (imap_stream, imap_recorded) = MockStream::new(script(&[
        GREETING,
        LOGIN_OK,
        "* LIST (\\HasNoChildren \\Sent) \"/\" \"Sent\"\r\na2 OK LIST done\r\n",
        "* 1 EXISTS\r\n* OK [UIDVALIDITY 50] valid\r\na3 OK [READ-WRITE] SELECT done\r\n",
        "* SEARCH\r\na4 OK SEARCH completed\r\n",
        "+ OK send literal\r\n",
        "a5 OK [APPENDUID 50 9] APPEND completed\r\n",
    ]));
    let mut conn = Connection::open(imap_stream).await.unwrap();
    conn.login("alice", "pw").await.unwrap();
    let wrapper = seal(false);
    let account = ImapAccount::offline_wrapped(conn, None, Some(wrapper.clone()));
    let provider = account.provider(MailboxId::try_from("INBOX").unwrap());

    let draft = draft()
        .with_bcc(vec![EmailAddress::new("dave@test.local")])
        .with_crypto(true, true);
    let key = provider.refile(&draft).await.expect("the copy is filed");

    assert_eq!(key.as_str(), "imap:v50:u9@Sent");
    let appended = written(&imap_recorded);
    assert!(appended.contains("CIPHERTEXT"), "{appended}");
    assert!(appended.contains("Bcc: dave@test.local\r\n"), "{appended}");
    assert!(
        !appended.contains("the secret body"),
        "the repair never files plaintext: {appended}"
    );
}

/// An installed wrapper is inert for a draft that does not ask: the send is
/// byte-for-byte the pre-seam one.
#[tokio::test]
async fn an_installed_wrapper_is_inert_for_a_draft_that_does_not_ask() {
    let wrapper = seal(false);
    let as_trait: Arc<dyn MimeWrapper> = wrapper.clone();
    let sub = prepare_submission(&draft(), Some(&as_trait)).await.unwrap();

    assert!(wrapper.seen.lock().expect("seal lock").is_empty());
    assert!(
        sub.filed.is_none(),
        "a plain send derives its filed copy later"
    );
    let message = String::from_utf8(sub.message.clone()).expect("utf-8");
    assert!(message.contains("the secret body"), "{message}");
}
