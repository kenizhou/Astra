// SPDX-License-Identifier: GPL-3.0-only
//! The built-in crypto decoder's W9 password prompt proof: a SKESK-only
//! message (no PKESK slot fits the recipient's key) opens needs-password, the
//! fail-closed banner beside the prompt affordance, no plaintext, no
//! ciphertext chip, and no result row (the verdict is transient). The fixture
//! hand-builds the SKESK message via the backend's symmetric encrypt (W9 send
//! is itself unwired); the app harness and fixtures are the parent module's.

use super::*;

/// The full RFC5322 source for a SKESK (passphrase-protected) PGP/MIME message:
/// outer headers plus the `multipart/encrypted` framing a symmetric envelope.
async fn symmetric_source(
    store: &Arc<CryptoStore>,
    pass: &str,
    inner_entity: &[u8],
    outer_subject: &str,
) -> Vec<u8> {
    let backend = OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), SENDER)),
        CryptoPolicy::default(),
    );
    let env = backend
        .encrypt_symmetric(
            &[Part {
                id: PartId("body".into()),
                kind: PartKind::Body,
                data: inner_entity.to_vec(),
            }],
            SerializationStrategy::SingleMimeBlob,
            SecretBox::new(Box::new(pass.to_owned())),
            None,
        )
        .await
        .expect("symmetric encrypt the fixture");
    let entity = pgp_mime::wrap_encrypted(&env.parts[0].ciphertext);
    let mut source =
        format!("From: peer@x.com\r\nTo: me@work.local\r\nSubject: {outer_subject}\r\n")
            .into_bytes();
    source.extend_from_slice(&entity);
    source
}

#[tokio::test]
async fn a_passphrase_protected_message_renders_the_password_prompt() {
    // A SKESK-only message: the recipient holds a private key, but no PKESK
    // slot fits it, so the verdict is needs-password (a prompt), not no-key.
    let (store, _peer, _work) = two_accounts(true).await;
    let source = symmetric_source(&store, "hunter2", &protected_inner(), "...").await;
    let app = app_with_source(store.clone(), source, "...").await;

    let reading = open(&app).await;

    assert_eq!(
        reading.security_banner,
        Some(SecurityBannerView {
            level: SecurityLevelView::Warning,
            text: "Encrypted message; a passphrase is required.".to_owned(),
        })
    );
    assert_eq!(
        reading.password_prompt,
        Some(PasswordPromptState::NeedsPassword),
        "the prompt affordance rides the snapshot"
    );
    assert_eq!(reading.plain, None, "a prompt never yields plaintext");
    assert_eq!(reading.html, None, "a prompt never yields ciphertext");
    assert_eq!(reading.subject, "...", "no subject restores on a prompt");
    assert!(!has_crypto_chip(&reading), "the ciphertext chip hides");
    assert!(
        read_result(&store, "m1").is_none(),
        "the needs-password verdict is transient: no result row lands"
    );
}
