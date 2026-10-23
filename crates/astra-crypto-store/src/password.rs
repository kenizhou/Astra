// SPDX-License-Identifier: GPL-3.0-only
//! The W9 password mode: kylins' `apply_password_crypto` ported. Password
//! mode encrypts the message with an OpenPGP SYMMETRIC message (RFC 4880
//! SKESK + SEIPDv1, the `gpg --symmetric` shape) instead of recipient
//! public keys: any PGP-capable recipient can decrypt with the passphrase,
//! communicated out-of-band. The passphrase is held ONLY in memory, is
//! never logged, and is never persisted to SQLite.
//!
//! Lives in the store crate (not core) because the symmetric op is an
//! inherent method on the concrete OpenPGP backend, not the
//! `CryptoBackend` trait (the same placement kylins chose).

use astra_crypto_core::apply::{key_handle_ref, DefaultSigningKey, SendDraft};
use astra_crypto_core::mime::{hide_subject_header, split_message, wrap_protected_headers};
use astra_crypto_core::pgp_mime::wrap_encrypted;
use astra_crypto_core::{
    CryptoSendError, KeyHandleRef, Part, PartId, PartKind, SecretBox, SendCryptoPolicy,
    SerializationStrategy,
};
use astra_crypto_openpgp::OpenpgpBackend;
use secrecy::ExposeSecret;

/// W9 password mode: encrypt a built MIME message with OpenPGP SYMMETRIC
/// encryption (SKESK + SEIPDv1) using `passphrase`, framed as a standard
/// RFC 3156 `multipart/encrypted` via [`wrap_encrypted`] (a SKESK message
/// needs no framing change).
///
/// Differences from [`astra_crypto_core::apply_crypto`]'s encrypt arm (all
/// deliberate):
/// - **No recipient resolution.** No `find_by_email`, no trust gate, no
///   S/MIME cert-chain validation: password mode exists precisely for
///   recipients with no key. Encrypt-to-self does NOT apply either (there
///   is no recipient set at all; the sender reads their own Sent copy via
///   the passphrase they hold).
/// - **No Autocrypt-Gossip headers.** Gossip carries RECIPIENT public keys
///   inside the encrypted part; password mode has no recipient keys to
///   gossip.
/// - **Optional inline sign** with the account's default OpenPGP key (the
///   same `sign_with` composition as `apply_crypto`): recipients who have
///   the sender's public key can verify inside the symmetric ciphertext.
///
/// The protected-headers gate applies exactly as in `apply_crypto`
/// (password mode IS encryption: the subject is protected by default).
///
/// Fail-closed:
/// - `draft.encrypt && password mode` -> [`CryptoSendError::PasswordConflict`]
///   (mutually exclusive modes).
/// - empty passphrase -> [`CryptoSendError::PasswordRequired`] BEFORE any
///   ciphertext is produced.
/// - `draft.sign` with no default OpenPGP signing key ->
///   [`CryptoSendError::NoSigningKey`].
///
/// # Errors
/// [`CryptoSendError`] per the fail-closed arms above.
pub async fn apply_password_crypto(
    backend: &OpenpgpBackend,
    mime: &[u8],
    draft: &SendDraft,
    default_signing_key: Option<&DefaultSigningKey>,
    passphrase: SecretBox<String>,
    policy: &dyn SendCryptoPolicy,
) -> Result<Vec<u8>, CryptoSendError> {
    // Mutual exclusion (fail-closed): password mode is symmetric-ONLY
    // encryption; a draft requesting both modes is invalid.
    if draft.encrypt {
        return Err(CryptoSendError::PasswordConflict);
    }
    // Fail-closed: no ciphertext for an empty passphrase.
    if passphrase.expose_secret().is_empty() {
        return Err(CryptoSendError::PasswordRequired);
    }

    let (mut outer, mut entity) = split_message(mime)?;

    // Optional inline sign: same resolution as `apply_crypto`.
    let signer: Option<KeyHandleRef> = if draft.sign {
        let signer_row = default_signing_key.ok_or(CryptoSendError::NoSigningKey)?;
        Some(key_handle_ref(signer_row))
    } else {
        None
    };

    // The protected-headers gate: identical policy to `apply_crypto`'s
    // encrypt arm (default ON; read error defaults to ON, fail-safe).
    match policy.protect_subject().await {
        Ok(false) => {}
        Ok(true) => {
            let (new_outer, subject_block) = hide_subject_header(&outer.headers);
            outer.headers = new_outer;
            entity.0 = wrap_protected_headers(&entity.0, subject_block.as_deref(), draft.sign);
        }
        Err(e) => {
            log::warn!("[crypto] protect_subject read failed ({e}); defaulting to ON");
            let (new_outer, subject_block) = hide_subject_header(&outer.headers);
            outer.headers = new_outer;
            entity.0 = wrap_protected_headers(&entity.0, subject_block.as_deref(), draft.sign);
        }
    }

    let env = backend
        .encrypt_symmetric(
            &[Part {
                id: PartId("body".into()),
                kind: PartKind::Body,
                data: entity.0.clone(),
            }],
            SerializationStrategy::SingleMimeBlob,
            passphrase,
            signer.as_ref(),
        )
        .await?;
    let envelope_bytes = env
        .parts
        .first()
        .expect("encrypt_symmetric returns exactly one part")
        .ciphertext
        .clone();
    // RFC 3156 §2 framing: unchanged for SKESK messages.
    let wrapped_entity = wrap_encrypted(&envelope_bytes);
    let mut out = outer.headers.into_bytes();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(&wrapped_entity);
    Ok(out)
}

#[cfg(test)]
#[path = "password_tests.rs"]
mod password_tests;
