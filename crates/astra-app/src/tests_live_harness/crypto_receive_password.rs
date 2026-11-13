// SPDX-License-Identifier: GPL-3.0-only
//! The gated W9 password receive proof (SP4c-3c D2): a passphrase-protected
//! (SKESK) PGP/MIME message delivered to astra2 opens needs-password through
//! the product's reading pipeline; the reader's passphrase re-enters through
//! `App::submit_reading_password` to the words and the restored protected
//! subject, a wrong passphrase re-prompts with no plaintext, and every copy is
//! withdrawn. W9 send is itself unwired, so the fixture hand-builds the SKESK
//! message via the backend's symmetric encrypt and delivers it through the
//! harness's raw SMTP submission listener (the reading-proof discipline);
//! every assertion that follows reads through the product.

use std::sync::Arc;

use astra_crypto_core::{
    mime, pgp_mime, CryptoPolicy, Part, PartId, PartKind, SecretBox, SerializationStrategy,
};
use astra_crypto_openpgp::OpenpgpBackend;
use astra_crypto_store::{CryptoStore, SqliteKeyStore};
use astra_viewmodel::PasswordPromptState;
use engine_api::{EmailAddress, Engine};

use crate::crypto_decode::CryptoDecoder;
use crate::{Account, App, AppObserver, FolderRef, Intent};

use super::{
    dial_with_accepted_certificate, harness, inbox_key, proof, row_by_subject, CountingObserver,
};

/// The sender's address, the first harness user (the raw submission identity).
const SENDER: &str = "astra1@example.test";
/// The protected subject the proof restores: unique among the harness fixtures.
const SUBJECT: &str = "Harness W9 password proof";
/// The words the encrypted body carries; the reading snapshot asserts them.
const WORDS: &str = "Sealed for the W9 password proof, opened by the passphrase.";
/// The passphrase the fixture seals under and the reader supplies.
const PASS: &str = "hunter2";

fn passphrase(s: &str) -> SecretBox<String> {
    SecretBox::new(Box::new(s.to_owned()))
}

/// The full RFC5322 source for a SKESK PGP/MIME message under `pass`: outer
/// headers (the protected placeholder subject) plus the `multipart/encrypted`
/// framing a symmetric envelope, the protected real subject over both legs.
/// Hand-built via the backend's symmetric encrypt: W9 send is itself unwired.
async fn symmetric_source(store: &Arc<CryptoStore>, sender: &str, pass: &str) -> Vec<u8> {
    let backend = OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), sender)),
        CryptoPolicy::default(),
    );
    let alternative = format!(
        "Content-Type: multipart/alternative; boundary=\"alt\"\r\n\r\n\
         --alt\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{WORDS}\r\n\
         --alt\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
         <p>{WORDS}</p>\r\n--alt--\r\n"
    );
    let inner = mime::wrap_protected_headers(
        alternative.as_bytes(),
        Some(&format!("Subject: {SUBJECT}")),
        true,
    );
    let env = backend
        .encrypt_symmetric(
            &[Part {
                id: PartId("body".into()),
                kind: PartKind::Body,
                data: inner,
            }],
            SerializationStrategy::SingleMimeBlob,
            passphrase(pass),
            None,
        )
        .await
        .expect("symmetric encrypt the fixture");
    let entity = pgp_mime::wrap_encrypted(&env.parts[0].ciphertext);
    let mut source = format!(
        "From: {SENDER}\r\nTo: {}\r\nSubject: ...\r\n",
        proof::RECIPIENT
    )
    .into_bytes();
    source.extend_from_slice(&entity);
    source
}

