//! Hardware-token abstraction (W6): PKCS#11 tokens (S/MIME) and OpenPGP cards.
//!
//! Private key material NEVER leaves the token — sign/decrypt operations are
//! delegated to the device through the [`TokenProvider`] trait. Two provider
//! families implement it in the backend crate (`src/mail/token/`):
//!
//! - **PKCS#11** via the `cryptoki` crate (dynamic-loaded vendor module —
//!   OpenSC, Yubico PIV, SoftHSM, ...).
//! - **OpenPGP card** via `openpgp-card` + `card-backend-pcsc` (PC/SC —
//!   YubiKey, Nitrokey, Gnuk, ...).
//!
//! The trait is **synchronous** because both underlying stacks are blocking
//! C-call wrappers (PKCS#11 `C_*` functions, PC/SC APDUs). Async backends call
//! providers from contexts that already run blocking crypto inline
//! (`SmimeBackend`'s sync CMS builders inside async fns) or inside
//! `tokio::task::spawn_blocking` (`OpenpgpBackend`'s engine calls), so no
//! additional bridging is needed. A token operation is a sub-100ms device
//! round-trip in the worst case, on par with the CMS/Sequoia CPU work those
//! call sites already perform inline.
//!
//! # PIN sessions
//!
//! Providers own their PIN session state: [`TokenProvider::unlock`] verifies
//! the PIN against the device and caches it **in memory only** (zeroized on
//! drop, never persisted, never in the OS keyring). Private ops return
//! [`CryptoError::TokenPinRequired`] when no session PIN is cached and
//! [`CryptoError::TokenNotPresent`] when the device was removed, so the UI can
//! surface actionable prompts instead of generic crypto failures.

use serde::{Deserialize, Serialize};

use crate::error::{CryptoError, Result};
use crate::handle::KeyUsage;
use crate::ids::TokenKeyId;
use crate::secret::SecretBox;

/// Which token family a provider talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenKind {
    /// PKCS#11 module (OpenSC, vendor middleware, SoftHSM). S/MIME keys.
    #[serde(rename = "pkcs11")]
    Pkcs11,
    /// OpenPGP card over PC/SC (YubiKey, Nitrokey, Gnuk). OpenPGP keys.
    #[serde(rename = "openpgp-card")]
    OpenPgpCard,
}

impl TokenKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenKind::Pkcs11 => "pkcs11",
            TokenKind::OpenPgpCard => "openpgp-card",
        }
    }
}

/// A physically present token, as enumerated by [`TokenProvider::list_tokens`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    /// Stable serial identifying the device (PKCS#11 token serial / OpenPGP
    /// card application-identifier serial). Matches `KeyHandle::Token`'s
    /// `token_serial`.
    pub serial: String,
    /// Human-readable label (token label / card holder).
    pub label: String,
    pub kind: TokenKind,
    /// `true` when a session PIN is cached and private ops will not prompt.
    pub unlocked: bool,
}

/// A key object living on a token, as enumerated by
/// [`TokenProvider::list_keys`]. Carries PUBLIC material only — private
/// material is never read off the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenKeyInfo {
    /// Provider-stable key identifier (PKCS#11 `CKA_ID` hex / OpenPGP slot
    /// name). Matches `KeyHandle::Token`'s `key_id`.
    pub key_id: TokenKeyId,
    pub label: String,
    /// Normalized algorithm string (`"RSA-2048"`, `"ECDSA-P256"`, ...). Stored
    /// in `crypto_keys.policy_json` at registration; informational only.
    pub algorithm: String,
    pub usage: KeyUsage,
    /// Public material readable from the token: X.509 cert DER (PKCS#11) or
    /// the OpenPGP public key fingerprint (cards). `None` when the token
    /// carries no public object for the key (e.g. cert-less PKCS#11 keys).
    pub public_data: Option<Vec<u8>>,
}

/// A reference to one key on one token — the address every private op takes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenKeyRef {
    pub token_serial: String,
    pub key_id: TokenKeyId,
}

impl TokenKeyRef {
    pub fn new(token_serial: impl Into<String>, key_id: TokenKeyId) -> Self {
        Self {
            token_serial: token_serial.into(),
            key_id,
        }
    }
}

/// The signature operation a token must perform. All mechanisms hand the
/// token a PRE-COMPUTED digest (or, for EdDSA, the bytes Ed25519 signs — the
/// OpenPGP hash output, which cards treat as the Ed25519 "message"); the
/// token never sees the full message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSignMechanism {
    /// RSA PKCS#1 v1.5 over `DigestInfo(SHA-256) || digest`. The provider
    /// constructs the DigestInfo prefix; the token does the raw RSA op.
    RsaPkcs1Sha256,
    /// ECDSA P-256 over a SHA-256 digest. Token returns raw `r || s`
    /// (fixed 32-byte limbs); callers re-encode as needed (DER for CMS, MPI
    /// pair for OpenPGP).
    EcdsaP256Sha256,
    /// ECDSA P-384 over a SHA-384 digest (raw `r || s`, 48-byte limbs).
    EcdsaP384Sha384,
    /// Ed25519: `digest` is the hash output OpenPGP feeds to Ed25519 as its
    /// message input. Token returns the 64-byte Ed25519 signature.
    Ed25519,
}

