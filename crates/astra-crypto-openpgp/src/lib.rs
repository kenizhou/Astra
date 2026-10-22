// SPDX-License-Identifier: GPL-3.0-only
//! OpenPGP crypto backend (Sequoia engine). Implements `astra_crypto_core::CryptoBackend`.
//!
//! Task 8 wiring: [`OpenpgpBackend`] adapts the Sequoia engine (Tasks 5-7) to
//! the framework's [`astra_crypto_core::CryptoBackend`] trait via the [`KeyStore`]
//! abstraction. Heavy Sequoia ops run inside [`tokio::task::spawn_blocking`] so
//! the async runtime's worker threads are not blocked by CPU-bound crypto.
//!
//! ## `KeyHandleRef` → `Cert` resolution
//!
//! Every op resolves its [`KeyHandleRef`]s to [`openpgp::Cert`]s via
//! `self.keystore`: `keystore.get(&ref.handle).await?` → `Option<StoredKey>`
//! → [`keymap::parse_certs`] → `Cert`. The PRIVATE blob is preferred for ops
//! that need secret material (decrypt, sign, inline-sign-then-encrypt); the
//! PUBLIC blob suffices for recipient-encrypt + verify. A `None` keystore hit
//! surfaces as [`CryptoError::KeyNotFound`].
//!
//! ## `spawn_blocking` rationale
//!
//! Sequoia's encrypt/decrypt/sign/verify are CPU-bound (public-key crypto +
//! hashing). Running them on a tokio worker thread would degrade the runtime's
//! ability to make progress on other futures (notably, sequential IPC calls).
//! We therefore wrap each heavy op in `tokio::task::spawn_blocking`. To make
//! the closure `'static`, the resolved `Cert`s and a cloned `Arc<PgpPolicy>`
//! are MOVED into the closure (no `&self` borrows cross the boundary).
//!
//! [`CryptoError::KeyNotFound`]: astra_crypto_core::CryptoError::KeyNotFound

pub mod engine;
pub mod error;
pub mod keymap;
pub mod policy;

// W6: Sequoia Signer/Decryptor shims over astra_crypto_core::TokenProvider
// (hardware-token / OpenPGP-card keys).
pub mod card;

// W6: card-routing E2E tests (MockTokenProvider - no hardware).
#[cfg(test)]
mod card_tests;

use astra_crypto_core::{
    CryptoBackend, CryptoError, CryptoPolicy, DecryptOp, DecryptedPayload, EncryptOp,
    EncryptedEnvelope, KeyGenParams, KeyHandle, KeyHandleRef, KeyId, KeyStore, Result, SecretBox,
    SignOp, SignedEnvelope, Standard, StoredKey, VerificationResult, VerifyOp,
};
use async_trait::async_trait;
use secrecy::ExposeSecret;
use sequoia_openpgp as openpgp;
use std::sync::Arc;

/// OpenPGP `CryptoBackend` adapter.
///
/// Fields:
/// - `keystore`: shared key-material store; resolves `KeyHandleRef` → `Cert`.
/// - `core_policy`: the framework-wide algorithm policy, returned by
///   [`CryptoBackend::policy`].
/// - `pgp_policy`: Sequoia-engine policy bundle (write/read policies + weak-algo
///   detector), derived from `core_policy` once at construction. Held in
///   `Arc` so a cheap clone can be moved into each `spawn_blocking` closure
///   without copying the underlying `StandardPolicy` state.
/// - `token_provider` (W6): optional hardware-token provider. `KeyHandle::Token`
///   keys route sign/decrypt through [`card::CardSigner`] / [`card::CardDecryptor`]
///   shims; software keys are unaffected. A token handle with no provider
///   attached fails fast with [`CryptoError::TokenNotPresent`].
///
/// [`CryptoError::TokenNotPresent`]: astra_crypto_core::CryptoError::TokenNotPresent
pub struct OpenpgpBackend {
    keystore: Arc<dyn KeyStore>,
    core_policy: CryptoPolicy,
    pgp_policy: Arc<policy::PgpPolicy>,
    token_provider: Option<Arc<dyn astra_crypto_core::TokenProvider>>,
}