/// Submits a raw source from astra1 to astra2 through the harness's SMTP
/// submission listener: curl over implicit TLS, the reading-proof discipline.
/// Seeding only; every assertion that follows reads through the product.
fn submit_raw(host: &str, password: &str, source: &[u8]) {
    let path =
        std::env::temp_dir().join(format!("astra-password-proof-{}.eml", std::process::id()));
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

/// The gated run: the delivered SKESK mail opens needs-password, the correct
/// passphrase re-enters to the words and the restored subject, a wrong
/// passphrase re-prompts with no plaintext. Every copy is withdrawn.
#[tokio::test]
async fn the_password_receive_opens_the_delivered_skesk_mail_through_the_reading_pipeline() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    // The fixed test master key keeps the proof off the OS keyring.
    astra_crypto_store::vault::set_test_master_key([0x42u8; 32]);

    // The crypto store: the recipient holds no key that fits a SKESK-only
    // message, so the open prompts. The sender is registered for the fixture's
    // keystore handle (the symmetric encrypt needs no signing key).
    let store = Arc::new(CryptoStore::open_in_memory().expect("the in-memory store opens"));
    let sender_id = super::account(&host, &port, &user, &password, Vec::new())
        .account_id()
        .expect("the sender's account id");
    let recipient_id = super::account(&host, &port, proof::RECIPIENT, &password, Vec::new())
        .account_id()
        .expect("the recipient's account id");
    store.register_account(sender_id.as_str(), SENDER).unwrap();
    store
        .register_account(recipient_id.as_str(), proof::RECIPIENT)
        .unwrap();

    // Both accounts dial ordinarily; the built-in crypto decoder rides the
    // seeded store, and the same store is installed for the password re-entry.
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
    app.install_crypto_store(store.clone());

    // A known state: sweep any earlier run's proof mail first.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(&app, &sender, &[SUBJECT, "..."]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT, "..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();

    // The delivery: the hand-built SKESK mail reaches the recipient's INBOX.
    let source = symmetric_source(&store, sender_id.as_str(), PASS).await;
    submit_raw(&host, &password, &source);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox =
        FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient)).expect("the INBOX");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, "...");
    let astra_viewmodel::RowKey::Message { key, .. } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    let message = crate::reference::MessageRef {
        account: recipient.clone(),
        key: key.clone(),
    };

    // The open: needs-password, fail closed (the banner and the affordance,
    // no plaintext, no restored subject, no ciphertext chip).
    app.dispatch(Intent::OpenMessage {
        message: message.clone(),
    })
    .await
    .unwrap();
    let closed = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert_eq!(
        closed.password_prompt,
        Some(PasswordPromptState::NeedsPassword),
        "the SKESK open answers the needs-password affordance"
    );
    let banner = closed
        .security_banner
        .as_ref()
        .expect("the needs-password banner rides the snapshot");
    assert!(
        banner.text.contains("a passphrase is required"),
        "the banner names the passphrase prompt: {}",
        banner.text
    );
    assert_eq!(closed.plain, None, "the open never yields plaintext");
    assert_eq!(closed.html, None, "the open never yields ciphertext");
    assert_eq!(closed.subject, "...", "no subject restores on the prompt");
    assert!(
        !closed
            .attachments
            .iter()
            .any(|chip| chip.file_name.eq_ignore_ascii_case("encrypted.asc")),
        "the ciphertext chip hides: {:?}",
        closed.attachments
    );
    assert!(
        astra_crypto_store::get_message_crypto_result(&store, recipient.as_str(), key.as_str())
            .is_none(),
        "the needs-password verdict is transient: no result row lands"
    );

    // A wrong passphrase: the re-prompt signal, no plaintext, no row.
    let prompt = app
        .submit_reading_password(message.clone(), passphrase("not-the-passphrase"))
        .await;
    assert_eq!(prompt, Some(PasswordPromptState::WrongPassword));
    let reprompt = app
        .reading()
        .expect("the re-entry republishes the snapshot");
    assert_eq!(
        reprompt.password_prompt,
        Some(PasswordPromptState::WrongPassword),
        "the wrong passphrase re-prompts"
    );
    assert_eq!(
        reprompt.plain, None,
        "a wrong passphrase never yields plaintext"
    );
    assert_eq!(
        reprompt.html, None,
        "a wrong passphrase never yields ciphertext"
    );

    // The correct passphrase: the words and the restored protected subject,
    // the affordance dropped, the ok result row landed.
    let prompt = app
        .submit_reading_password(message.clone(), passphrase(PASS))
        .await;
    assert_eq!(prompt, None, "the decrypt drops the prompt affordance");
    let reading = app
        .reading()
        .expect("the re-entry republishes the snapshot");
    assert_eq!(reading.password_prompt, None, "the affordance is gone");
    assert_eq!(reading.subject, SUBJECT, "the protected subject restores");
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "the words render in the plain leg: {:?}",
        reading.plain
    );
    assert!(
        reading
            .html
            .as_deref()
            .is_some_and(|body| body.contains(WORDS)),
        "the words render in the html leg: {:?}",
        reading.html
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
    let result =
        astra_crypto_store::get_message_crypto_result(&store, recipient.as_str(), key.as_str())
            .expect("the ok result row lands keyed by the account and provider key");
    assert_eq!(result.decrypt_state, "ok");

    // The cleanup: every proof mail leaves both accounts.
    proof::delete_subject_mail(&app, &sender, &[SUBJECT, "..."]).await;
    proof::delete_subject_mail(&app, &recipient, &[SUBJECT, "..."]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
}
