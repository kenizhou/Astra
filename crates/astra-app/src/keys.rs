// SPDX-License-Identifier: GPL-3.0-only
//! The key import surface (SP4d-1 D2): the app methods the desktop import
//! command drives. Split from the crate root (at the line cap) the way
//! `reading_password.rs` is: a child-module `impl App` block reading
//! `self.crypto_store()`.
//!
//! Two paths, dispatched by the material itself. Public material (an
//! OpenPGP public cert, an S/MIME cert) lands through the store's
//! public-only import, which never writes a private blob. Secret material
//! (an OpenPGP TSK, an S/MIME PEM key bundle or PKCS#12 bag) lands through
//! the existing backend `import_key`, which decrypts the secret under the
//! supplied passphrase and wraps it at rest under the account-bound AAD.
//!
//! **Privacy.** The passphrase moves through as a [`SecretBox`] (zeroized
//! on drop) and is never logged or persisted; only the vault-wrapped key
//! blob lands at rest.

use std::sync::Arc;

use astra_crypto_core::{CryptoBackend, CryptoPolicy, SecretBox, Standard};
use astra_crypto_store::{CryptoKeyRow, CryptoStore};
use engine_api::{AccountId, Provider};

use crate::App;

/// Why a key import refused. The words name the refusal, never the key
/// material or the passphrase.
#[derive(Debug, thiserror::Error)]
pub enum KeyImportError {
    /// No crypto store is installed: nothing to import against.
    #[error("no crypto store is installed")]
    NoStore,
    /// The input parses as no key of either standard.
    #[error("the input is not a recognised key or certificate")]
    Unrecognised,
    /// The backend refused the material or the passphrase.
    #[error("the key import failed: {0}")]
    Backend(String),
    /// The store refused the write.
    #[error("the key store refused the import: {0}")]
    Store(String),
}

impl<P: Provider> App<P> {
    /// Imports key material into `account`, dispatching by the material:
    /// secret-bearing input takes the private path (the backend import,
    /// passphrase-gated), everything else the public-cert store path.
    /// Answers the landed row (public view, no private material).
    ///
    /// The first private key of a standard becomes the account's default
    /// signing key when none stands, so the account can sign straight after
    /// the import; a later import never steals the standing default.
    ///
    /// **Privacy.** The passphrase moves into the backend as a [`SecretBox`]
    /// and is never logged or persisted.
    ///
    /// # Errors
    /// Refuses when no store is installed, the material parses as no key,
    /// the passphrase does not unlock the secret, or a write fails.
    pub async fn import_key(
        &self,
        account: &AccountId,
        bytes: &[u8],
        passphrase: Option<SecretBox<String>>,
    ) -> Result<CryptoKeyRow, KeyImportError> {
        let store = self.crypto_store().ok_or(KeyImportError::NoStore)?;

        // OpenPGP first: armor or binary. A TSK carries secret material and
        // takes the private path; a public cert takes the store path.
        if let Ok(certs) = astra_crypto_openpgp::keymap::parse_certs(bytes) {
            if let Some(cert) = certs.first() {
                if cert.is_tsk() {
                    return import_openpgp_private(&store, account, bytes, passphrase).await;
                }
                return astra_crypto_store::import_public_cert(&store, account.as_str(), bytes)
                    .map_err(KeyImportError::Store);
            }
        }

        // S/MIME: a PEM bundle carrying a private key, or a binary PKCS#12
        // bag, takes the private path; a lone cert (PEM or DER) takes the
        // store path.
        if smime_carries_secret(bytes) {
            return import_smime_private(&store, account, bytes, passphrase).await;
        }
        astra_crypto_store::import_public_cert(&store, account.as_str(), bytes)
            .map_err(KeyImportError::Store)
    }

    /// The account's keys across both standards, public view only (no
    /// private material crosses this answer). OpenPGP rows first, then
    /// S/MIME, each ordered by fingerprint.
    ///
    /// # Errors
    /// Refuses when no store is installed or a read fails.
    pub fn list_keys(&self, account: &AccountId) -> Result<Vec<CryptoKeyRow>, KeyImportError> {
        let store = self.crypto_store().ok_or(KeyImportError::NoStore)?;
        let mut rows =
            astra_crypto_store::list_crypto_keys_for_account(&store, account.as_str(), "openpgp")
                .map_err(KeyImportError::Store)?;
        rows.extend(
            astra_crypto_store::list_crypto_keys_for_account(&store, account.as_str(), "smime")
                .map_err(KeyImportError::Store)?,
        );
        Ok(rows)
    }
}

