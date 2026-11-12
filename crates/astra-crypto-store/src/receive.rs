// SPDX-License-Identifier: GPL-3.0-only
//! The OpenPGP receive orchestrator (SP4c D1): the receive-side mirror of
//! `wrap.rs`. Where the send half interprets the crypto framing to SEAL a
//! draft, this interprets the framing to OPEN an incoming PGP/MIME message:
//! detect + unwrap `multipart/encrypted`, resolve the account's private key,
//! decrypt + verify through `decrypt_with_signers` directly, persist the
//! per-message result, and restore the protected subject.
//!
//! The result is a neutral, engine-free type (the decrypted legs + the
//! security state + the restored subject), mirroring `wrap.rs`'s dependency
//! direction: this crate reads keys and trust from the shared crypto store
//! and never depends on astra-app.
//!
//! **Fail closed.** No key, a decrypt failure, or a tampered ciphertext never
//! yields plaintext or renders the ciphertext: the outcome carries the state
//! and empty legs. Only a successful decrypt releases the legs.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content, its
//! recipients, or its subject; counts and ids only.

use std::sync::Arc;

use astra_crypto_core::{
    mime, pgp_mime, CryptoError, CryptoPolicy, EncryptedEnvelope, EncryptedPart, Fingerprint,
    KeyHandle, KeyHandleRef, KeyId, KeyUsage, PartId, PartKind, SerializationStrategy,
    SignatureState, Standard, TokenKeyId,
};
use astra_crypto_openpgp::{keymap, OpenpgpBackend};
use sequoia_openpgp as openpgp;

use crate::{
    get_crypto_key_full, list_collected_keys_for_account, list_crypto_keys_for_account,
    CryptoKeyRow, CryptoStore, SqliteKeyStore,
};

/// Where the receive path landed for one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiveState {
    /// The source is not PGP/MIME `multipart/encrypted`: the caller leaves the
    /// store's own rendering untouched and no result row is written.
    NotEncrypted,
    /// The framing is recognised but the account holds no private OpenPGP key
    /// that can decrypt it. Fail closed: no plaintext.
    NoKey,
    /// The framing is recognised and a key was offered, but the decrypt failed
    /// (a tampered ciphertext, a bad embedded signature, a wrong key). Fail
    /// closed: no plaintext.
    Failed,
    /// The ciphertext decrypted; the legs and the security state are carried.
    Decrypted,
}

/// The neutral receive answer: the decrypted legs, the security state, and the
/// restored protected subject. Engine-free; the reading integration maps it to
/// the snapshot's legs and banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveOutcome {
    /// Where the receive path landed.
    pub state: ReceiveState,
    /// The signature verdict (the six-state vocabulary).
    pub signature_state: SignatureState,
    /// The signer's primary fingerprint, when one was identified.
    pub signer_fingerprint: Option<String>,
    /// The signer's email, when one was identified.
    pub signer_email: Option<String>,
    /// The first weak/legacy signature-algorithm warning, when one was noticed.
    pub weak_warning: Option<String>,
    /// The restored protected subject (the real Subject inside the protected
    /// headers), when the message carried one.
    pub subject: Option<String>,
    /// The decrypted plain-text leg.
    pub plain: Option<String>,
    /// The decrypted HTML leg.
    pub html: Option<String>,
}

impl ReceiveOutcome {
    /// A fail-closed answer: the state and signature verdict, empty legs. No
    /// plaintext, ciphertext, or subject is released on any non-decrypt arm.
    fn closed(state: ReceiveState, signature_state: SignatureState) -> Self {
        Self {
            state,
            signature_state,
            signer_fingerprint: None,
            signer_email: None,
            weak_warning: None,
            subject: None,
            plain: None,
            html: None,
        }
    }
}