impl OpenpgpBackend {
    /// Construct a new backend over `keystore` with the framework-wide
    /// `core_policy`. The PGP policy bundle is derived from `core_policy` once
    /// (write/read policies + weak-algo detector) and shared across all calls.
    pub fn new(keystore: Arc<dyn KeyStore>, core_policy: CryptoPolicy) -> Self {
        let pgp_policy = Arc::new(policy::PgpPolicy::from_core(&core_policy));
        Self {
            keystore,
            core_policy,
            pgp_policy,
            token_provider: None,
        }
    }

    /// Attach a hardware-token provider (W6). Builder-style, mirroring
    /// `SmimeBackend::with_token_provider`.
    pub fn with_token_provider(
        mut self,
        provider: Arc<dyn astra_crypto_core::TokenProvider>,
    ) -> Self {
        self.token_provider = Some(provider);
        self
    }

    /// Resolve the token provider or fail typed (see `SmimeBackend`'s
    /// equivalent - a token handle without a provider is an actionable
    /// `TokenNotPresent`, not a generic backend error).
    fn require_token_provider(&self) -> Result<&Arc<dyn astra_crypto_core::TokenProvider>> {
        self.token_provider.as_ref().ok_or_else(|| {
            CryptoError::TokenNotPresent(
                "no hardware-token provider configured (token support unavailable)".into(),
            )
        })
    }

    /// Build a card-backed signer for a `KeyHandle::Token` signing key: pick
    /// the cert's signing (sub)key WITHOUT the secret-material filter (token
    /// certs carry none) and wrap it in a [`card::CardSigner`]. Returns the
    /// signer plus the typed-error slot the caller checks on engine failure.
    fn card_signer_for(
        &self,
        cert: &openpgp::Cert,
        token_serial: &str,
        key_id: &astra_crypto_core::TokenKeyId,
    ) -> Result<(card::CardSigner, card::CardErrorSlot)> {
        let provider = self.require_token_provider()?.clone();
        let write_policy = self.pgp_policy.write_policy();
        let ka = cert
            .keys()
            .with_policy(write_policy, None)
            .supported()
            .alive()
            .revoked(false)
            .for_signing()
            .next()
            .ok_or_else(|| {
                CryptoError::Policy("token sign: cert has no usable signing subkey".into())
            })?;
        let slot = card::CardErrorSlot::default();
        let signer = card::CardSigner::new(
            ka.key().clone(),
            provider,
            astra_crypto_core::TokenKeyRef::new(token_serial, key_id.clone()),
            slot.clone(),
        )?;
        Ok((signer, slot))
    }

    /// Build a card-backed decryptor for a `KeyHandle::Token` decryption key
    /// (transport-encryption subkey, no secret filter - see
    /// [`Self::card_signer_for`]).
    fn card_decryptor_for(
        &self,
        cert: &openpgp::Cert,
        token_serial: &str,
        key_id: &astra_crypto_core::TokenKeyId,
    ) -> Result<(card::CardDecryptor, card::CardErrorSlot)> {
        let provider = self.require_token_provider()?.clone();
        let read_policy = self.pgp_policy.read_policy();
        let ka = cert
            .keys()
            .with_policy(read_policy, None)
            .supported()
            .alive()
            .revoked(false)
            .for_transport_encryption()
            .next()
            .ok_or_else(|| {
                CryptoError::NoMatchingKey(
                    "token decrypt: cert has no usable transport-encryption subkey".into(),
                )
            })?;
        let slot = card::CardErrorSlot::default();
        let decryptor = card::CardDecryptor::new(
            ka.key().clone(),
            provider,
            astra_crypto_core::TokenKeyRef::new(token_serial, key_id.clone()),
            slot.clone(),
        )?;
        Ok((decryptor, slot))
    }

