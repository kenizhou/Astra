//! W6: Sequoia [`crypto::Signer`] / [`crypto::Decryptor`] shims over the
//! astra-crypto-core [`TokenProvider`] trait (OpenPGP hardware tokens / cards).
//!
//! The shims are **generic over the provider trait** — they know nothing
//! about PC/SC or OpenPGP-card APDUs. The backend's `OpenPgpCardProvider`
//! (pcsc + openpgp-card) and the test-suite `MockTokenProvider` both plug in
//! behind the same trait, so the full sign/decrypt path is exercised in CI
//! without hardware.
//!
//! ## Wire-format contracts (mirrors `TokenSignMechanism`)
//!
//! - RSA: the provider signs `DigestInfo(SHA-256) || digest` with PKCS#1
//!   v1.5 and returns the raw RSA signature. The shim wraps it in
//!   `mpi::Signature::RSA`.
//! - ECDSA P-256/P-384: the provider signs the digest and returns raw
//!   fixed-width `r || s`; the shim splits it into the two MPIs.
//! - Ed25519 (old-style EdDSA keys): the provider signs the OpenPGP hash
//!   output (Ed25519's "message" input per GnuPG/card convention) and
//!   returns the 64-byte signature; the shim emits `R || S` MPIs.
//!
//! Decrypt delegates the private op the same way:
//!
//! - RSA PKESK: provider returns the PKCS#1-unpadded block
//!   (`algo || session-key || checksum`), which Sequoia parses.
//! - ECDH (NIST + legacy Cv25519): provider returns the shared secret `Z`;
//!   Sequoia's public `ecdh::decrypt_unwrap` runs the KDF + AES-KW unwrap.
//! - X25519 (RFC 9580): provider returns the shared point `S`; the shim
//!   runs the RFC 9580 HKDF-SHA256 KEK derivation + AES-128-KW unwrap
//!   (Sequoia's own HKDF helper is `pub(crate)`, so the tiny HKDF is
//!   re-implemented here over Sequoia's hash contexts).
//!
//! ## Typed-error channel
//!
//! Sequoia collapses shim errors into its own `Error` type, which the
//! engine then wraps as `CryptoError::backend` — erasing the typed token
//! errors (`TokenPinRequired` / `TokenNotPresent` / `TokenPinIncorrect`)
//! the UI depends on. Both shims therefore capture the real error into a
//! [`CardErrorSlot`] before returning a placeholder Sequoia error; the
//! backend checks the slot first when the engine call fails.

use std::sync::{Arc, Mutex};

use astra_crypto_core::{
    CryptoError, Result as CryptoResult, TokenKeyRef, TokenProvider, TokenSignMechanism,
};
use sequoia_openpgp as openpgp;

use openpgp::crypto::{self, mpi, Decryptor, SessionKey, Signer};
use openpgp::packet::key::{Key, PublicParts, UnspecifiedRole};
use openpgp::types::{Curve, HashAlgorithm, SymmetricAlgorithm};

/// Shared slot recovering typed token errors across the Sequoia boundary
/// (see the module docs). `Clone` so the backend keeps a handle while the
/// shim moves into the engine.
#[derive(Clone, Default)]
pub struct CardErrorSlot(Arc<Mutex<Option<CryptoError>>>);

impl CardErrorSlot {
    /// Take the captured typed error (if any). Called by the backend after a
    /// failed engine op.
    pub fn take(&self) -> Option<CryptoError> {
        self.0.lock().expect("card error slot poisoned").take()
    }

    fn store(&self, e: CryptoError) {
        *self.0.lock().expect("card error slot poisoned") = Some(e);
    }
}

/// Capture `e` in the slot and return a placeholder Sequoia error (the slot
/// is the authoritative channel; the placeholder only forces Sequoia to
/// abort the operation). Sequoia's `Result` is anyhow-based, so the error is
/// returned in that form.
fn seq_err(e: CryptoError, slot: &CardErrorSlot) -> openpgp::anyhow::Error {
    let msg = format!("{e}");
    slot.store(e);
    openpgp::Error::InvalidOperation(msg).into()
}

/// A plain (non-slot) Sequoia error for slot-miss conditions — see the ECDH
/// arm's NOTE. Extracted to keep the anyhow type annotation in one place.
fn plain_seq_err(msg: String) -> openpgp::anyhow::Error {
    openpgp::Error::InvalidOperation(msg).into()
}