/// The private OpenPGP path: the existing backend import (passphrase-gated
/// secret decrypt, persist through the keystore), then the landed row read
/// back. The first private key of the standard becomes the default signer
/// when none stands.
async fn import_openpgp_private(
    store: &Arc<CryptoStore>,
    account: &AccountId,
    bytes: &[u8],
    passphrase: Option<SecretBox<String>>,
) -> Result<CryptoKeyRow, KeyImportError> {
    let backend = astra_crypto_openpgp::OpenpgpBackend::new(
        Arc::new(astra_crypto_store::SqliteKeyStore::new(
            Arc::clone(store),
            account.as_str(),
        )),
        CryptoPolicy::default(),
    );
    let handle = backend
        .import_key(bytes, passphrase)
        .await
        .map_err(|e| KeyImportError::Backend(e.to_string()))?;
    let row = astra_crypto_store::get_crypto_key_public(
        store,
        account.as_str(),
        "openpgp",
        handle.fingerprint.as_str(),
    )
    .map_err(KeyImportError::Store)?
    .ok_or(KeyImportError::Unrecognised)?;
    arm_default_signer(store, account, Standard::OpenPgp, &row.fingerprint)?;
    Ok(row)
}

/// The private S/MIME path: the backend's chain-aware import (PEM bundle or
/// PKCS#12 bag, passphrase-gated). The leaf lands through the keystore; the
/// bag's intermediates land as `intermediate` rows, never as trust anchors.
async fn import_smime_private(
    store: &Arc<CryptoStore>,
    account: &AccountId,
    bytes: &[u8],
    passphrase: Option<SecretBox<String>>,
) -> Result<CryptoKeyRow, KeyImportError> {
    let backend = astra_crypto_smime::SmimeBackend::new(
        Arc::new(astra_crypto_store::SqliteKeyStore::new(
            Arc::clone(store),
            account.as_str(),
        )),
        CryptoPolicy::default(),
    );
    let (handle, intermediates) = backend
        .import_key_with_chain(bytes, passphrase)
        .await
        .map_err(|e| KeyImportError::Backend(e.to_string()))?;
    for der in &intermediates {
        astra_crypto_store::upsert_intermediate_cert(store, account.as_str(), der)
            .map_err(KeyImportError::Store)?;
    }
    let row = astra_crypto_store::get_crypto_key_public(
        store,
        account.as_str(),
        "smime",
        handle.fingerprint.as_str(),
    )
    .map_err(KeyImportError::Store)?
    .ok_or(KeyImportError::Unrecognised)?;
    arm_default_signer(store, account, Standard::Smime, &row.fingerprint)?;
    Ok(row)
}

/// Flags the imported key as the standard's default signer when the account
/// has none yet; a standing default is never displaced.
fn arm_default_signer(
    store: &Arc<CryptoStore>,
    account: &AccountId,
    standard: Standard,
    fingerprint: &str,
) -> Result<(), KeyImportError> {
    let standing = astra_crypto_store::get_default_signing_key(store, account.as_str(), standard)
        .map_err(KeyImportError::Store)?;
    if standing.is_none() {
        astra_crypto_store::set_default_signing_key(
            store,
            account.as_str(),
            standard.as_str(),
            fingerprint,
        )
        .map_err(KeyImportError::Store)?;
    }
    Ok(())
}

/// Whether an S/MIME input carries a private key: a PEM block naming one,
/// or binary bytes that are not a lone DER cert (a PKCS#12 bag).
fn smime_carries_secret(bytes: &[u8]) -> bool {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if text.trim_start().starts_with("-----BEGIN") {
            return text.contains("PRIVATE KEY");
        }
    }
    astra_crypto_smime::fingerprint_of_cert_der(bytes).is_err()
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod keys_tests;