    /// Resolve a [`KeyHandleRef`] to an [`openpgp::Cert`] via the keystore.
    ///
    /// When `prefer_private` is true, the private blob is preferred (ops that
    /// need secret material - decrypt, sign). When false, the public blob is
    /// used (encrypt recipients, verify signers). If the keystore has no entry
    /// for `handle.handle`, returns [`CryptoError::KeyNotFound`].
    ///
    /// When `prefer_private=true` but the `StoredKey` is public-only
    /// (`private_data == None`), this falls back to the public blob; the engine
    /// call then fails with a clear "no usable signing/encryption subkey"
    /// error - surfaced as [`CryptoError::NoMatchingKey`] for the decrypt path
    /// (wrong-key condition) and as [`CryptoError::Backend`] elsewhere.
    ///
    /// [`CryptoError::Backend`]: astra_crypto_core::CryptoError::Backend
    /// [`CryptoError::KeyNotFound`]: astra_crypto_core::CryptoError::KeyNotFound
    /// [`CryptoError::NoMatchingKey`]: astra_crypto_core::CryptoError::NoMatchingKey
    async fn resolve_cert(
        &self,
        handle: &KeyHandleRef,
        prefer_private: bool,
    ) -> Result<openpgp::Cert> {
        let stored = self.keystore.get(&handle.handle).await?.ok_or_else(|| {
            CryptoError::KeyNotFound(format!("OpenPGP key not found: {}", handle.fingerprint))
        })?;

        let blob: Vec<u8> = if prefer_private {
            stored
                .private_data
                .as_ref()
                .map(|p| p.expose_secret().clone())
                .unwrap_or_else(|| stored.public_data.clone())
        } else {
            stored.public_data.clone()
        };

        let certs = keymap::parse_certs(&blob)?;
        certs.into_iter().next().ok_or_else(|| {
            CryptoError::Malformed(format!(
                "OpenPGP blob for {} contained no certs",
                handle.fingerprint
            ))
        })
    }

    /// Persist a freshly-generated or freshly-imported `Cert` to the keystore
    /// as a [`StoredKey`] (public blob + private blob) and return the canonical
    /// [`KeyHandleRef`] (the one the keystore returns from `put`, which must
    /// match [`keymap::cert_to_handle`]).
    async fn persist_cert(&self, cert: openpgp::Cert) -> Result<KeyHandleRef> {
        let handle = keymap::cert_to_handle(&cert);
        let public_data = keymap::cert_to_public_blob(&cert)?;
        let private_data = keymap::cert_to_secret_blob(&cert)?;
        let stored = StoredKey {
            handle: handle.clone(),
            public_data,
            private_data: Some(SecretBox::new(Box::new(private_data))),
        };
        // `put` returns the canonical handle; for the memory store (and any
        // sane SQLite-backed implementation) this equals `handle`. The returned
        // reference is what the caller retains.
        self.keystore.put(stored).await
    }
}