/// Provider-error mapping for the DECRYPT paths. PKESK::decrypt folds every
/// error into `None` ("try the next slot"), so device errors would vanish
/// unless captured — but a generic crypto failure (e.g. an RSA padding
/// reject on a slot addressed to a DIFFERENT key) is a normal slot-miss,
/// not a device error. Only the actionable device-state errors
/// (`TokenPinRequired` / `TokenNotPresent` / `TokenPinIncorrect`) go into
/// the typed slot; everything else is a plain Sequoia error.
fn decrypt_op_err(e: CryptoError, slot: &CardErrorSlot) -> openpgp::anyhow::Error {
    match &e {
        CryptoError::TokenPinRequired(_)
        | CryptoError::TokenNotPresent(_)
        | CryptoError::TokenPinIncorrect(_) => seq_err(e, slot),
        other => plain_seq_err(format!("{other}")),
    }
}

/// Map a signing public key to its token mechanism + the hash constraint the
/// mechanism carries. Ed25519 is hash-agnostic (the card signs whatever
/// digest OpenPGP hands it), so it accepts every default hash.
fn mechanism_for_signing_key(
    public: &Key<PublicParts, UnspecifiedRole>,
) -> CryptoResult<(TokenSignMechanism, Vec<HashAlgorithm>)> {
    match public.mpis() {
        mpi::PublicKey::RSA { .. } => Ok((
            TokenSignMechanism::RsaPkcs1Sha256,
            vec![HashAlgorithm::SHA256],
        )),
        mpi::PublicKey::ECDSA { curve, .. } => match curve {
            Curve::NistP256 => Ok((
                TokenSignMechanism::EcdsaP256Sha256,
                vec![HashAlgorithm::SHA256],
            )),
            Curve::NistP384 => Ok((
                TokenSignMechanism::EcdsaP384Sha384,
                vec![HashAlgorithm::SHA384],
            )),
            other => Err(CryptoError::UnsupportedStandard(format!(
                "token signing: ECDSA curve {other} not supported"
            ))),
        },
        mpi::PublicKey::EdDSA { .. } => Ok((
            TokenSignMechanism::Ed25519,
            // Hash-agnostic (the card signs whatever digest OpenPGP hands
            // it) — accept the modern default set.
            vec![
                HashAlgorithm::SHA256,
                HashAlgorithm::SHA384,
                HashAlgorithm::SHA512,
            ],
        )),
        other => Err(CryptoError::UnsupportedStandard(format!(
            "token signing: public-key algorithm {} not supported",
            other.algo().map(|a| a.to_string()).unwrap_or_default()
        ))),
    }
}

/// A Sequoia [`Signer`] whose private key lives on a hardware token.
pub struct CardSigner {
    public: Key<PublicParts, UnspecifiedRole>,
    provider: Arc<dyn TokenProvider>,
    key: TokenKeyRef,
    mechanism: TokenSignMechanism,
    acceptable: Vec<HashAlgorithm>,
    err: CardErrorSlot,
}

impl CardSigner {
    /// Build a signer for `public` (the token key's public half, selected
    /// from the stored cert by the caller). `err` is the typed-error
    /// channel — clone it BEFORE moving the signer into the engine.
    pub fn new(
        public: Key<PublicParts, UnspecifiedRole>,
        provider: Arc<dyn TokenProvider>,
        key: TokenKeyRef,
        err: CardErrorSlot,
    ) -> CryptoResult<Self> {
        let (mechanism, acceptable) = mechanism_for_signing_key(&public)?;
        Ok(Self {
            public,
            provider,
            key,
            mechanism,
            acceptable,
            err,
        })
    }
}

impl Signer for CardSigner {
    fn public(&self) -> &Key<PublicParts, UnspecifiedRole> {
        &self.public
    }

    fn acceptable_hashes(&self) -> &[HashAlgorithm] {
        &self.acceptable
    }

    fn sign(
        &mut self,
        _hash_algo: HashAlgorithm,
        digest: &[u8],
    ) -> openpgp::Result<mpi::Signature> {
        let raw = self
            .provider
            .sign(&self.key, self.mechanism, digest)
            .map_err(|e| seq_err(e, &self.err))?;
        signature_from_raw(self.public.mpis(), &raw).map_err(|e| seq_err(e, &self.err))
    }
}

