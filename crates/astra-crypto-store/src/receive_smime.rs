// SPDX-License-Identifier: GPL-3.0-only
//! The S/MIME receive orchestrator (SP4c-2 D2): the receive-side mirror of
//! [`crate::receive`]. Detects + unwraps the `application/pkcs7-mime` enveloped
//! framing, resolves the account's S/MIME private leaf, decrypts through
//! [`SmimeBackend::decrypt`], verifies the nested sign-then-encrypt SignedData
//! through [`SmimeBackend::verify_with_context`] with store-resolved inputs,
//! persists the per-message result with the chain fields, and restores the
//! protected subject.
//!
//! **Fail closed.** No key, a decrypt failure, or a tampered ciphertext never
//! yields plaintext or renders the ciphertext: the outcome carries the state
//! and empty legs. Only a successful decrypt releases the legs.
//!
//! **Privacy.** A message is the user's. Nothing here logs its content, its
//! recipients, or its subject; counts and ids only.

use std::sync::Arc;

use astra_crypto_core::{
    mime, smime_mime, CryptoBackend, CryptoError, CryptoPolicy, DecryptOp, DetachedSignature,
    EncryptedEnvelope, EncryptedPart, Fingerprint, KeyHandle, KeyHandleRef, KeyId, KeyUsage,
    PartId, PartKind, SerializationStrategy, SignatureState, SignedEnvelope, Standard, TokenKeyId,
    TrustState,
};
use astra_crypto_smime::{
    cert_summary_of_der, crl_distribution_point_urls, extract_intermediates,
    extract_signed_content, extract_signer_cert, fingerprint_of_cert_der, is_signed_data,
    SmimeBackend,
};

use crate::receive::{write_result, ResultVerdict};
use crate::{
    fetch_or_cache_crl, get_latest_trust_decision, insert_harvested_cert, list_crls,
    list_crypto_keys_for_account, list_intermediate_certs, list_trust_anchor_certs, CrlFetcher,
    CryptoKeyRow, CryptoStore, NoopCrlFetcher, ReceiveOutcome, ReceiveState, SqliteKeyStore,
};

/// Opens one incoming S/MIME enveloped message for `account_id`, persisting the
/// result row keyed by `(account_id, message_id)`. See the module docs for the
/// fail-closed contract.
///
/// The steps: detect + unwrap the `application/pkcs7-mime` framing; resolve the
/// account's private S/MIME leaf; decrypt through [`SmimeBackend::decrypt`];
/// for the nested sign-then-encrypt shape, extract the eContent and verify
/// through [`SmimeBackend::verify_with_context`] with the store-resolved inputs;
/// harvest the signer leaf; persist the result row; restore the protected
/// subject. A source that is not the crypto framing answers
/// [`ReceiveState::NotEncrypted`] and writes no row.
///
/// The offline entry: no CRL fetcher is attached, so revocation uses only the
/// already-cached CRLs (the [`NoopCrlFetcher`] default). The host reading path
/// calls [`receive_smime_with_fetcher`] to populate the cache first.
pub async fn receive_smime(
    store: &Arc<CryptoStore>,
    account_id: &str,
    message_id: &str,
    source: &[u8],
) -> ReceiveOutcome {
    receive_smime_with_fetcher(store, account_id, message_id, source, &NoopCrlFetcher).await
}