// =========================================================================
// W9 - symmetric (SKESK) ops, OpenpgpBackend-concrete
// =========================================================================
//
// Password-protected mail is OpenPGP-only: S/MIME has no symmetric mode, so
// these are CONCRETE methods on `OpenpgpBackend`, not `CryptoBackend` trait
// methods (adding them to the trait would force a meaningless
// `UnsupportedStandard` stub onto every other backend and widen the trait
// for a single-standard feature). Callers that need symmetric ops construct
// `OpenpgpBackend` directly (the same pattern the S/MIME-only `.p12` paths
// use with `SmimeBackend::import_key_with_chain`).
impl OpenpgpBackend {
    /// Symmetrically encrypt `parts` with `passphrase` (OpenPGP SKESK +
    /// SEIPDv1 - see [`engine::encrypt_symmetric`]'s interop docs).
    ///
    /// `sign_with` (optional) resolves to the signer cert's PRIVATE blob via
    /// the keystore, exactly like the PKESK `encrypt` op's signer resolution.
    ///
    /// The passphrase enters as a [`SecretBox<String>`] and is converted to
    /// Sequoia's zeroizing `Password` inside the engine; it is never logged
    /// and never persisted.
    pub async fn encrypt_symmetric(
        &self,
        parts: &[astra_crypto_core::Part],
        serialization: astra_crypto_core::SerializationStrategy,
        passphrase: SecretBox<String>,
        sign_with: Option<&KeyHandleRef>,
    ) -> Result<EncryptedEnvelope> {
        // Resolve the optional signer cert (private blob needed for the
        // signing keypair) - same as `CryptoBackend::encrypt`. W6: a token
        // signer is built as a card shim instead (public blob only).
        let mut token_signer: Option<(
            Box<dyn openpgp::crypto::Signer + Send + Sync>,
            card::CardErrorSlot,
        )> = None;
        let signer_cert = match sign_with {
            Some(s) => {
                if let KeyHandle::Token {
                    token_serial,
                    key_id,
                } = &s.handle
                {
                    let cert = self.resolve_cert(s, /* prefer_private */ false).await?;
                    let (signer, slot) = self.card_signer_for(&cert, token_serial, key_id)?;
                    token_signer = Some((Box::new(signer), slot));
                    Some(cert)
                } else {
                    Some(self.resolve_cert(s, /* prefer_private */ true).await?)
                }
            }
            None => None,
        };

        let parts: Vec<_> = parts.to_vec();
        let pgp_policy = self.pgp_policy.clone();

        // CPU-bound Sequoia encrypt off the tokio worker thread. The
        // passphrase MOVES into the closure (SecretBox is Send; zeroized on
        // drop inside the engine after conversion to Sequoia's Password).
        match token_signer {
            Some((signer, slot)) => {
                let result = tokio::task::spawn_blocking(move || {
                    engine::encrypt_symmetric_with_signer(
                        &parts,
                        serialization,
                        passphrase,
                        signer,
                        &pgp_policy,
                    )
                })
                .await
                .map_err(map_join_err)?;
                match result {
                    Ok(env) => Ok(env),
                    Err(e) => Err(slot.take().unwrap_or(e)),
                }
            }
            None => tokio::task::spawn_blocking(move || {
                engine::encrypt_symmetric(
                    &parts,
                    serialization,
                    passphrase,
                    signer_cert.as_ref(),
                    &pgp_policy,
                )
            })
            .await
            .map_err(map_join_err)?,
        }
    }

    /// Resolve candidate signer certs for embedded-signature verification:
    /// every `known_signers` handle resolvable via the keystore PLUS every
    /// parseable blob in `extra_signer_blobs` (material that lives OUTSIDE
    /// the keystore - e.g. the backend's `collected_keys` staging area).
    /// Best-effort, warn-and-skip per candidate (mirrors `verify`'s F2
    /// resolution): a keystore error or unparseable blob on one candidate
    /// must not mask another. De-duped by cert fingerprint.
    async fn resolve_signer_certs(
        &self,
        known_signers: &[KeyHandleRef],
        extra_signer_blobs: &[Vec<u8>],
    ) -> Vec<openpgp::Cert> {
        let mut certs: Vec<openpgp::Cert> = Vec::new();
        let mut seen_fps: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut push = |cert: openpgp::Cert, certs: &mut Vec<openpgp::Cert>| {
            if seen_fps.insert(cert.fingerprint().to_hex()) {
                certs.push(cert);
            }
        };
        for handle in known_signers {
            let stored = match self.keystore.get(&handle.handle).await {
                Ok(Some(stored)) => stored,
                Ok(None) => continue,
                Err(e) => {
                    log::warn!("OpenpgpBackend: signer candidate lookup failed: {e}");
                    continue;
                }
            };
            match keymap::parse_certs(&stored.public_data) {
                Ok(parsed) => {
                    if let Some(cert) = parsed.into_iter().next() {
                        push(cert, &mut certs);
                    }
                }
                Err(e) => {
                    log::warn!("OpenpgpBackend: parse signer candidate failed: {e}");
                }
            }
        }
        for blob in extra_signer_blobs {
            match keymap::parse_certs(blob) {
                Ok(parsed) => {
                    for cert in parsed {
                        push(cert, &mut certs);
                    }
                }
                Err(e) => {
                    log::warn!("OpenpgpBackend: parse extra signer blob failed: {e}");
                }
            }
        }
        certs
    }