/// Reassemble a token's raw signature bytes into Sequoia's `mpi::Signature`,
/// dispatched on the public key algorithm (the wire form is fixed by the
/// mechanism contract — see module docs).
fn signature_from_raw(public: &mpi::PublicKey, raw: &[u8]) -> CryptoResult<mpi::Signature> {
    match public {
        mpi::PublicKey::RSA { .. } => Ok(mpi::Signature::RSA {
            s: mpi::MPI::new(raw),
        }),
        mpi::PublicKey::ECDSA { curve, .. } => {
            let limb = curve.bits().map(|b| b.div_ceil(8)).map_err(|_| {
                CryptoError::Malformed("token signing: unknown ECDSA curve size".into())
            })?;
            if raw.len() != limb * 2 {
                return Err(CryptoError::Token(format!(
                    "token returned {}-byte ECDSA signature; expected {} (2 × {limb}-byte limbs)",
                    raw.len(),
                    limb * 2
                )));
            }
            Ok(mpi::Signature::ECDSA {
                r: mpi::MPI::new(&raw[..limb]),
                s: mpi::MPI::new(&raw[limb..]),
            })
        }
        mpi::PublicKey::EdDSA { .. } => {
            if raw.len() != 64 {
                return Err(CryptoError::Token(format!(
                    "token returned {}-byte Ed25519 signature; expected 64",
                    raw.len()
                )));
            }
            Ok(mpi::Signature::EdDSA {
                r: mpi::MPI::new(&raw[..32]),
                s: mpi::MPI::new(&raw[32..]),
            })
        }
        other => Err(CryptoError::UnsupportedStandard(format!(
            "token signing: public-key algorithm {} not supported",
            other.algo().map(|a| a.to_string()).unwrap_or_default()
        ))),
    }
}

/// A Sequoia [`Decryptor`] whose private key lives on a hardware token.
pub struct CardDecryptor {
    public: Key<PublicParts, UnspecifiedRole>,
    provider: Arc<dyn TokenProvider>,
    key: TokenKeyRef,
    err: CardErrorSlot,
}

impl CardDecryptor {
    /// Build a decryptor for `public` (the token key's public half). `err` is
    /// the typed-error channel — clone it BEFORE moving the decryptor into
    /// the engine.
    pub fn new(
        public: Key<PublicParts, UnspecifiedRole>,
        provider: Arc<dyn TokenProvider>,
        key: TokenKeyRef,
        err: CardErrorSlot,
    ) -> CryptoResult<Self> {
        Ok(Self {
            public,
            provider,
            key,
            err,
        })
    }
}

impl Decryptor for CardDecryptor {
    fn public(&self) -> &Key<PublicParts, UnspecifiedRole> {
        &self.public
    }

    fn decrypt(
        &mut self,
        ciphertext: &mpi::Ciphertext,
        plaintext_len: Option<usize>,
    ) -> openpgp::Result<SessionKey> {
        match ciphertext {
            mpi::Ciphertext::RSA { c } => {
                // The token does the PKCS#1 v1.5 private op and returns the
                // unpadded block (`algo || session-key || checksum`), which
                // Sequoia's PKESK layer parses.
                let block = self
                    .provider
                    .decrypt(&self.key, c.value())
                    .map_err(|e| decrypt_op_err(e, &self.err))?;
                Ok(SessionKey::from(block))
            }
            mpi::Ciphertext::ECDH { e, .. } => {
                let curve = match self.public.mpis() {
                    mpi::PublicKey::ECDH { curve, .. } => curve.clone(),
                    _ => {
                        return Err(seq_err(
                            CryptoError::Malformed(
                                "ECDH ciphertext presented to a non-ECDH token key".into(),
                            ),
                            &self.err,
                        ))
                    }
                };
                // Ephemeral public point, in the provider's ECDH-derive
                // input contract: SEC1 uncompressed (0x04 || x || y) for
                // NIST curves, raw 32 bytes for Cv25519. Note sequoia's
                // `decode_point` returns (x, y) SPLIT for NIST curves and
                // (raw32, empty) for 25519 — reassemble the wire form here.
                let (x, y) = e.decode_point(&curve).map_err(|e| {
                    seq_err(
                        CryptoError::Malformed(format!("decode ECDH ephemeral point: {e}")),
                        &self.err,
                    )
                })?;
                let point: Vec<u8> = if y.is_empty() {
                    x.to_vec()
                } else {
                    let mut p = Vec::with_capacity(1 + x.len() + y.len());
                    p.push(0x04);
                    p.extend_from_slice(x);
                    p.extend_from_slice(y);
                    p
                };
                let z = self
                    .provider
                    .decrypt(&self.key, &point)
                    .map_err(|e| decrypt_op_err(e, &self.err))?;
                // NOTE: an unwrap failure means "this PKESK slot was not
                // addressed to this key" (PKESK::decrypt folds errors into
                // `None` so the next slot is tried) — NOT a device error.
                // It must NOT populate the typed-error slot, or a wrong-key
                // iteration would surface as a spurious token failure
                // instead of `NoMatchingKey`.
                crypto::ecdh::decrypt_unwrap(
                    &self.public,
                    &crypto::mem::Protected::from(z),
                    ciphertext,
                    plaintext_len,
                )
                .map_err(|e| plain_seq_err(format!("ECDH session-key unwrap failed: {e}")))
            }
            mpi::Ciphertext::X25519 { e, key } => {
                let u = match self.public.mpis() {
                    mpi::PublicKey::X25519 { u } => u,
                    _ => {
                        return Err(seq_err(
                            CryptoError::Malformed(
                                "X25519 ciphertext presented to a non-X25519 token key".into(),
                            ),
                            &self.err,
                        ))
                    }
                };
                // The token computes the shared point S = x·E on-card.
                let s = self
                    .provider
                    .decrypt(&self.key, &e[..])
                    .map_err(|e| decrypt_op_err(e, &self.err))?;
                // RFC 9580 §3.2.2: KEK = HKDF-SHA256(E || U || S,
                // info="OpenPGP X25519"), then AES-128-KW unwrap. Unwrap
                // failures are slot-misses, not device errors (see the ECDH
                // arm's NOTE) — no typed-error slot.
                let kek = hkdf_sha256_openpgp_x25519(e.as_slice(), u, &s)
                    .map_err(|e| plain_seq_err(format!("X25519 KDF failed: {e}")))?;
                let sk = crypto::ecdh::aes_key_unwrap(
                    SymmetricAlgorithm::AES128,
                    &crypto::mem::Protected::from(kek),
                    &crypto::mem::Protected::from(key.to_vec()),
                )
                .map_err(|e| plain_seq_err(format!("X25519 session-key unwrap failed: {e}")))?;
                Ok(SessionKey::from(sk))
            }
            other => Err(seq_err(
                CryptoError::UnsupportedStandard(format!(
                    "token decrypt: ciphertext variant {} not supported",
                    other.pk_algo().map(|a| a.to_string()).unwrap_or_default()
                )),
                &self.err,
            )),
        }
    }
}