/// [`receive_smime`] with a CRL fetch port (SP4c-3b D2). Before the chain
/// verify, the signer leaf's and intermediates' distribution-point URLs are
/// extracted and each CRL is fetched-or-cached through `fetcher` (fail-soft:
/// a fetch failure degrades to unchecked and never blocks the open). The
/// refreshed cache then feeds the chain validation.
pub async fn receive_smime_with_fetcher(
    store: &Arc<CryptoStore>,
    account_id: &str,
    message_id: &str,
    source: &[u8],
    fetcher: &dyn CrlFetcher,
) -> ReceiveOutcome {
    // 1. Detect + unwrap the S/MIME enveloped framing. Anything else is left
    //    untouched.
    let Ok((_outer, entity)) = mime::split_message(source) else {
        return ReceiveOutcome::closed_with_standard(
            ReceiveState::NotEncrypted,
            SignatureState::NotSigned,
            Standard::Smime,
        );
    };
    let Some(ciphertext) = smime_mime::unwrap_enveloped(&entity.0) else {
        // Recognised S/MIME framing whose ciphertext will not extract fails
        // closed (it must never render untouched); anything else is left alone.
        if smime_mime::is_smime_enveloped(&entity.0) {
            write_result(
                store,
                account_id,
                message_id,
                failed_verdict(SignatureState::Invalid),
            );
            return ReceiveOutcome::closed_with_standard(
                ReceiveState::Failed,
                SignatureState::Invalid,
                Standard::Smime,
            );
        }
        return ReceiveOutcome::closed_with_standard(
            ReceiveState::NotEncrypted,
            SignatureState::NotSigned,
            Standard::Smime,
        );
    };

    // 2. Resolve the account's private S/MIME leaf keys (decryption candidates).
    //    The default encryption key leads; the rest follow as fallbacks.
    let Ok(rows) = list_crypto_keys_for_account(store, account_id, "smime") else {
        return ReceiveOutcome::closed_with_standard(
            ReceiveState::Failed,
            SignatureState::Invalid,
            Standard::Smime,
        );
    };
    let mut ranked: Vec<(bool, KeyHandleRef)> = rows
        .iter()
        .filter(|row| row.key_type == "cert" && row.has_private)
        .map(|row| (row.is_default_encrypt, handle_for_row(row)))
        .collect();
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    let candidates: Vec<KeyHandleRef> = ranked.into_iter().map(|(_, handle)| handle).collect();
    if candidates.is_empty() {
        write_result(store, account_id, message_id, no_key_verdict());
        return ReceiveOutcome::closed_with_standard(
            ReceiveState::NoKey,
            SignatureState::NotSigned,
            Standard::Smime,
        );
    }

    // 3. Decrypt, offering each candidate leaf. A "no matching recipient"
    //    Malformed means this key fits no RecipientInfo: the next candidate may
    //    fit. Any other error fails closed for every key.
    let backend = SmimeBackend::new(
        Arc::new(SqliteKeyStore::new(store.clone(), account_id)),
        CryptoPolicy::default(),
    );
    let envelope = EncryptedEnvelope {
        standard: Standard::Smime,
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
        let op = DecryptOp {
            envelope: &envelope,
            decryption_key: handle.clone(),
        };
        match backend.decrypt(op).await {
            Ok(payload) => {
                decrypted = Some(payload);
                break;
            }
            // A wrong key fits no RecipientInfo: the next candidate may fit.
            Err(CryptoError::Malformed(msg)) if msg.contains("no matching recipient") => {}
            // A tampered ciphertext or a bad key fails for every candidate.
            Err(_) => break,
        }
    }
    let Some(payload) = decrypted else {
        write_result(
            store,
            account_id,
            message_id,
            failed_verdict(SignatureState::Invalid),
        );
        return ReceiveOutcome::closed_with_standard(
            ReceiveState::Failed,
            SignatureState::Invalid,
            Standard::Smime,
        );
    };
    let plaintext = payload
        .parts
        .iter()
        .find_map(|part| matches!(part.kind, PartKind::Body).then(|| part.data.clone()))
        .unwrap_or_default();

    // 4. The nested sign-then-encrypt shape: the plaintext is itself an opaque
    //    SignedData. Extract its eContent (the inner MIME entity) and verify.
    //    Encrypt-only plaintext is the inner entity directly (NotSigned).
    let from_email = mime::from_email(source);
    let signed = is_signed_data(&plaintext);
    let (inner_entity, verification) = if signed {
        // A SignedData whose eContent will not extract fails closed.
        let Ok(econtent) = extract_signed_content(&plaintext) else {
            write_result(
                store,
                account_id,
                message_id,
                failed_verdict(SignatureState::Invalid),
            );
            return ReceiveOutcome::closed_with_standard(
                ReceiveState::Failed,
                SignatureState::Invalid,
                Standard::Smime,
            );
        };
        let outcome = verify_signed(
            &backend,
            store,
            account_id,
            &plaintext,
            &econtent,
            from_email.as_deref(),
            &rows,
            fetcher,
        )
        .await;
        (econtent, Some(outcome))
    } else {
        (plaintext, None)
    };

    // 5. Restore the protected subject and extract the decrypted legs.
    let (subject, legs_entity) = match mime::unwrap_protected_headers(&inner_entity) {
        Some((subject, body)) => (subject, body),
        None => (None::<String>, inner_entity),
    };
    let (plain, html) = mime::body_legs(&legs_entity);

    // 6. Map the verification to the result row + outcome. Encrypt-only is
    //    NotSigned with no chain assessment.
    let (
        signature_state,
        signer_fingerprint,
        signer_email,
        chain_valid,
        revocation_state,
        failure_reason,
        revocation_reason,
    ) = match verification {
        Some(v) => (
            v.state,
            v.signer_fingerprint,
            v.signer_email,
            v.chain_valid,
            v.revocation_state,
            v.failure_reason,
            v.revocation_reason,
        ),
        None => (
            SignatureState::NotSigned,
            None,
            None,
            None,
            "unchecked".to_string(),
            None,
            None,
        ),
    };

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
            standard: Some("smime"),
            chain_valid,
            revocation_state: &revocation_state,
            failure_reason: failure_reason.as_deref(),
            revocation_reason: revocation_reason.as_deref(),
        },
    );

    ReceiveOutcome {
        state: ReceiveState::Decrypted,
        signature_state,
        signer_fingerprint,
        signer_email,
        weak_warning: None,
        subject,
        plain,
        html,
        standard: Standard::Smime,
        chain_valid,
        revocation_state: Some(revocation_state),
        failure_reason,
        revocation_reason,
    }
}