    /// Symmetrically decrypt `envelope` with `passphrase` (OpenPGP SKESK).
    ///
    /// `known_signers` (+ `extra_signer_blobs`) are offered to Sequoia for
    /// embedded-signature verification (see [`Self::resolve_signer_certs`]).
    /// A wrong passphrase surfaces as the typed
    /// [`astra_crypto_core::CryptoError::WrongPassphrase`]; an unverifiable
    /// embedded signature (unknown signer) DEGRADES - the returned
    /// [`engine::DecryptOutcome`] carries `unverifiable_signature: true`.
    pub async fn decrypt_symmetric(
        &self,
        envelope: &EncryptedEnvelope,
        passphrase: SecretBox<String>,
        known_signers: &[KeyHandleRef],
        extra_signer_blobs: &[Vec<u8>],
    ) -> Result<engine::DecryptOutcome> {
        let signer_certs = self
            .resolve_signer_certs(known_signers, extra_signer_blobs)
            .await;

        let envelope = envelope.clone();
        let pgp_policy = self.pgp_policy.clone();

        tokio::task::spawn_blocking(move || {
            engine::decrypt_symmetric(&envelope, passphrase, &signer_certs, &pgp_policy)
        })
        .await
        .map_err(map_join_err)?
    }

    /// PKESK decrypt with embedded-signature verification candidates
    /// (`known_signers` + `extra_signer_blobs` - see
    /// [`Self::resolve_signer_certs`]). This is the receive path's entry
    /// point: the framework offers the account's keystore + collected-keys
    /// certs so a cross-cert sign-then-encrypt message verifies its embedded
    /// signature instead of degrading to `unverifiable_signature`.
    ///
    /// The [`CryptoBackend::decrypt`] trait impl delegates here with EMPTY
    /// signer sets (decryption-cert-only verification, the historical
    /// behavior).
    pub async fn decrypt_with_signers(
        &self,
        envelope: &EncryptedEnvelope,
        decryption_key: &KeyHandleRef,
        known_signers: &[KeyHandleRef],
        extra_signer_blobs: &[Vec<u8>],
    ) -> Result<engine::DecryptOutcome> {
        let signer_certs = self
            .resolve_signer_certs(known_signers, extra_signer_blobs)
            .await;

        // W6: token-held decryption key - route through the card decryptor
        // shim. The cert is public-only; the PKESK private op runs on-card.
        if let KeyHandle::Token {
            token_serial,
            key_id,
        } = &decryption_key.handle
        {
            let decryption_cert = self
                .resolve_cert(decryption_key, /* prefer_private */ false)
                .await?;
            let (decryptor, slot) =
                self.card_decryptor_for(&decryption_cert, token_serial, key_id)?;
            let envelope = envelope.clone();
            let pgp_policy = self.pgp_policy.clone();
            let result = tokio::task::spawn_blocking(move || {
                engine::decrypt_with_token(
                    &envelope,
                    &decryption_cert,
                    &signer_certs,
                    Box::new(decryptor),
                    &pgp_policy,
                )
            })
            .await
            .map_err(map_join_err)?;
            return match result {
                Ok(outcome) => Ok(outcome),
                Err(e) => Err(slot.take().unwrap_or(e)),
            };
        }

        let decryption_cert = self
            .resolve_cert(decryption_key, /* prefer_private */ true)
            .await?;

        // Clone the envelope so the closure is 'static. The trait passes
        // `&envelope` tied to the caller's lifetime; `spawn_blocking` cannot
        // borrow from it.
        let envelope = envelope.clone();
        let pgp_policy = self.pgp_policy.clone();

        tokio::task::spawn_blocking(move || {
            engine::decrypt(&envelope, &decryption_cert, &signer_certs, &pgp_policy)
        })
        .await
        .map_err(map_join_err)?
    }
}

