// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the send gate: passthrough, the typed fail-closed arms, the
//! sign-then-encrypt composition shape, protected headers, and the wrapping
//! dispatch. The backend is a recording mock; the keystore is a
//! MemoryKeyStore-shaped fixture (the shape kylins' apply tests use); the
//! policy is an in-memory fixture.

use std::collections::HashMap;
use std::sync::Mutex;

use super::*;
use crate::envelope::{DetachedSignature, EncryptedEnvelope, EncryptedPart, SignedEnvelope};
use crate::error::Result as CryptoResult;
use crate::keystore::StoredKey;
use crate::trust::{RecipientTrustState, RecipientTrustStatus};

/// Memory-backed `KeyStore` fixture, keyed by `KeyHandle` with an email
/// index for `find_by_email` (the shape kylins' MemoryKeyStore takes).
#[derive(Default)]
struct MemoryKeyStore {
    inner: Mutex<HashMap<KeyHandle, StoredKey>>,
    emails: Mutex<HashMap<(Standard, String), Vec<KeyHandleRef>>>,
}

#[async_trait::async_trait]
impl KeyStore for MemoryKeyStore {
    async fn put(&self, key: StoredKey) -> CryptoResult<KeyHandleRef> {
        let handle = key.handle.clone();
        self.inner
            .lock()
            .unwrap()
            .insert(handle.handle.clone(), key);
        Ok(handle)
    }

    async fn get(&self, handle: &KeyHandle) -> CryptoResult<Option<StoredKey>> {
        // StoredKey is not Clone (the SecretBox private half); the gate tests
        // never exercise get (the mock backend resolves nothing), so a miss
        // is fine here.
        let _ = handle;
        Ok(None)
    }