/// Opens one incoming PGP/MIME message for `account_id`, persisting the result
/// row keyed by `(account_id, message_id)`. See the module docs for the
/// fail-closed contract.
///
/// The steps: detect + unwrap the `multipart/encrypted` framing; resolve the
/// account's private OpenPGP key; decrypt + verify through
/// [`OpenpgpBackend::decrypt_with_signers`] directly (offering the keystore
/// signers + the collected certs); map the outcome to the signature state;
/// persist the result row; restore the protected subject. A source that is not
/// the crypto framing answers [`ReceiveState::NotEncrypted`] and writes no row.
pub async fn receive_openpgp(
    store: &Arc<CryptoStore>,
    account_id: &str,
    message_id: &str,
    source: &[u8],
) -> ReceiveOutcome {
    // 1. Detect + unwrap the PGP/MIME framing. Anything else is left untouched.
    let Ok((_outer, entity)) = mime::split_message(source) else {
        return ReceiveOutcome::closed(ReceiveState::NotEncrypted, SignatureState::NotSigned);
    };
    let Some(ciphertext) = pgp_mime::unwrap_encrypted(&entity.0) else {
        // Recognised PGP/MIME framing whose ciphertext will not extract fails
        // closed (it must never render untouched); anything else is left alone.
        if pgp_mime::is_pgp_encrypted(&entity.0) {
            write_result(
                store,
                account_id,
                message_id,
                ResultVerdict {
                    crypto_kind: "encrypted",
                    decrypt_state: "failed",
                    signature_state: SignatureState::Invalid,
                    signer_fingerprint: None,
                    signer_email: None,
                },
            );
            return ReceiveOutcome::closed(ReceiveState::Failed, SignatureState::Invalid);
        }
        return ReceiveOutcome::closed(ReceiveState::NotEncrypted, SignatureState::NotSigned);
    };

    // 2. Resolve the account's private OpenPGP keys (decryption candidates).
    let Ok(rows) = list_crypto_keys_for_account(store, account_id, "openpgp") else {
        return ReceiveOutcome::closed(ReceiveState::Failed, SignatureState::Invalid);
    };
    let mut ranked: Vec<(bool, KeyHandleRef)> = rows
        .iter()
        .filter(|row| row.has_private)
        .map(|row| (row.is_default_encrypt, handle_for_row(row)))
        .collect();
    // The default encryption key leads; the rest follow as fallbacks.
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    let candidates: Vec<KeyHandleRef> = ranked.into_iter().map(|(_, handle)| handle).collect();
    if candidates.is_empty() {
        write_result(
            store,
            account_id,
            message_id,
            ResultVerdict {
                crypto_kind: "encrypted",
                decrypt_state: "no-key",
                signature_state: SignatureState::NotSigned,
                signer_fingerprint: None,
                signer_email: None,
            },
        );
        return ReceiveOutcome::closed(ReceiveState::NoKey, SignatureState::NotSigned);
    }

    // 3. Decrypt + verify, offering the keystore signers + the collected certs.
    let backend = OpenpgpBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), account_id)),
        CryptoPolicy::default(),
    );
    let known_signers: Vec<KeyHandleRef> = rows.iter().map(handle_for_row).collect();
    let extra_signer_blobs: Vec<Vec<u8>> =
        list_collected_keys_for_account(store, account_id, "openpgp")
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.public_data)
            .collect();
    let envelope = EncryptedEnvelope {
        standard: Standard::OpenPgp,
        serialization: SerializationStrategy::SingleMimeBlob,
        parts: vec![EncryptedPart {
            id: PartId("body".into()),
            kind: PartKind::Body,
            ciphertext,
            signature: None,
        }],
        recipients: Vec::new(),
    };
    let mut decrypted = None;
    for handle in &candidates {
        match backend
            .decrypt_with_signers(&envelope, handle, &known_signers, &extra_signer_blobs)
            .await
        {
            Ok(outcome) => {
                decrypted = Some(outcome);
                break;
            }
            // A wrong key fits no PKESK: the next candidate may fit.
            Err(CryptoError::NoMatchingKey(_)) => {}
            // A tampered ciphertext or a bad signature fails for every key.
            Err(_) => break,
        }
    }
    let Some(outcome) = decrypted else {
        write_result(
            store,
            account_id,
            message_id,
            ResultVerdict {
                crypto_kind: "encrypted",
                decrypt_state: "failed",
                signature_state: SignatureState::Invalid,
                signer_fingerprint: None,
                signer_email: None,
            },
        );
        return ReceiveOutcome::closed(ReceiveState::Failed, SignatureState::Invalid);
    };

    // 4. Map the outcome to the signature state. An unknown signer degrades to
    //    UnknownKey (never a failure); a verified embedded signature is
    //    ValidVerified; no embedded signature is NotSigned.
    let signed = !outcome.signer_fingerprints.is_empty();
    let signature_state = if outcome.unverifiable_signature {
        SignatureState::UnknownKey
    } else if signed {
        SignatureState::ValidVerified
    } else {
        SignatureState::NotSigned
    };
    let (signer_fingerprint, signer_email) =
        resolve_signer(store, account_id, &outcome.signer_fingerprints);

    // 5. Restore the protected subject and extract the decrypted legs.
    let inner = outcome
        .payload
        .parts
        .iter()
        .find_map(|part| matches!(part.kind, PartKind::Body).then(|| part.data.clone()))
        .unwrap_or_default();
    let (subject, legs_entity) = match mime::unwrap_protected_headers(&inner) {
        Some((subject, body)) => (subject, body),
        None => (None, inner),
    };
    let (plain, html) = mime::body_legs(&legs_entity);

    // 6. Persist the result row (the first writer of message_crypto_results).
    let crypto_kind = if signed {
        "encrypted-signed"
    } else {
        "encrypted"
    };
    write_result(
        store,
        account_id,
        message_id,
        ResultVerdict {
            crypto_kind,
            decrypt_state: "ok",
            signature_state,
            signer_fingerprint: signer_fingerprint.as_deref(),
            signer_email: signer_email.as_deref(),
        },
    );
    ReceiveOutcome {
        state: ReceiveState::Decrypted,
        signature_state,
        signer_fingerprint,
        signer_email,
        weak_warning: outcome.weak_warning,
        subject,
        plain,
        html,
    }
}