/// Wrapper to give `tokio::task::JoinError` a `Display + std::error::Error`
/// impl that incorporates the panic payload's message. (JoinError implements
/// `Error` directly, but its `Display` is terse - wrapping here lets us add
/// the "spawn_blocking task" context the framework logs benefit from.)
#[derive(Debug)]
struct JoinErrorWrapper(tokio::task::JoinError);

impl std::fmt::Display for JoinErrorWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "spawn_blocking task failed: {}", self.0)
    }
}

impl std::error::Error for JoinErrorWrapper {}

/// Map a `tokio::task::JoinError` (panic in a spawned task) to a
/// [`CryptoError::Backend`]. Centralized so every `spawn_blocking` site has
/// the same panic-surfacing shape.
fn map_join_err(e: tokio::task::JoinError) -> CryptoError {
    CryptoError::backend(JoinErrorWrapper(e))
}

#[async_trait]
impl CryptoBackend for OpenpgpBackend {
    fn standard(&self) -> Standard {
        Standard::OpenPgp
    }

    fn policy(&self) -> &CryptoPolicy {
        &self.core_policy
    }

    async fn encrypt(&self, op: EncryptOp<'_>) -> Result<EncryptedEnvelope> {
        // Resolve recipient certs (public blob is sufficient for encryption).
        // Collect into an owned Vec so the spawn_blocking closure is 'static.
        let mut recipient_certs = Vec::with_capacity(op.recipients.len());
        for r in op.recipients {
            recipient_certs.push(self.resolve_cert(r, /* prefer_private */ false).await?);
        }

        // W6: token-held inline signer - build the card shim up-front; the
        // engine's token signer entry point consumes it. Software signers
        // keep the cert-resolution path.
        let mut token_signer: Option<(
            Box<dyn openpgp::crypto::Signer + Send + Sync>,
            card::CardErrorSlot,
        )> = None;
        let signer_cert = match op.sign_with.as_ref() {
            Some(s) => {
                if let KeyHandle::Token {
                    token_serial,
                    key_id,
                } = &s.handle
                {
                    let cert = self.resolve_cert(s, /* prefer_private */ false).await?;
                    let (signer, slot) = self.card_signer_for(&cert, token_serial, key_id)?;
                    token_signer = Some((Box::new(signer), slot));
                    Some(cert)
                } else {
                    Some(self.resolve_cert(s, /* prefer_private */ true).await?)
                }
            }
            None => None,
        };

        let parts: Vec<_> = op.parts.to_vec();
        let serialization = op.serialization;
        let pgp_policy = self.pgp_policy.clone();

        // Run CPU-bound Sequoia encrypt off the tokio worker thread.
        match token_signer {
            Some((signer, slot)) => {
                let result = tokio::task::spawn_blocking(move || {
                    engine::encrypt_with_signer(
                        &parts,
                        serialization,
                        &recipient_certs,
                        signer,
                        &pgp_policy,
                    )
                })
                .await
                .map_err(map_join_err)?;
                match result {
                    Ok(env) => Ok(env),
                    Err(e) => Err(slot.take().unwrap_or(e)),
                }
            }
            None => tokio::task::spawn_blocking(move || {
                engine::encrypt(
                    &parts,
                    serialization,
                    &recipient_certs,
                    signer_cert.as_ref(),
                    &pgp_policy,
                )
            })
            .await
            .map_err(map_join_err)?,
        }
    }

    async fn decrypt(&self, op: DecryptOp<'_>) -> Result<DecryptedPayload> {
        // Delegate to the signer-aware concrete path with EMPTY signer sets:
        // embedded-signature verification candidates are the decryption cert
        // only (the historical trait behavior). The receive path calls
        // [`OpenpgpBackend::decrypt_with_signers`] directly so keystore +
        // collected-keys certs are offered. An embedded signature that still
        // cannot be verified (unknown signer) DEGRADES - decrypt succeeds;
        // the trait's `DecryptedPayload` has no channel for the flag, so it
        // is dropped here exactly like the weak-algo warning below.
        let outcome = self
            .decrypt_with_signers(op.envelope, &op.decryption_key, &[], &[])
            .await?;

        // Drop the weak-algorithm warning + unverifiable-signature flag.
        // Surfacing them through `VerificationResult.failure_reason` is
        // reserved for the receive slice (spec §7), which adds a dedicated
        // channel rather than overload the failure-reason string; the
        // backend's receive orchestrator consumes the flag via
        // `decrypt_with_signers` directly.
        Ok(outcome.payload)
    }