/// The resolved verify outcome: the signature state plus the chain fields the
/// result row and the outcome carry.
struct Verified {
    state: SignatureState,
    signer_fingerprint: Option<String>,
    signer_email: Option<String>,
    chain_valid: Option<bool>,
    revocation_state: String,
    failure_reason: Option<String>,
    revocation_reason: Option<String>,
}

/// Verify the nested SignedData through [`SmimeBackend::verify_with_context`]
/// with the store-resolved inputs, harvest the signer leaf, and map the state
/// ladder. The chain fields derive from the verdict: `Invalid` → chain invalid,
/// the valid states → chain valid, `UnknownKey` → no chain assessment.
#[allow(clippy::too_many_arguments)]
async fn verify_signed(
    backend: &SmimeBackend,
    store: &Arc<CryptoStore>,
    account_id: &str,
    signed_data_der: &[u8],
    econtent: &[u8],
    from_email: Option<&str>,
    own_rows: &[CryptoKeyRow],
    fetcher: &dyn CrlFetcher,
) -> Verified {
    // The signer leaf: its fingerprint resolves the trust decision, and its DER
    // is harvested (never auto-trusted). A leaf that will not extract degrades
    // to UnknownKey (no signer to verify against).
    let leaf_der = extract_signer_cert(signed_data_der).ok();
    let signer_fp = leaf_der
        .as_deref()
        .and_then(|der| fingerprint_of_cert_der(der).ok());

    // The signer's email: the cert SAN, falling back to the From address.
    let signer_email = leaf_der
        .as_deref()
        .and_then(|der| cert_summary_of_der(der).ok())
        .and_then(|summary| summary.email)
        .or_else(|| from_email.map(str::to_string));

    // Resolve the verify inputs from the store.
    let anchors = list_trust_anchor_certs(store, account_id).unwrap_or_default();
    let intermediates = merge_intermediates(store, account_id, signed_data_der);

    // SP4c-3b CRL hook: extract the distribution-point URLs from the signer
    // leaf + intermediates, fetch-or-cache each (fail-soft), then the refreshed
    // cache feeds the chain. Every step is best-effort: a parse or fetch failure
    // degrades to unchecked and never blocks the open.
    refresh_crl_cache(store, fetcher, leaf_der.as_deref(), &intermediates).await;

    let crls = list_crls(store).unwrap_or_default();
    let signer_trust = resolve_signer_trust(
        store,
        account_id,
        from_email,
        signer_fp.as_deref(),
        own_rows,
    );

    // Harvest the signer leaf (public only, origin='harvested', no trust
    // decision). Best-effort: a harvest failure never blocks the verdict.
    if let (Some(der), Some(email)) = (leaf_der.as_deref(), signer_email.as_deref()) {
        let _ = insert_harvested_cert(store, account_id, email, der, None);
    }

    // The full receive verifier. A parse failure degrades to Invalid (the
    // SignedData is malformed); the chain assessment never runs.
    let signed = SignedEnvelope {
        standard: Standard::Smime,
        payload: econtent.to_vec(),
        signature: DetachedSignature {
            standard: Standard::Smime,
            signer: KeyHandleRef {
                handle: KeyHandle::Software(KeyId(format!(
                    "smime|{}",
                    signer_fp.clone().unwrap_or_default()
                ))),
                standard: Standard::Smime,
                fingerprint: Fingerprint::new(signer_fp.clone().unwrap_or_default()),
                usage: KeyUsage::SignAndEncrypt,
                algorithm: "ECDSA-P256".into(),
            },
            signature: signed_data_der.to_vec(),
        },
    };
    let Ok(result) = backend
        .verify_with_context(
            &signed,
            from_email,
            &anchors,
            &intermediates,
            &crls,
            signer_trust,
        )
        .await
    else {
        return Verified {
            state: SignatureState::Invalid,
            signer_fingerprint: signer_fp,
            signer_email,
            chain_valid: Some(false),
            revocation_state: "unchecked".to_string(),
            failure_reason: None,
            revocation_reason: None,
        };
    };

    // The state ladder → the chain fields. Invalid covers both a chain failure
    // and a pre-chain signature failure; the valid states all chain-validated.
    let chain_valid = match result.state {
        SignatureState::Invalid => Some(false),
        SignatureState::Mismatch
        | SignatureState::ValidVerified
        | SignatureState::ValidUnverified => Some(true),
        _ => None,
    };
    // The persisted state marks a chain revocation hard-fail as revoked;
    // otherwise it reads unchecked (the Good/Stale granularity is a follow-up).
    let revocation_state = if result.revocation_reason.is_some() {
        "revoked"
    } else {
        "unchecked"
    };

    Verified {
        state: result.state,
        signer_fingerprint: signer_fp,
        signer_email,
        chain_valid,
        revocation_state: revocation_state.to_string(),
        failure_reason: result.failure_reason,
        revocation_reason: result.revocation_reason,
    }
}