    async fn find_by_email(
        &self,
        standard: Standard,
        email: &str,
    ) -> CryptoResult<Vec<KeyHandleRef>> {
        Ok(self
            .emails
            .lock()
            .unwrap()
            .get(&(standard, email.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    async fn remove(&self, handle: &KeyHandle) -> CryptoResult<()> {
        self.inner.lock().unwrap().remove(handle);
        Ok(())
    }
}

impl MemoryKeyStore {
    fn seed_email(&self, standard: Standard, email: &str, handle: KeyHandleRef) {
        self.emails
            .lock()
            .unwrap()
            .entry((standard, email.to_string()))
            .or_default()
            .push(handle);
    }
}

/// A recording mock backend: `encrypt` returns a canned envelope and
/// remembers the op's recipients/sign_with; `sign` returns a canned detached
/// signature; `export_public` returns canned DER.
#[derive(Default)]
struct MockBackend {
    policy: crate::policy::CryptoPolicy,
    encrypt_calls: Mutex<Vec<(Vec<String>, bool)>>,
    encrypt_payload: Mutex<Vec<u8>>,
}

#[async_trait::async_trait]
impl CryptoBackend for MockBackend {
    fn standard(&self) -> Standard {
        Standard::Smime
    }

    fn policy(&self) -> &crate::policy::CryptoPolicy {
        &self.policy
    }

    async fn encrypt(&self, op: EncryptOp<'_>) -> CryptoResult<EncryptedEnvelope> {
        let recipients = op
            .recipients
            .iter()
            .map(|r| r.fingerprint.as_str().to_string())
            .collect();
        self.encrypt_calls
            .lock()
            .unwrap()
            .push((recipients, op.sign_with.is_some()));
        *self.encrypt_payload.lock().unwrap() = op.parts[0].data.clone();
        Ok(EncryptedEnvelope {
            standard: Standard::Smime,
            serialization: SerializationStrategy::SingleMimeBlob,
            parts: vec![EncryptedPart {
                id: PartId("body".into()),
                kind: PartKind::Body,
                ciphertext: b"CANNED-CMS".to_vec(),
                signature: None,
            }],
            recipients: Vec::new(),
        })
    }

    async fn decrypt(&self, _op: crate::DecryptOp<'_>) -> CryptoResult<crate::DecryptedPayload> {
        unimplemented!("not exercised by the send gate tests")
    }

    async fn sign(&self, op: SignOp<'_>) -> CryptoResult<SignedEnvelope> {
        Ok(SignedEnvelope {
            standard: Standard::Smime,
            payload: op.payload.to_vec(),
            signature: DetachedSignature {
                standard: Standard::Smime,
                signer: op.signing_key,
                signature: b"CANNED-SIG".to_vec(),
            },
        })
    }

    async fn verify(&self, _op: crate::VerifyOp<'_>) -> CryptoResult<crate::VerificationResult> {
        unimplemented!("not exercised by the send gate tests")
    }

    async fn generate_key(&self, _params: crate::KeyGenParams) -> CryptoResult<KeyHandleRef> {
        unimplemented!("not exercised by the send gate tests")
    }

    async fn import_key(
        &self,
        _data: &[u8],
        _passphrase: Option<crate::SecretBox<String>>,
    ) -> CryptoResult<KeyHandleRef> {
        unimplemented!("not exercised by the send gate tests")
    }

    async fn export_public(&self, _handle: &KeyHandle) -> CryptoResult<Vec<u8>> {
        Ok(b"CANNED-DER".to_vec())
    }
}

/// In-memory policy fixture: every knob the gate reads, in one struct.
#[derive(Default)]
struct FixturePolicy {
    failures: Vec<RecipientTrustFailure>,
    anchors: Vec<Vec<u8>>,
    chain_error: Option<String>,
    protect_subject: bool,
    protect_subject_error: bool,
    gossip_calls: Mutex<usize>,
    gossip_entity_prefix: Option<Vec<u8>>,
}

#[async_trait::async_trait]
impl SendCryptoPolicy for FixturePolicy {
    async fn recipient_trust_failures(
        &self,
        _standard: Standard,
        _emails: &[String],
    ) -> Vec<RecipientTrustFailure> {
        self.failures.clone()
    }

    async fn pin_preferred_handle(
        &self,
        _standard: Standard,
        _email: &str,
        handles: &[KeyHandleRef],
    ) -> Option<KeyHandleRef> {
        handles.first().cloned()
    }

    async fn trust_anchor_ders(&self) -> Result<Vec<Vec<u8>>, String> {
        Ok(self.anchors.clone())
    }

    async fn validate_recipient_certs(
        &self,
        _recipient_cert_ders: &[Vec<u8>],
        _trust_anchor_ders: &[Vec<u8>],
        _now_unix: u64,
    ) -> Result<(), String> {
        match &self.chain_error {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    async fn protect_subject(&self) -> Result<bool, String> {
        if self.protect_subject_error {
            return Err("store hiccup".to_string());
        }
        Ok(self.protect_subject)
    }

    async fn maybe_prepend_gossip_headers(
        &self,
        _backend: &dyn CryptoBackend,
        entity: &[u8],
        _account_email: &str,
        _recipient_emails: &[String],
        _recipients: &[KeyHandleRef],
    ) -> Vec<u8> {
        *self.gossip_calls.lock().unwrap() += 1;
        match &self.gossip_entity_prefix {
            Some(prefix) => {
                let mut out = prefix.clone();
                out.extend_from_slice(entity);
                out
            }
            None => entity.to_vec(),
        }
    }
}

fn signer_key() -> DefaultSigningKey {
    DefaultSigningKey {
        standard: "smime".to_string(),
        fingerprint: "fFsigner".to_string(),
        email: Some("me@x.com".to_string()),
        ..Default::default()
    }
}

fn handle_for(standard: Standard, email: &str) -> KeyHandleRef {
    KeyHandleRef {
        handle: KeyHandle::Software(crate::KeyId(format!("{}|fp-{email}", standard.as_str()))),
        standard,
        fingerprint: Fingerprint::new(format!("fp-{email}")),
        usage: KeyUsage::SignAndEncrypt,
        algorithm: "ECDSA-P256".into(),
    }
}

const MIME: &[u8] = b"From: Me <me@x.com>\r\nSubject: secret plans\r\nMessage-ID: <m@x>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nhello body";

fn draft(sign: bool, encrypt: bool) -> SendDraft {
    SendDraft {
        to: vec![AddressSpec {
            name: None,
            email: "peer@x.com".to_string(),
        }],
        sign,
        encrypt,
        ..Default::default()
    }
}

fn seeded_keystore(standard: Standard, emails: &[&str]) -> MemoryKeyStore {
    let ks = MemoryKeyStore::default();
    for email in emails {
        ks.seed_email(standard, email, handle_for(standard, email));
    }
    ks
}

#[tokio::test]
async fn passthrough_when_neither_sign_nor_encrypt() {
    let backend = MockBackend::default();
    let ks = MemoryKeyStore::default();
    let policy = FixturePolicy::default();
    let out = apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(false, false),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect("passthrough");
    assert_eq!(out, MIME, "no crypto: the input is returned unchanged");
    assert!(backend.encrypt_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn standard_override_mismatch_fails_closed() {
    let backend = MockBackend::default();
    let ks = MemoryKeyStore::default();
    let policy = FixturePolicy::default();
    let mut d = draft(false, true);
    d.standard_override = Some("openpgp".to_string());
    let err = apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &d,
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect_err("an override/dispatch mismatch is a caller bug");
    assert!(
        matches!(
            err,
            CryptoSendError::Backend(CryptoError::UnsupportedStandard(_))
        ),
        "typed fail-closed: {err}"
    );
}

#[tokio::test]
async fn sign_without_a_default_key_is_no_signing_key() {
    let backend = MockBackend::default();
    let ks = MemoryKeyStore::default();
    let policy = FixturePolicy::default();
    let err = apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(true, false),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect_err("no default signing key");
    assert!(matches!(err, CryptoSendError::NoSigningKey));
}

#[tokio::test]
async fn trust_gate_refusal_never_reaches_encrypt() {
    let backend = MockBackend::default();
    let ks = MemoryKeyStore::default();
    let policy = FixturePolicy {
        failures: vec![RecipientTrustFailure {
            email: "peer@x.com".to_string(),
            state: RecipientTrustState::Undecided,
            fingerprint: Some("fppeer".to_string()),
        }],
        ..Default::default()
    };
    let err = apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(false, true),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect_err("an undecided recipient blocks the send");
    let CryptoSendError::RecipientTrust(failures) = err else {
        panic!("typed RecipientTrust expected: {err}");
    };
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].state, RecipientTrustState::Undecided);
    assert!(
        backend.encrypt_calls.lock().unwrap().is_empty(),
        "no ciphertext is produced when the gate refuses"
    );
}

#[tokio::test]
async fn a_recipient_the_keystore_cannot_resolve_is_missing_not_a_backend_error() {
    let backend = MockBackend::default();
    // Only the sender is seeded; peer@x.com resolves to nothing.
    let ks = seeded_keystore(Standard::Smime, &["me@x.com"]);
    let policy = FixturePolicy::default();
    let err = apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(false, true),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect_err("unresolvable recipient");
    assert!(
        matches!(err, CryptoSendError::MissingRecipientCert(_)),
        "the precise fail-closed miss: {err}"
    );
    assert_eq!(
        err.to_string(),
        "no S/MIME cert for recipient peer@x.com",
        "the standard-aware phrase"
    );
    assert!(backend.encrypt_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn smime_chain_invalid_recipient_fails_closed_before_encrypt() {
    let backend = MockBackend::default();
    let ks = seeded_keystore(Standard::Smime, &["me@x.com", "peer@x.com"]);
    let policy = FixturePolicy {
        chain_error: Some("recipient cert #0 invalid: expired".to_string()),
        ..Default::default()
    };
    let err = apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(false, true),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect_err("chain-invalid recipient");
    assert!(
        matches!(err, CryptoSendError::InvalidRecipientCert(_)),
        "typed chain failure: {err}"
    );
    assert!(backend.encrypt_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn pgp_recipients_skip_the_cert_chain_gate() {
    let backend = MockBackend::default();
    let ks = seeded_keystore(Standard::OpenPgp, &["me@x.com", "peer@x.com"]);
    // A chain error that WOULD fail an S/MIME send must not fire for PGP.
    let policy = FixturePolicy {
        chain_error: Some("must not be consulted".to_string()),
        ..Default::default()
    };
    let out = apply_crypto(
        &backend,
        Standard::OpenPgp,
        &ks,
        MIME,
        &draft(false, true),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect("PGP encrypt proceeds without chain validation");
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("multipart/encrypted"),
        "PGP/MIME framing: {text}"
    );
}

#[tokio::test]
async fn encrypt_protects_the_subject_by_default() {
    let backend = MockBackend::default();
    let ks = seeded_keystore(Standard::Smime, &["me@x.com", "peer@x.com"]);
    let policy = FixturePolicy {
        protect_subject: true,
        ..Default::default()
    };
    let out = apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(false, true),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect("encrypt");
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("Subject: ...\r\n"),
        "the outer subject is the placeholder: {text}"
    );
    assert!(
        !text.contains("secret plans\r\n\r\n") || text.contains("protected-headers"),
        "the real subject only rides inside the protected wrapper"
    );
    // The encrypted payload carries the protected-headers wrapper with the
    // real subject inside.
    let payload = backend.encrypt_payload.lock().unwrap().clone();
    let inner = String::from_utf8(payload).unwrap();
    assert!(inner.contains("protected-headers=\"v1\""));
    assert!(inner.contains("Subject: secret plans"));
}

#[tokio::test]
async fn protect_subject_read_error_defaults_to_on() {
    let backend = MockBackend::default();
    let ks = seeded_keystore(Standard::Smime, &["me@x.com", "peer@x.com"]);
    let policy = FixturePolicy {
        protect_subject_error: true,
        ..Default::default()
    };
    apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(false, true),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect("encrypt");
    let payload = backend.encrypt_payload.lock().unwrap().clone();
    let inner = String::from_utf8(payload).unwrap();
    assert!(
        inner.contains("protected-headers=\"v1\""),
        "fail-safe: a store hiccup never leaks the subject"
    );
}

#[tokio::test]
async fn encrypt_respects_protect_subject_off() {
    let backend = MockBackend::default();
    let ks = seeded_keystore(Standard::Smime, &["me@x.com", "peer@x.com"]);
    let policy = FixturePolicy {
        protect_subject: false,
        ..Default::default()
    };
    let out = apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(false, true),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect("encrypt");
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("Subject: secret plans\r\n"));
    let payload = backend.encrypt_payload.lock().unwrap().clone();
    assert!(!String::from_utf8(payload)
        .unwrap()
        .contains("protected-headers"));
}

#[tokio::test]
async fn encrypt_only_smime_wraps_enveloped_and_encrypts_to_self_first() {
    let backend = MockBackend::default();
    let ks = seeded_keystore(Standard::Smime, &["me@x.com", "peer@x.com"]);
    let policy = FixturePolicy::default();
    let out = apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(false, true),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect("encrypt");
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("application/pkcs7-mime; smime-type=enveloped-data"),
        "the S/MIME framing: {text}"
    );
    let calls = backend.encrypt_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let (recipients, signed) = &calls[0];
    assert_eq!(
        recipients.first().map(String::as_str),
        Some("fp-me@x.com"),
        "encrypt-to-self is the FIRST recipient"
    );
    assert!(recipients.contains(&"fp-peer@x.com".to_string()));
    assert!(!signed, "encrypt-only: no sign_with");
}

#[tokio::test]
async fn sign_and_encrypt_composes_sign_with_never_clear_sign_then_encrypt() {
    let backend = MockBackend::default();
    let ks = seeded_keystore(Standard::Smime, &["me@x.com", "peer@x.com"]);
    let policy = FixturePolicy::default();
    apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(true, true),
        "me@x.com",
        Some(&signer_key()),
        &policy,
    )
    .await
    .expect("sign+encrypt");
    let calls = backend.encrypt_calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "exactly one backend.encrypt");
    assert!(
        calls[0].1,
        "the signer rides EncryptOp.sign_with (opaque SignedData inside EnvelopedData)"
    );
}

#[tokio::test]
async fn gossip_runs_only_for_openpgp_sign_and_encrypt() {
    let backend = MockBackend::default();
    let ks = seeded_keystore(Standard::OpenPgp, &["me@x.com", "peer@x.com"]);
    let policy = FixturePolicy {
        gossip_entity_prefix: Some(b"Autocrypt-Gossip: addr=peer@x.com; keydata=QUJD\r\n".to_vec()),
        ..Default::default()
    };
    let mut signer = signer_key();
    signer.standard = "openpgp".to_string();
    apply_crypto(
        &backend,
        Standard::OpenPgp,
        &ks,
        MIME,
        &draft(true, true),
        "me@x.com",
        Some(&signer),
        &policy,
    )
    .await
    .expect("pgp sign+encrypt");
    assert_eq!(
        *policy.gossip_calls.lock().unwrap(),
        1,
        "gossip injection fired for openpgp sign+encrypt"
    );
    let payload = backend.encrypt_payload.lock().unwrap().clone();
    assert!(
        String::from_utf8(payload)
            .unwrap()
            .starts_with("Autocrypt-Gossip:"),
        "gossip headers land INSIDE the encrypted part"
    );

    // Encrypt-only (no sign): gossip must NOT fire (TB's gating).
    let backend2 = MockBackend::default();
    let policy2 = FixturePolicy::default();
    apply_crypto(
        &backend2,
        Standard::OpenPgp,
        &seeded_keystore(Standard::OpenPgp, &["me@x.com", "peer@x.com"]),
        MIME,
        &draft(false, true),
        "me@x.com",
        None,
        &policy2,
    )
    .await
    .expect("pgp encrypt-only");
    assert_eq!(*policy2.gossip_calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn sign_only_smime_wraps_multipart_signed_and_leaves_subject_clear() {
    let backend = MockBackend::default();
    let ks = MemoryKeyStore::default();
    let policy = FixturePolicy::default();
    let out = apply_crypto(
        &backend,
        Standard::Smime,
        &ks,
        MIME,
        &draft(true, false),
        "me@x.com",
        Some(&signer_key()),
        &policy,
    )
    .await
    .expect("sign-only");
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("Subject: secret plans\r\n"),
        "sign-only keeps the clear subject"
    );
    assert!(
        text.contains("multipart/signed; protocol=\"application/pkcs7-signature\"; micalg=sha-256")
    );
    // Part 1 is the exact signed region: entity + exactly one trailing CRLF.
    assert!(text.contains(
        "------=_astra_smime_signed_0001\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nhello body\r\n------=_astra_smime_signed_0001\r\n"
    ));
    assert!(backend.encrypt_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn sign_only_openpgp_wraps_rfc3156_signed() {
    let backend = MockBackend::default();
    let ks = MemoryKeyStore::default();
    let policy = FixturePolicy::default();
    let mut signer = signer_key();
    signer.standard = "openpgp".to_string();
    let out = apply_crypto(
        &backend,
        Standard::OpenPgp,
        &ks,
        MIME,
        &draft(true, false),
        "me@x.com",
        Some(&signer),
        &policy,
    )
    .await
    .expect("pgp sign-only");
    let text = String::from_utf8(out).unwrap();
    assert!(text
        .contains("multipart/signed; protocol=\"application/pgp-signature\"; micalg=pgp-sha512"));
    assert!(text.contains(
        "Content-Type: text/plain; charset=utf-8\r\n\r\nhello body\r\n------=_astra_pgp_signed_0001\r\n"
    ));
}

#[tokio::test]
async fn sm_is_a_typed_unsupported_standard() {
    let backend = MockBackend::default();
    let ks = seeded_keystore(Standard::Sm, &["me@x.com", "peer@x.com"]);
    let policy = FixturePolicy::default();
    let err = apply_crypto(
        &backend,
        Standard::Sm,
        &ks,
        MIME,
        &draft(false, true),
        "me@x.com",
        None,
        &policy,
    )
    .await
    .expect_err("sm not implemented");
    assert!(matches!(
        err,
        CryptoSendError::Backend(CryptoError::UnsupportedStandard(_))
    ));
}

#[test]
fn recipient_trust_status_wire_shape_is_camelcase_with_kebab_state() {
    let status = RecipientTrustStatus {
        email: "p@x.com".to_string(),
        standard: "smime".to_string(),
        state: RecipientTrustState::AcceptedOtherEmail,
        fingerprint: Some("fp".to_string()),
        decision: None,
        accepted_elsewhere: vec!["a@x.com".to_string()],
        key_expired: false,
    };
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["state"], "accepted-other-email");
    assert!(json.get("acceptedElsewhere").is_some());
    assert!(json.get("keyExpired").is_some());
    assert!(json.get("decision").is_none(), "skip_serializing_if");
}

#[test]
fn crypto_method_db_strings_round_trip() {
    assert_eq!(CryptoMethod::from_db_str("smime"), CryptoMethod::Smime);
    assert_eq!(CryptoMethod::from_db_str("openpgp"), CryptoMethod::Openpgp);
    assert_eq!(CryptoMethod::from_db_str("sm"), CryptoMethod::None);
    assert_eq!(CryptoMethod::from_db_str(""), CryptoMethod::None);
    assert_eq!(CryptoMethod::Smime.standard(), Some(Standard::Smime));
    assert_eq!(CryptoMethod::Openpgp.standard(), Some(Standard::OpenPgp));
    assert_eq!(CryptoMethod::None.standard(), None);
    assert_eq!(CryptoMethod::Smime.as_db_str(), "smime");
}

#[test]
fn key_handle_ref_builds_software_and_token_handles() {
    let soft = key_handle_ref(&signer_key());
    assert!(matches!(soft.handle, KeyHandle::Software(_)));
    if let KeyHandle::Software(id) = &soft.handle {
        assert_eq!(id.0, "smime|fFsigner");
    }
    assert_eq!(soft.algorithm, "ECDSA-P256");

    let token = key_handle_ref(&DefaultSigningKey {
        standard: "openpgp".to_string(),
        fingerprint: "fpT".to_string(),
        token_serial: Some("SER".to_string()),
        token_key_id: Some("k1".to_string()),
        ..Default::default()
    });
    assert!(
        matches!(token.handle, KeyHandle::Token { .. }),
        "token rows build Token handles"
    );
    assert_eq!(token.algorithm, "default");
}