    async fn sign(&self, op: SignOp<'_>) -> Result<SignedEnvelope> {
        // Only detached signing is supported in engine-core. Inline signing
        // for its own sake is unnecessary because `encrypt(sign_with=Some(..))`
        // already produces inline-signed ciphertext.
        if !op.detached {
            return Err(error::policy("inline sign not supported in engine-core"));
        }

        // W6: token-held signing key - route through the card signer shim.
        if let KeyHandle::Token {
            token_serial,
            key_id,
        } = &op.signing_key.handle
        {
            let signing_cert = self
                .resolve_cert(&op.signing_key, /* prefer_private */ false)
                .await?;
            let (signer, slot) = self.card_signer_for(&signing_cert, token_serial, key_id)?;
            let payload: Vec<u8> = op.payload.to_vec();
            let payload_for_envelope = payload.clone();
            let pgp_policy = self.pgp_policy.clone();
            let signature = match tokio::task::spawn_blocking(move || {
                engine::sign_detached_with_signer(
                    &payload,
                    &signing_cert,
                    Box::new(signer),
                    &pgp_policy,
                )
            })
            .await
            .map_err(map_join_err)?
            {
                Ok(sig) => sig,
                Err(e) => {
                    // Recover the typed token error if the shim captured one.
                    return Err(slot.take().unwrap_or(e));
                }
            };
            return Ok(SignedEnvelope {
                standard: Standard::OpenPgp,
                payload: payload_for_envelope,
                signature,
            });
        }

        let signing_cert = self
            .resolve_cert(&op.signing_key, /* prefer_private */ true)
            .await?;

        let payload: Vec<u8> = op.payload.to_vec();
        let pgp_policy = self.pgp_policy.clone();

        // Move `payload` into the closure; keep a clone for the SignedEnvelope
        // so we don't re-expose `op.payload` (which borrows from the caller).
        let payload_for_envelope = payload.clone();
        let signature = tokio::task::spawn_blocking(move || {
            engine::sign_detached(&payload, &signing_cert, &pgp_policy)
        })
        .await
        .map_err(map_join_err)??;

        Ok(SignedEnvelope {
            standard: Standard::OpenPgp,
            payload: payload_for_envelope,
            signature,
        })
    }