/// The contract every hardware-token provider implements.
///
/// Implementations must uphold these invariants:
/// - Private material never leaves the device: no method returns private
///   bytes, and no cached session state is persisted anywhere.
/// - PINs are held in zeroizing containers and dropped on removal/lock.
/// - Device-absence and PIN failures surface as the typed
///   [`CryptoError::TokenNotPresent`] / [`CryptoError::TokenPinRequired`] /
///   [`CryptoError::TokenPinIncorrect`] variants — never as generic
///   `Backend`/`Malformed` errors — so the UI can prompt appropriately.
pub trait TokenProvider: Send + Sync {
    /// Which token family this provider talks to.
    fn kind(&self) -> TokenKind;

    /// Enumerate currently-present tokens. An empty list is NOT an error.
    fn list_tokens(&self) -> Result<Vec<TokenInfo>>;

    /// Enumerate the key objects on one token. `TokenNotPresent` when the
    /// serial is no longer present.
    fn list_keys(&self, token_serial: &str) -> Result<Vec<TokenKeyInfo>>;

    /// Verify `pin` against the token and cache it for this process's
    /// session (in-memory, zeroized; never persisted). `remember = false`
    /// marks the cache entry single-use: it is dropped after the next
    /// successful private op ("don't remember for this session").
    ///
    /// Errors: `TokenNotPresent`, `TokenPinIncorrect` (message SHOULD carry
    /// the remaining retry count when the device reports one).
    fn unlock(&self, token_serial: &str, pin: SecretBox<String>, remember: bool) -> Result<()>;

    /// Drop any cached session PIN for the token (e.g. on device removal or
    /// an explicit "forget PIN" action). No-op when nothing is cached.
    fn lock(&self, token_serial: &str) -> Result<()>;

    /// Sign `digest` with the token key under `mechanism`. Returns the raw
    /// signature in the mechanism's wire format (see [`TokenSignMechanism`]).
    fn sign(
        &self,
        key: &TokenKeyRef,
        mechanism: TokenSignMechanism,
        digest: &[u8],
    ) -> Result<Vec<u8>>;

    /// Raw private-key decrypt/derive operation:
    /// - RSA keys: PKCS#1 v1.5 decryption of `ciphertext`; returns the
    ///   unpadded block (for CMS this is the CEK; for OpenPGP the
    ///   `algo || session-key || checksum` block).
    /// - ECDH keys: ECDH against the ephemeral public point in `ciphertext`;
    ///   returns the raw shared secret. Point format: SEC1 uncompressed
    ///   (`0x04 || x || y`) for NIST curves, raw 32 bytes for Curve25519.
    fn decrypt(&self, key: &TokenKeyRef, ciphertext: &[u8]) -> Result<Vec<u8>>;

    /// X.509 cert chain stored on the token for `key` (leaf first), for
    /// S/MIME chain assembly. Default: not supported — callers fall back to
    /// the keystore-stored cert captured at registration time.
    fn x509_chain(&self, _key: &TokenKeyRef) -> Result<Vec<Vec<u8>>> {
        Err(CryptoError::NotImplemented(
            "x509_chain not supported by this provider".into(),
        ))
    }
}

/// Convenience: build the [`crate::handle::KeyHandle::Token`] for a key ref.
impl From<&TokenKeyRef> for crate::handle::KeyHandle {
    fn from(r: &TokenKeyRef) -> Self {
        crate::handle::KeyHandle::Token {
            token_serial: r.token_serial.clone(),
            key_id: r.key_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_key_ref_converts_to_token_handle() {
        let r = TokenKeyRef::new("YUBI-123", TokenKeyId("slot-9a".into()));
        let h = crate::handle::KeyHandle::from(&r);
        match h {
            crate::handle::KeyHandle::Token {
                token_serial,
                key_id,
            } => {
                assert_eq!(token_serial, "YUBI-123");
                assert_eq!(key_id.0, "slot-9a");
            }
            other => panic!("expected Token handle, got {other:?}"),
        }
    }

    #[test]
    fn token_info_serializes_camel_case() {
        let info = TokenInfo {
            serial: "S1".into(),
            label: "YubiKey 5".into(),
            kind: TokenKind::OpenPgpCard,
            unlocked: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("\"tokenSerial\""), "serial stays 'serial'");
        assert!(json.contains("\"serial\":\"S1\""));
        assert!(json.contains("\"kind\":\"openpgp-card\""));
    }

    #[test]
    fn default_x509_chain_is_not_implemented() {
        struct P;
        impl TokenProvider for P {
            fn kind(&self) -> TokenKind {
                TokenKind::Pkcs11
            }
            fn list_tokens(&self) -> Result<Vec<TokenInfo>> {
                Ok(vec![])
            }
            fn list_keys(&self, _s: &str) -> Result<Vec<TokenKeyInfo>> {
                Ok(vec![])
            }
            fn unlock(&self, _s: &str, _p: SecretBox<String>, _r: bool) -> Result<()> {
                Ok(())
            }
            fn lock(&self, _s: &str) -> Result<()> {
                Ok(())
            }
            fn sign(&self, _k: &TokenKeyRef, _m: TokenSignMechanism, _d: &[u8]) -> Result<Vec<u8>> {
                Err(CryptoError::NotImplemented("sign".into()))
            }
            fn decrypt(&self, _k: &TokenKeyRef, _c: &[u8]) -> Result<Vec<u8>> {
                Err(CryptoError::NotImplemented("decrypt".into()))
            }
        }
        let p = P;
        let key = TokenKeyRef::new("s", TokenKeyId("k".into()));
        match p.x509_chain(&key) {
            Err(CryptoError::NotImplemented(_)) => {}
            other => panic!("expected NotImplemented, got {other:?}"),
        }
    }
}