/// HMAC-SHA256 over Sequoia's hash context (no extra deps).
fn hmac_sha256(key: &[u8], data: &[u8]) -> CryptoResult<[u8; 32]> {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let mut h = HashAlgorithm::SHA256
            .context()
            .map_err(|e| CryptoError::Token(format!("hash context: {e}")))?
            .for_digest();
        h.update(key);
        let mut digest = [0u8; 32];
        h.digest(&mut digest)
            .map_err(|e| CryptoError::Token(format!("hash digest: {e}")))?;
        key_block[..32].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }

    let new_ctx = || {
        HashAlgorithm::SHA256
            .context()
            .map(|c| c.for_digest())
            .map_err(|e| CryptoError::Token(format!("hash context: {e}")))
    };

    let mut inner = new_ctx()?;
    inner.update(&ipad);
    inner.update(data);
    let mut inner_digest = [0u8; 32];
    inner
        .digest(&mut inner_digest)
        .map_err(|e| CryptoError::Token(format!("hash digest: {e}")))?;

    let mut outer = new_ctx()?;
    outer.update(&opad);
    outer.update(&inner_digest);
    let mut out = [0u8; 32];
    outer
        .digest(&mut out)
        .map_err(|e| CryptoError::Token(format!("hash digest: {e}")))?;
    Ok(out)
}

/// RFC 5869 HKDF-SHA256 (extract + expand, multi-block). Sequoia's own HKDF
/// helper is `pub(crate)`, hence this re-implementation over `hmac_sha256`.
fn hkdf_sha256(
    ikm: &[u8],
    salt: Option<&[u8]>,
    info: &[u8],
    out_len: usize,
) -> CryptoResult<Vec<u8>> {
    // Extract: PRK = HMAC(salt, IKM); absent salt = HashLen zero bytes.
    let zero_salt = [0u8; 32];
    let prk = hmac_sha256(salt.unwrap_or(&zero_salt), ikm)?;
    // Expand: T(0) = empty; T(i) = HMAC(PRK, T(i-1) || info || i).
    let mut out = Vec::with_capacity(out_len);
    let mut t: Vec<u8> = Vec::new();
    let mut counter: u8 = 1;
    while out.len() < out_len {
        let mut input = t.clone();
        input.extend_from_slice(info);
        input.push(counter);
        t = hmac_sha256(&prk, &input)?.to_vec();
        out.extend_from_slice(&t);
        counter = counter
            .checked_add(1)
            .ok_or_else(|| CryptoError::Token("hkdf: too many blocks".into()))?;
    }
    out.truncate(out_len);
    Ok(out)
}