    async fn verify(&self, op: VerifyOp<'_>) -> Result<VerificationResult> {
        // Signer resolution (F2): the caller's `signed.signature.signer` handle
        // may be a receive-path placeholder (the signer is not known a priori),
        // and detached sigs usually carry the SIGNING SUBKEY's fingerprint in
        // the `IssuerFingerprint` subpacket while keystores are keyed by the
        // cert's PRIMARY fingerprint. We therefore resolve candidates from BOTH
        // the caller-supplied handle AND one `openpgp|<issuer-fp>` handle per
        // issuer fingerprint in the sig packet; the keystore resolves a
        // subkey-fp handle to the cert that owns it (the subkey-aware `get`
        // contract). Every resolved cert is handed to the engine, which
        // pre-filters by issuer fingerprint (primary OR subkey match) and
        // returns `UnknownKey` - fail-closed - when none match.
        //
        // A malformed signature errors here exactly as it would inside the
        // engine (`CryptoError::Malformed` from the packet parse).
        let issuer_fps = engine::issuer_fingerprints_of(&op.signed.signature.signature)?;

        let mut candidate_handles: Vec<KeyHandle> = Vec::with_capacity(issuer_fps.len() + 1);
        candidate_handles.push(op.signed.signature.signer.handle.clone());
        candidate_handles.extend(issuer_fps.iter().map(|fp| {
            KeyHandle::Software(KeyId(format!(
                "openpgp|{}",
                fp.to_hex().to_ascii_lowercase()
            )))
        }));

        // Best-effort per candidate (mirrors the receive path's per-key
        // warn-and-skip): a keystore error or unparseable blob on one
        // candidate must not mask another candidate that does resolve - the
        // engine's `UnknownKey` remains the fail-closed floor.
        let mut known_signers: Vec<openpgp::Cert> = Vec::new();
        let mut seen_fps: std::collections::HashSet<String> = std::collections::HashSet::new();
        for handle in &candidate_handles {
            let stored = match self.keystore.get(handle).await {
                Ok(Some(stored)) => stored,
                Ok(None) => continue,
                Err(e) => {
                    log::warn!("OpenpgpBackend::verify: signer candidate lookup failed: {e}");
                    continue;
                }
            };
            match keymap::parse_certs(&stored.public_data) {
                Ok(certs) => {
                    if let Some(cert) = certs.into_iter().next() {
                        // De-dupe: the caller's handle and an issuer-fp handle
                        // may resolve to the SAME cert.
                        if seen_fps.insert(cert.fingerprint().to_hex()) {
                            known_signers.push(cert);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("OpenpgpBackend::verify: parse signer candidate failed: {e}");
                }
            }
        }

        let payload: Vec<u8> = op.signed.payload.clone();
        let sig = op.signed.signature.clone();
        let pgp_policy = self.pgp_policy.clone();

        tokio::task::spawn_blocking(move || {
            engine::verify_detached(&payload, &sig, &known_signers, &pgp_policy)
        })
        .await
        .map_err(map_join_err)?
    }

    async fn generate_key(&self, params: KeyGenParams) -> Result<KeyHandleRef> {
        if params.standard != Standard::OpenPgp {
            return Err(CryptoError::UnsupportedStandard(format!(
                "OpenpgpBackend.generate_key: expected openpgp, got {}",
                params.standard
            )));
        }
        // Engine-core ships only the default Ed25519/X25519 key shape. RSA
        // generation is not exercised by this slice (the write policy already
        // rejects RSA2048; RSA3072+ generation is reserved for a later slice
        // that also adds a keygen-policy decision tree).
        if params.algorithm != "default" {
            return Err(error::policy(format!(
                "OpenpgpBackend.generate_key: algorithm '{}' not supported in engine-core (use 'default')",
                params.algorithm
            )));
        }
        // `params.passphrase` is intentionally ignored: at-rest protection is
        // the master-key layer's job, not OpenPGP S2K. We surface this
        // explicitly by NOT forwarding the passphrase to `engine::generate`.
        let cert = engine::generate(&params.user_id)?;
        self.persist_cert(cert).await
    }

    async fn import_key(
        &self,
        data: &[u8],
        passphrase: Option<SecretBox<String>>,
    ) -> Result<KeyHandleRef> {
        // Parse + (optional) secret-key decryption. Parse is fast enough to
        // run on the worker thread; a future hardening pass may move this to
        // `spawn_blocking` if benchmarks show meaningful blocking.
        let data: Vec<u8> = data.to_vec();
        let cert = engine::import(&data, passphrase)?;
        self.persist_cert(cert).await
    }

    async fn export_public(&self, handle: &KeyHandle) -> Result<Vec<u8>> {
        let stored = self.keystore.get(handle).await?.ok_or_else(|| {
            CryptoError::KeyNotFound(format!(
                "OpenpgpBackend.export_public: key not found for handle {:?}",
                handle
            ))
        })?;

        let certs = keymap::parse_certs(&stored.public_data)?;
        let cert = certs.into_iter().next().ok_or_else(|| {
            CryptoError::Malformed(
                "OpenpgpBackend.export_public: public blob contained no certs".into(),
            )
        })?;

        engine::export_armored_public(&cert)
    }
}