/// Build the canonical keystore handle for a key row (software or token), the
/// shape `SqliteKeyStore::get` resolves.
fn handle_for_row(row: &CryptoKeyRow) -> KeyHandleRef {
    let fingerprint = Fingerprint::new(&row.fingerprint);
    let handle = match (&row.token_serial, &row.token_key_id) {
        (Some(serial), Some(key_id)) => KeyHandle::Token {
            token_serial: serial.clone(),
            key_id: TokenKeyId(key_id.clone()),
        },
        _ => KeyHandle::Software(KeyId(format!("openpgp|{}", row.fingerprint))),
    };
    KeyHandleRef {
        handle,
        standard: Standard::OpenPgp,
        fingerprint,
        usage: KeyUsage::SignAndEncrypt,
        algorithm: "unknown".to_string(),
    }
}

/// Resolve the signer's primary fingerprint + email from the embedded
/// signatures' issuer fingerprints. The issuer fingerprint may name a signing
/// subkey, so the account's keystore certs and collected certs are scanned for
/// one owning that key. Best-effort: an unresolved signer keeps the raw issuer
/// fingerprint and no email.
fn resolve_signer(
    store: &CryptoStore,
    account_id: &str,
    issuer_fingerprints: &[openpgp::Fingerprint],
) -> (Option<String>, Option<String>) {
    let Some(target) = issuer_fingerprints.first() else {
        return (None, None);
    };
    let target_hex = target.to_hex().to_ascii_lowercase();

    if let Ok(rows) = list_crypto_keys_for_account(store, account_id, "openpgp") {
        for row in rows {
            let Ok(Some(record)) =
                get_crypto_key_full(store, account_id, "openpgp", &row.fingerprint)
            else {
                continue;
            };
            let Ok(blob) = hex::decode(&record.public_data) else {
                continue;
            };
            let Ok(certs) = keymap::parse_certs(&blob) else {
                continue;
            };
            for cert in &certs {
                if cert_owns_key(cert, &target_hex) {
                    let primary = keymap::fingerprint_of(cert).as_str().to_string();
                    let email = row.email.clone().or_else(|| email_of_cert(cert));
                    return (Some(primary), email);
                }
            }
        }
    }

    if let Ok(rows) = list_collected_keys_for_account(store, account_id, "openpgp") {
        for row in rows {
            let Ok(certs) = keymap::parse_certs(&row.public_data) else {
                continue;
            };
            for cert in &certs {
                if cert_owns_key(cert, &target_hex) {
                    let primary = keymap::fingerprint_of(cert).as_str().to_string();
                    let email = row.peer_email.clone().or_else(|| email_of_cert(cert));
                    return (Some(primary), email);
                }
            }
        }
    }

    (Some(target_hex), None)
}