/// Populate the CRL cache for the signer chain (SP4c-3b D2). Extracts the
/// distribution-point URLs from the signer leaf and every intermediate,
/// deduplicates them, and fetches-or-caches each through the port at the
/// current wall-clock instant. Fail-soft throughout: a cert that will not
/// parse contributes no URLs, and a fetch failure leaves the cache as-is, so
/// the message open is never blocked. The chain validates as-of signing time,
/// so a freshly fetched CRL is still checked against the signing instant.
async fn refresh_crl_cache(
    store: &Arc<CryptoStore>,
    fetcher: &dyn CrlFetcher,
    leaf_der: Option<&[u8]>,
    intermediates: &[Vec<u8>],
) {
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| "0".to_string(), |d| d.as_secs().to_string());

    let mut seen = std::collections::HashSet::new();
    let certs = leaf_der
        .into_iter()
        .chain(intermediates.iter().map(Vec::as_slice));
    for der in certs {
        let Ok(urls) = crl_distribution_point_urls(der) else {
            continue;
        };
        for url in urls {
            if seen.insert(url.clone()) {
                fetch_or_cache_crl(store, fetcher, &url, &now_epoch).await;
            }
        }
    }
}

/// Merge the stored intermediates with the SignedData's embedded cert set,
/// deduped by the shared SHA-1-of-SPKI fingerprint so a cert carried in both
/// sets joins the chain once.
fn merge_intermediates(
    store: &Arc<CryptoStore>,
    account_id: &str,
    signed_data_der: &[u8],
) -> Vec<Vec<u8>> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let stored = list_intermediate_certs(store, account_id).unwrap_or_default();
    let embedded = extract_intermediates(signed_data_der).unwrap_or_default();
    for der in stored.into_iter().chain(embedded) {
        if let Ok(fp) = fingerprint_of_cert_der(&der) {
            if seen.insert(fp) {
                out.push(der);
            }
        }
    }
    out
}

