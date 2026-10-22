// SPDX-License-Identifier: GPL-3.0-only
//! The at-rest secret vault: kylins' `platform::secrets` AAD-bound half,
//! ported. A 256-bit master key lives in the OS keyring; every wrapped blob
//! is `hex(0x01 || nonce(12) || ct+tag)` of AES-256-GCM with the caller's
//! context (plus the version byte) as AAD, so a blob lifted from one
//! account/field fails AEAD verification anywhere else. The legacy v0
//! no-AAD API is not ported: this is a fresh store with no v0 blobs to read.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use rand::Rng;
use secrecy::{ExposeSecret, SecretBox};
use std::sync::Mutex;

/// OS keyring entry holding the 256-bit master secret that wraps the crypto
/// store's soft private keys at rest.
pub const KEYRING_SERVICE: &str = "astra-mail";
/// The keyring user name the master secret is stored under.
pub const KEYRING_USER: &str = "crypto-master-key";

/// Vault format + master-key version. v1 = AAD-bound, version-prefixed.
pub const VAULT_VERSION_V1: u8 = 0x01;
const NONCE_LEN: usize = 12;

type MasterKey = SecretBox<[u8; 32]>;

static KEY: Mutex<Option<MasterKey>> = Mutex::new(None);

/// Typed vault error.
#[derive(Debug, thiserror::Error)]
pub enum CryptoVaultError {
    /// The master-key cache lock is poisoned.
    #[error("master key lock poisoned")]
    KeyLockPoisoned,
    /// The OS keyring rejected the read or write.
    #[error("keyring error: {0}")]
    Keyring(String),
    /// A stored blob is not valid hex.
    #[error("hex decode error: {0}")]
    Decode(String),
    /// A stored blob is shorter than version + nonce + tag.
    #[error("ciphertext too short")]
    CiphertextTooShort,
    /// The blob's version byte is not the v1 this build reads.
    #[error("unsupported vault version byte: {0}")]
    UnsupportedVersion(u8),
    /// AEAD verification or the cipher itself failed.
    #[error("aes-gcm error: {0}")]
    Aead(String),
}

/// RAII guard holding the master-key lock, so the secret is only exposed
/// while the guard is alive; the cache is populated lazily on first access.
pub struct KeyGuard<'a> {
    guard: std::sync::MutexGuard<'a, Option<MasterKey>>,
}

impl std::fmt::Debug for KeyGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the key material, not even its presence.
        f.debug_struct("KeyGuard").finish_non_exhaustive()
    }
}

impl KeyGuard<'_> {
    /// The raw master-key bytes, scoped to this guard's lifetime.
    fn key_bytes(&self) -> &[u8; 32] {
        self.guard
            .as_ref()
            .expect("key initialized before use")
            .expose_secret()
    }
}

/// Acquire the process master key, creating + persisting it on first use.
///
/// # Errors
/// Fails when the keyring cannot be read or written, or the cache lock is poisoned.
pub fn acquire_key() -> Result<KeyGuard<'static>, CryptoVaultError> {
    let mut guard = KEY.lock().map_err(|_| CryptoVaultError::KeyLockPoisoned)?;
    if guard.is_none() {
        *guard = Some(load_or_create_master_key()?);
    }
    Ok(KeyGuard { guard })
}

fn load_or_create_master_key() -> Result<MasterKey, CryptoVaultError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| CryptoVaultError::Keyring(e.to_string()))?;

    let key = match entry.get_password() {
        Ok(hex_key) => {
            let mut key = [0u8; 32];
            hex::decode_to_slice(hex_key, &mut key)
                .map_err(|e| CryptoVaultError::Decode(e.to_string()))?;
            key
        }
        Err(keyring::Error::NoEntry) => {
            let mut key = [0u8; 32];
            rand::rng().fill_bytes(&mut key);
            entry
                .set_password(&hex::encode(key))
                .map_err(|e| CryptoVaultError::Keyring(e.to_string()))?;
            key
        }
        Err(e) => return Err(CryptoVaultError::Keyring(e.to_string())),
    };

    Ok(SecretBox::new(Box::new(key)))
}

/// Installs a fixed master key for tests (sibling crates opt in through the
/// `testing` feature; this crate's own tests get it via `cfg(test)`), so the
/// suite never touches the real OS keyring.
#[cfg(any(test, feature = "testing"))]
pub fn set_test_master_key(key: [u8; 32]) {
    let mut guard = KEY.lock().expect("master key lock");
    *guard = Some(SecretBox::new(Box::new(key)));
}

/// Build the full AAD fed to AES-GCM: the caller's context is prefixed with
/// the version byte so the tag authenticates the format as well as the
/// caller-supplied context (prevents format/version confusion).
fn full_aad(aad: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + aad.len());
    out.push(VAULT_VERSION_V1);
    out.extend_from_slice(aad);
    out
}

/// Encrypt `plaintext` with caller-supplied AAD. Output format:
/// `hex(0x01 || nonce(12) || ct+tag)`. Bind `account_id`, field name, and
/// key version into `aad` so cross-account/field replay fails verification.
///
/// # Errors
/// Fails when the master key cannot be acquired or the cipher fails.
pub fn encrypt_with_aad(plaintext: &[u8], aad: &[u8]) -> Result<String, CryptoVaultError> {
    let guard = acquire_key()?;
    let cipher = Aes256Gcm::new_from_slice(guard.key_bytes())
        .map_err(|e| CryptoVaultError::Aead(e.to_string()))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let payload_aad = full_aad(aad);
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: &payload_aad,
            },
        )
        .map_err(|e| CryptoVaultError::Aead(e.to_string()))?;

    let mut out = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
    out.push(VAULT_VERSION_V1);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(hex::encode(out))
}

/// Decrypt a v1 `hex(0x01 || nonce(12) || ct+tag)` blob produced by
/// [`encrypt_with_aad`], verifying the version byte and AAD.
///
/// # Errors
/// Fails when the blob is malformed, has an unsupported version, or AEAD verification fails (wrong AAD or key).
pub fn decrypt_with_aad(blob_hex: &str, aad: &[u8]) -> Result<Vec<u8>, CryptoVaultError> {
    let raw = hex::decode(blob_hex).map_err(|e| CryptoVaultError::Decode(e.to_string()))?;
    if raw.len() < 1 + NONCE_LEN + 16 {
        return Err(CryptoVaultError::CiphertextTooShort);
    }
    let version = raw[0];
    if version != VAULT_VERSION_V1 {
        return Err(CryptoVaultError::UnsupportedVersion(version));
    }
    let nonce_bytes = &raw[1..=NONCE_LEN];
    let ciphertext = &raw[1 + NONCE_LEN..];

    let guard = acquire_key()?;
    let cipher = Aes256Gcm::new_from_slice(guard.key_bytes())
        .map_err(|e| CryptoVaultError::Aead(e.to_string()))?;

    let payload_aad = full_aad(aad);
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad: &payload_aad,
            },
        )
        .map_err(|e| CryptoVaultError::Aead(e.to_string()))
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod vault_tests;