/// Whether `cert` owns `target_hex` as its primary OR any subkey fingerprint.
fn cert_owns_key(cert: &openpgp::Cert, target_hex: &str) -> bool {
    cert.keys().any(|ka| {
        ka.key()
            .fingerprint()
            .to_hex()
            .eq_ignore_ascii_case(target_hex)
    })
}

/// The email of a cert's first user id carrying an `<addr>` form, when present.
fn email_of_cert(cert: &openpgp::Cert) -> Option<String> {
    for ua in cert.userids() {
        let uid = ua.userid().to_string();
        if let Some(start) = uid.find('<') {
            if let Some(end) = uid[start + 1..].find('>') {
                let email = uid[start + 1..start + 1 + end].trim();
                if !email.is_empty() {
                    return Some(email.to_string());
                }
            }
        }
    }
    None
}

/// The `message_crypto_results.signature_state` column spelling for a verdict.
fn signature_state_db(state: SignatureState) -> &'static str {
    match state {
        SignatureState::NotSigned => "not-signed",
        SignatureState::ValidVerified => "valid-verified",
        SignatureState::ValidUnverified => "valid-unverified",
        SignatureState::Invalid => "invalid",
        SignatureState::UnknownKey => "unknown-key",
        SignatureState::Mismatch => "mismatch",
    }
}

/// The verdict fields persisted into `message_crypto_results` for one message.
#[derive(Clone, Copy)]
struct ResultVerdict<'a> {
    crypto_kind: &'a str,
    decrypt_state: &'a str,
    signature_state: SignatureState,
    signer_fingerprint: Option<&'a str>,
    signer_email: Option<&'a str>,
}

/// Persist the per-message result, upserting on the `(account_id, message_id)`
/// primary key. The schema drops the foreign key to the engine's messages
/// table, so the linkage is enforced here in Rust: an empty account or message
/// id is an orphan row and is refused. `chain_valid` stays NULL (OpenPGP has no
/// X.509 chain) and `revocation_state` stays `unchecked` (CRL fetching is a
/// later slice). A store write failure is swallowed: the verdict still reaches
/// the caller, and the next open re-persists.
fn write_result(
    store: &CryptoStore,
    account_id: &str,
    message_id: &str,
    verdict: ResultVerdict<'_>,
) {
    if account_id.is_empty() || message_id.is_empty() {
        return;
    }
    let _ = store.conn().execute(
        "INSERT INTO message_crypto_results
            (account_id, message_id, crypto_kind, decrypt_state, signature_state,
             signer_fingerprint, signer_email, chain_valid, revocation_state, verified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 'unchecked', strftime('%s','now'))
         ON CONFLICT(account_id, message_id) DO UPDATE SET
            crypto_kind = excluded.crypto_kind,
            decrypt_state = excluded.decrypt_state,
            signature_state = excluded.signature_state,
            signer_fingerprint = excluded.signer_fingerprint,
            signer_email = excluded.signer_email,
            chain_valid = excluded.chain_valid,
            revocation_state = excluded.revocation_state,
            verified_at = excluded.verified_at",
        rusqlite::params![
            account_id,
            message_id,
            verdict.crypto_kind,
            verdict.decrypt_state,
            signature_state_db(verdict.signature_state),
            verdict.signer_fingerprint,
            verdict.signer_email,
        ],
    );
}

#[cfg(test)]
#[path = "receive_tests.rs"]
mod receive_tests;