/// RFC 9580 §3.2.2 KEK derivation for X25519 PKESKs:
/// `KEK = HKDF-SHA256(IKM = E || U || S, salt = none, info = "OpenPGP
/// X25519", L = 16)` — matches Sequoia's own software path
/// (`Backend::hkdf_sha256(&ikm, None, b"OpenPGP X25519", kek)`).
fn hkdf_sha256_openpgp_x25519(e: &[u8], u: &[u8; 32], s: &[u8]) -> CryptoResult<Vec<u8>> {
    let mut ikm = Vec::with_capacity(96);
    ikm.extend_from_slice(e);
    ikm.extend_from_slice(u);
    ikm.extend_from_slice(s);
    hkdf_sha256(&ikm, None, b"OpenPGP X25519", 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-check `hmac_sha256` against the RFC 4231 test case 2 vector
    /// (key = "Jefe", data = "what do ya want for nothing?").
    #[test]
    fn hmac_sha256_matches_rfc4231() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?").expect("hmac");
        let expected: [u8; 32] = [
            0x5b, 0xdc, 0xc1, 0x46, 0xbf, 0x60, 0x75, 0x4e, 0x6a, 0x04, 0x24, 0x26, 0x08, 0x95,
            0x75, 0xc7, 0x5a, 0x00, 0x3f, 0x08, 0x9d, 0x27, 0x39, 0x83, 0x9d, 0xec, 0x58, 0xb9,
            0x64, 0xec, 0x38, 0x43,
        ];
        assert_eq!(mac, expected);
    }

    /// Cross-check `hkdf_sha256` against RFC 5869 Test Case 1 (SHA-256,
    /// multi-block expand, L=42) — proves the extract+expand construction
    /// the X25519 KEK derivation is built on.
    #[test]
    fn hkdf_sha256_matches_rfc5869_case1() {
        let ikm = [0x0bu8; 22];
        let salt: Vec<u8> = (0x00u8..=0x0c).collect();
        let info: Vec<u8> = (0xf0u8..=0xf9).collect();
        let okm = hkdf_sha256(&ikm, Some(&salt), &info, 42).expect("hkdf");
        let expected: [u8; 42] = [
            0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a, 0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36,
            0x2f, 0x2a, 0x2d, 0x2d, 0x0a, 0x90, 0xcf, 0x1a, 0x5a, 0x4c, 0x5d, 0xb0, 0x2d, 0x56,
            0xec, 0xc4, 0xc5, 0xbf, 0x34, 0x00, 0x72, 0x08, 0xd5, 0xb8, 0x87, 0x18, 0x58, 0x65,
        ];
        assert_eq!(okm.as_slice(), &expected[..]);
    }

    /// RFC 5869 Test Case 2-style check with the absent-salt convention the
    /// X25519 path uses (salt = None → HashLen zeros), asserting length +
    /// domain separation rather than a fixed vector.
    #[test]
    fn hkdf_is_deterministic_and_domain_separated() {
        let e = [0x11u8; 32];
        let u = [0x22u8; 32];
        let s = [0x33u8; 32];
        let a = hkdf_sha256_openpgp_x25519(&e, &u, &s).expect("hkdf");
        let b = hkdf_sha256_openpgp_x25519(&e, &u, &s).expect("hkdf");
        assert_eq!(a, b, "HKDF must be deterministic");
        assert_eq!(a.len(), 16, "AES-128 KEK size");
        let mut s2 = s;
        s2[0] ^= 1;
        let c = hkdf_sha256_openpgp_x25519(&e, &u, &s2).expect("hkdf");
        assert_ne!(a, c, "different shared secret must give a different KEK");
    }

    /// Raw-signature → mpi::Signature reassembly, per algorithm.
    #[test]
    fn signature_from_raw_splits_ecdsa_limbs() {
        let curve = Curve::NistP256;
        let q = mpi::MPI::new(&[0x04; 65]);
        let public = mpi::PublicKey::ECDSA { curve, q };
        let raw: Vec<u8> = (0u8..64).collect();
        let sig = signature_from_raw(&public, &raw).expect("ecdsa sig");
        match sig {
            mpi::Signature::ECDSA { r, s } => {
                // MPI::new trims leading zero bytes (canonical MPI form);
                // raw[0] == 0, so r drops it.
                assert_eq!(r.value(), &raw[1..32]);
                assert_eq!(s.value(), &raw[32..]);
            }
            other => panic!("expected ECDSA, got {other:?}"),
        }
        // Wrong length → typed Token error.
        let err = signature_from_raw(&public, &[0u8; 63]).expect_err("bad length");
        assert!(matches!(err, CryptoError::Token(_)));
    }
}