/// Resolve the signer's trust state: the latest trust decision for the
/// `(from_email, fingerprint)` bucket, or `Personal` when the signer is the
/// account's own private key. No decision degrades to `Undecided` (which
/// displays as unverified, never verified).
fn resolve_signer_trust(
    store: &Arc<CryptoStore>,
    account_id: &str,
    from_email: Option<&str>,
    signer_fp: Option<&str>,
    own_rows: &[CryptoKeyRow],
) -> TrustState {
    let Some(fp) = signer_fp else {
        return TrustState::Undecided;
    };
    // Personal for the account's own key (a sent-mail copy signed by us).
    if own_rows
        .iter()
        .any(|row| row.has_private && row.key_type == "cert" && row.fingerprint == fp)
    {
        return TrustState::Personal;
    }
    let decision = from_email
        .and_then(|email| get_latest_trust_decision(store, account_id, email, "smime", fp).ok())
        .flatten();
    match decision.map(|row| row.decision).as_deref() {
        Some("verified") => TrustState::Verified,
        Some("personal") => TrustState::Personal,
        Some("unverified") => TrustState::Unverified,
        Some("rejected") => TrustState::Rejected,
        _ => TrustState::Undecided,
    }
}

/// Build the canonical keystore handle for an S/MIME key row, the shape
/// `SqliteKeyStore::get` resolves (`smime|{fingerprint}`).
fn handle_for_row(row: &CryptoKeyRow) -> KeyHandleRef {
    // A token-held key routes to the W6 token decrypt path, exactly the
    // OpenPGP mirror; offering it as Software would fail (no private blob)
    // and abort the candidate fallback before a usable software key is tried.
    let handle = match (&row.token_serial, &row.token_key_id) {
        (Some(serial), Some(key_id)) => KeyHandle::Token {
            token_serial: serial.clone(),
            key_id: TokenKeyId(key_id.clone()),
        },
        _ => KeyHandle::Software(KeyId(format!("smime|{}", row.fingerprint))),
    };
    KeyHandleRef {
        handle,
        standard: Standard::Smime,
        fingerprint: Fingerprint::new(&row.fingerprint),
        usage: KeyUsage::SignAndEncrypt,
        algorithm: "unknown".to_string(),
    }
}

/// The fail-closed `failed` verdict (no chain assessment ran).
fn failed_verdict(signature_state: SignatureState) -> ResultVerdict<'static> {
    ResultVerdict {
        crypto_kind: "encrypted",
        decrypt_state: "failed",
        signature_state,
        signer_fingerprint: None,
        signer_email: None,
        standard: Some("smime"),
        chain_valid: None,
        revocation_state: "unchecked",
        failure_reason: None,
        revocation_reason: None,
    }
}

/// The fail-closed `no-key` verdict.
fn no_key_verdict() -> ResultVerdict<'static> {
    ResultVerdict {
        crypto_kind: "encrypted",
        decrypt_state: "no-key",
        signature_state: SignatureState::NotSigned,
        signer_fingerprint: None,
        signer_email: None,
        standard: Some("smime"),
        chain_valid: None,
        revocation_state: "unchecked",
        failure_reason: None,
        revocation_reason: None,
    }
}

#[cfg(test)]
#[path = "receive_smime_tests.rs"]
mod receive_smime_tests;
