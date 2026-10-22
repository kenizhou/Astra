//! Neutral, engine-free crypto abstractions for the Astra crypto framework.
//!
//! This crate defines the shared contract that every standard backend
//! (OpenPGP, S/MIME, 国密) and every key source (software, PKCS#11 token)
//! implements. It depends on NO cryptographic engine — only serde,
//! async-trait, thiserror, secrecy, zeroize, and subtle — so it compiles
//! standalone and lets the application hold one message type regardless of
//! backend.

pub mod backend;
pub mod envelope;
pub mod error;
pub mod handle;
pub mod ids;
pub mod keystore;
pub mod policy;
pub mod secret;
pub mod standard;
pub mod token;
pub mod trust;
pub mod util;

// W6: closure-driven TokenProvider mock for hardware-free tests. Compiled for
// this crate's own tests and for sibling crates that enable the `testing`
// feature via dev-dependencies (crypto-smime, crypto-openpgp, backend).
#[cfg(any(test, feature = "testing"))]
pub mod mock_token;

pub use backend::{CryptoBackend, DecryptOp, EncryptOp, KeyGenParams, SignOp, VerifyOp};
pub use envelope::{
    DecryptedPayload, DetachedSignature, EncryptedEnvelope, EncryptedPart, EncryptionGranularity,
    KeyPacketRef, Part, PartId, PartKind, SerializationStrategy, SignatureState, SignedEnvelope,
    VerificationResult,
};
pub use error::{CryptoError, Result};
pub use handle::{KeyHandle, KeyHandleRef, KeyUsage};
pub use ids::{Fingerprint, KeyId, TokenKeyId};
pub use keystore::{KeyStore, StoredKey};
pub use policy::{
    AeadAlgorithm, CryptoPolicy, DosLimits, HashAlgorithm, PkAlgorithm, SymmetricAlgorithm,
};
pub use secret::SecretBox;
pub use standard::Standard;
pub use token::{
    TokenInfo, TokenKeyInfo, TokenKeyRef, TokenKind, TokenProvider, TokenSignMechanism,
};
pub use trust::TrustState;
pub use util::constant_time_eq;
