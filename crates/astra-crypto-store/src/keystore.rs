// SPDX-License-Identifier: GPL-3.0-only
//! Store-backed `KeyStore` bridge: kylins' `platform::keystore.rs` ported.
//! Adapts [`astra_crypto_core::KeyStore`] (the material-aware trait) to the
//! [`CryptoStore`] layer.
//!
//! `StoredKey` <-> `CryptoKeyRecord` mapping:
//! - `public_data` (DER bytes) <-> hex-encoded `CryptoKeyRecord.public_data`.
//! - `private_data` (secret bytes) <-> hex-encoded
//!   `CryptoKeyRecord.private_data`; the store wraps these at rest via the
//!   vault.
//! - `algorithm` + `usage` (crypto-core metadata with no db column) ride in
//!   the per-key `policy_json` field so they survive a put->get round-trip.
//! - The `KeyHandleRef`'s `KeyId` embeds `standard|fingerprint` so that
//!   `get(&KeyHandle)` (which receives only the opaque `KeyHandle`) can
//!   recover the db lookup keys.
//!
//! # OpenPGP subkey-aware `get`
//!
//! For `standard == openpgp`, `get` additionally resolves a handle whose
//! fingerprint names ANY key of a stored cert (primary OR any subkey), not
//! just the primary: a detached signature's `IssuerFingerprint` subpacket
//! names the signing SUBKEY, and `OpenpgpBackend::verify` searches the
//! keystore by that issuer fingerprint. On a direct miss the store scans the
//! account's OpenPGP certs for one owning the requested key and returns it
//! under its canonical primary-fingerprint handle. A fingerprint no imported
//! cert owns stays a miss (fail-closed). S/MIME lookups are unaffected
//! (X.509 has no subkeys).
//!
//! # Account binding
//!
//! The db schema requires an account id on every insert; `StoredKey`/
//! `KeyHandleRef` carry no account id. The bridge therefore binds a fixed
//! `account_id` at construction, mirroring how the rest of the app scopes
//! data per mail account. The bridge borrows the store: the trait is async
//! but the store is sync, so the trait methods delegate to sync inherent
//! methods (also used directly by the sync recipient resolver).

use std::str::FromStr;

use astra_crypto_core::{
    CryptoError, Fingerprint, KeyHandle, KeyHandleRef, KeyId, KeyStore, KeyUsage,
    Result as CryptoResult, Standard, StoredKey,
};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

use crate::store::CryptoStore;

/// String-error wrapper so store `String` errors can flow through
/// `CryptoError::backend` (which requires `std::error::Error`).
#[derive(Debug, thiserror::Error)]
#[error("keystore db error: {0}")]
struct DbError(String);

/// Per-key metadata persisted in `crypto_keys.policy_json`. The db schema
/// has no `algorithm`/`usage` columns; these crypto-core fields ride
/// alongside the key so a `get` round-trip recovers them.
#[derive(Serialize, Deserialize)]
struct KeyMetadata {
    algorithm: String,
    usage: KeyUsage,
}

/// SQLite-backed `KeyStore` scoped to one mail account. The store layer
/// encrypts private material at rest; this bridge hex-encodes binary
/// material so DER survives the String-based `CryptoKeyRecord` fields
/// losslessly.
#[derive(Debug)]
pub struct SqliteKeyStore<'a> {
    store: &'a CryptoStore,
    account_id: String,
}

impl<'a> SqliteKeyStore<'a> {
    /// Construct a keystore bound to `account_id`. The account must be
    /// registered with the store (`CryptoStore::register_account`).
    pub fn from_store(store: &'a CryptoStore, account_id: impl Into<String>) -> Self {
        Self {
            store,
            account_id: account_id.into(),
        }
    }

    /// Encode `standard|fingerprint` into a `KeyId` so a later
    /// `get(&KeyHandle::Software(KeyId))` can recover the db lookup keys.
    fn encode_key_id(standard: Standard, fingerprint: &Fingerprint) -> KeyId {
        KeyId(format!("{}|{}", standard.as_str(), fingerprint.as_str()))
    }

    /// Parse a `KeyId` produced by [`SqliteKeyStore::encode_key_id`].
    fn decode_key_id(id: &KeyId) -> CryptoResult<(Standard, Fingerprint)> {
        let (std_str, fp) = id.0.split_once('|').ok_or_else(|| {
            CryptoError::Malformed(format!("invalid keystore KeyId (no '|'): {}", id.0))
        })?;
        let standard = Standard::from_str(std_str)
            .map_err(|_| CryptoError::Malformed(format!("unknown standard in KeyId: {std_str}")))?;
        Ok((standard, Fingerprint::new(fp)))
    }

    /// OpenPGP subkey-aware fallback: find the PRIMARY fingerprint of the
    /// account's OpenPGP cert that owns `requested` as its primary OR any
    /// subkey fingerprint.
    ///
    /// The `crypto_keys` table is keyed by primary fingerprint, but a
    /// detached signature's `IssuerFingerprint` subpacket names the signing
    /// SUBKEY: `OpenpgpBackend::verify` resolves signers by issuer
    /// fingerprint, so a subkey-fp handle must resolve to the owning cert.
    /// Best-effort per cert: an unparseable blob is skipped with a warning
    /// (one corrupt row must not brick signer resolution for the rest),
    /// mirroring the receive path's per-key warn-and-skip. Returns `None`
    /// when no imported cert contains the key: the fail-closed floor
    /// (`UnknownKey`) is preserved.
    fn find_openpgp_primary_for_key_fingerprint(
        &self,
        requested: &Fingerprint,
    ) -> CryptoResult<Option<String>> {
        let rows = crate::list_crypto_keys_for_account(self.store, &self.account_id, "openpgp")
            .map_err(|e| CryptoError::backend(DbError(e)))?;
        for row in rows {
            let Some(record) = crate::get_crypto_key_full(
                self.store,
                &self.account_id,
                "openpgp",
                &row.fingerprint,
            )
            .map_err(|e| CryptoError::backend(DbError(e)))?
            else {
                continue;
            };
            let public_data = match hex::decode(&record.public_data) {
                Ok(bytes) => bytes,
                Err(e) => {
                    log::warn!(
                        "[keystore] openpgp subkey fallback: public_data hex decode failed for {}: {}",
                        row.fingerprint,
                        e
                    );
                    continue;
                }
            };
            let certs = match astra_crypto_openpgp::keymap::parse_certs(&public_data) {
                Ok(certs) => certs,
                Err(e) => {
                    log::warn!(
                        "[keystore] openpgp subkey fallback: parse cert failed for {}: {}",
                        row.fingerprint,
                        e
                    );
                    continue;
                }
            };
            let owns_key = certs.iter().any(|cert| {
                cert.keys().any(|ka| {
                    ka.key()
                        .fingerprint()
                        .to_hex()
                        .eq_ignore_ascii_case(requested.as_str())
                })
            });
            if owns_key {
                return Ok(Some(row.fingerprint));
            }
        }
        Ok(None)
    }

    /// Sync body of [`KeyStore::put`].
    ///
    /// # Errors
    /// Fails when the store write cannot run.
    pub fn put_sync(&self, key: StoredKey) -> CryptoResult<KeyHandleRef> {
        let standard = key.handle.standard;
        let fingerprint = key.handle.fingerprint.clone();

        // Look up the bound account's email so find_by_email can locate this
        // key later (the cert's SAN email isn't parsed until import).
        let email = self
            .store
            .account_email(&self.account_id)
            .map_err(|e| CryptoError::backend(DbError(e)))?;

        // Hex-encode material so binary DER survives the String-based db fields.
        let public_hex = hex::encode(&key.public_data);
        let private_hex = key
            .private_data
            .as_ref()
            .map(|s| hex::encode(s.expose_secret().as_slice()));

        // Persist algorithm + usage in policy_json (no dedicated columns).
        let metadata = KeyMetadata {
            algorithm: key.handle.algorithm.clone(),
            usage: key.handle.usage,
        };
        let policy_json = serde_json::to_string(&metadata).ok();

        let record = crate::CryptoKeyRecord {
            row: crate::CryptoKeyRow {
                // Empty id: upsert_crypto_key generates a uuid.
                id: String::new(),
                account_id: self.account_id.clone(),
                standard: standard.as_str().to_string(),
                key_type: "cert".to_string(),
                email,
                fingerprint: fingerprint.as_str().to_string(),
                origin: "generated".to_string(),
                ..Default::default()
            },
            public_data: public_hex,
            private_data: private_hex,
            policy_json,
        };

        crate::upsert_crypto_key(self.store, &record)
            .map_err(|e| CryptoError::backend(DbError(e)))?;

        // Return a canonical KeyHandleRef whose KeyId embeds standard|fingerprint.
        let mut canonical = key.handle;
        canonical.handle = KeyHandle::Software(Self::encode_key_id(standard, &fingerprint));
        Ok(canonical)
    }

    /// Sync body of [`KeyStore::get`].
    ///
    /// # Errors
    /// Fails when a stored blob is malformed or the store read cannot run; a missing key is `Ok(None)`, never an error.
    pub fn get_sync(&self, handle: &KeyHandle) -> CryptoResult<Option<StoredKey>> {
        let key_id = match handle {
            KeyHandle::Software(id) => id,
            // Token keys resolve via (account, token_serial, token_key_id)
            // to the registration-time row: public material only; the
            // private op is delegated to the device by the backend.
            KeyHandle::Token {
                token_serial,
                key_id,
            } => {
                let record = crate::get_crypto_key_by_token(
                    self.store,
                    &self.account_id,
                    token_serial,
                    &key_id.0,
                )
                .map_err(|e| CryptoError::backend(DbError(e)))?;
                let Some(record) = record else {
                    return Ok(None);
                };
                let public_data = hex::decode(&record.public_data).map_err(|e| {
                    CryptoError::Malformed(format!("token public_data hex decode failed: {e}"))
                })?;
                let (algorithm, usage) = record
                    .policy_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<KeyMetadata>(s).ok())
                    .map_or_else(
                        || ("unknown".to_string(), KeyUsage::SignAndEncrypt),
                        |m| (m.algorithm, m.usage),
                    );
                let standard = Standard::from_str(&record.row.standard).map_err(|_| {
                    CryptoError::Malformed(format!(
                        "unknown standard in token row: {}",
                        record.row.standard
                    ))
                })?;
                let canonical_fp = Fingerprint::new(&record.row.fingerprint);
                return Ok(Some(StoredKey {
                    handle: KeyHandleRef {
                        handle: KeyHandle::Token {
                            token_serial: token_serial.clone(),
                            key_id: key_id.clone(),
                        },
                        standard,
                        fingerprint: canonical_fp,
                        usage,
                        algorithm,
                    },
                    public_data,
                    private_data: None,
                }));
            }
        };
        let (standard, fingerprint) = Self::decode_key_id(key_id)?;

        let mut record = crate::get_crypto_key_full(
            self.store,
            &self.account_id,
            standard.as_str(),
            fingerprint.as_str(),
        )
        .map_err(|e| CryptoError::backend(DbError(e)))?;

        // OpenPGP subkey-aware fallback: a detached sig's IssuerFingerprint
        // names the signing SUBKEY, but the table is keyed by primary fp.
        // On a direct miss for an OpenPGP handle, resolve the requested fp
        // to the owning cert's primary fp and fetch that record. S/MIME
        // misses stay misses (X.509 has no subkeys). Both the scan AND the
        // re-fetch are scoped to this keystore's account: a same-fingerprint
        // row under another account must never resolve (private-key leak).
        if record.is_none() && standard == Standard::OpenPgp {
            if let Some(primary_fp) = self.find_openpgp_primary_for_key_fingerprint(&fingerprint)? {
                record = crate::get_crypto_key_full(
                    self.store,
                    &self.account_id,
                    standard.as_str(),
                    &primary_fp,
                )
                .map_err(|e| CryptoError::backend(DbError(e)))?;
            }
        }

        let Some(record) = record else {
            return Ok(None);
        };

        // Hex-decode public material.
        let public_data = hex::decode(&record.public_data)
            .map_err(|e| CryptoError::Malformed(format!("public_data hex decode failed: {e}")))?;

        // Hex-decode private material (already decrypted by get_crypto_key_full).
        let private_data = match record.private_data.as_deref() {
            Some(hex_str) if !hex_str.is_empty() => {
                let bytes = hex::decode(hex_str).map_err(|e| {
                    CryptoError::Malformed(format!("private_data hex decode failed: {e}"))
                })?;
                Some(astra_crypto_core::SecretBox::new(Box::new(bytes)))
            }
            _ => None,
        };

        // Recover algorithm + usage from policy_json, or fall back to defaults.
        let (algorithm, usage) = record
            .policy_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<KeyMetadata>(s).ok())
            .map_or_else(
                || ("unknown".to_string(), KeyUsage::SignAndEncrypt),
                |m| (m.algorithm, m.usage),
            );

        // The returned handle is CANONICAL: built from the record's own
        // (primary) fingerprint, which differs from the requested fingerprint
        // when the subkey-aware fallback resolved a subkey-fp handle.
        let canonical_fp = Fingerprint::new(&record.row.fingerprint);
        let handle_ref = KeyHandleRef {
            handle: KeyHandle::Software(Self::encode_key_id(standard, &canonical_fp)),
            standard,
            fingerprint: canonical_fp,
            usage,
            algorithm,
        };

        Ok(Some(StoredKey {
            handle: handle_ref,
            public_data,
            private_data,
        }))
    }

    /// Sync body of [`KeyStore::find_by_email`].
    ///
    /// # Errors
    /// Fails when the store read cannot run.
    pub fn find_by_email_sync(
        &self,
        standard: Standard,
        email: &str,
    ) -> CryptoResult<Vec<KeyHandleRef>> {
        let rows = crate::list_crypto_keys_for_email(self.store, standard.as_str(), email)
            .map_err(|e| CryptoError::backend(DbError(e)))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            // Fetchability filter: the email->fingerprint discovery is
            // GLOBAL (the row carrying the recipient's email may live under
            // the recipient's own account), but `get` is account-scoped: a
            // handle this account's keystore cannot fetch would die
            // downstream with a raw backend error instead of the precise
            // fail-closed missing-cert signal. Surface only handles a
            // same-fingerprint row under THIS account can serve (e.g. a
            // contact cert imported into the sending account, the legitimate
            // multi-account send path); drop the rest so callers see a clean
            // miss.
            let fetchable = crate::get_crypto_key_public(
                self.store,
                &self.account_id,
                &row.standard,
                &row.fingerprint,
            )
            .map_err(|e| CryptoError::backend(DbError(e)))?
            .is_some();
            if !fetchable {
                continue;
            }
            let fp = Fingerprint::new(&row.fingerprint);
            let Some(std) = Standard::from_str(&row.standard).ok() else {
                continue;
            };
            // Token rows surface Token handles (the recipient path only
            // reads public material, which the token get above serves).
            let handle = match (&row.token_serial, &row.token_key_id) {
                (Some(serial), Some(key_id)) => KeyHandle::Token {
                    token_serial: serial.clone(),
                    key_id: astra_crypto_core::TokenKeyId(key_id.clone()),
                },
                _ => KeyHandle::Software(Self::encode_key_id(std, &fp)),
            };
            out.push(KeyHandleRef {
                handle,
                standard: std,
                fingerprint: fp,
                // find_by_email returns metadata-only refs; defaults are
                // fine: callers call get() for full material + algorithm.
                usage: KeyUsage::SignAndEncrypt,
                algorithm: "unknown".to_string(),
            });
        }
        Ok(out)
    }

    /// Sync body of [`KeyStore::remove`].
    ///
    /// # Errors
    /// Fails when the handle is malformed or the delete cannot run.
    pub fn remove_sync(&self, handle: &KeyHandle) -> CryptoResult<()> {
        let key_id = match handle {
            KeyHandle::Software(id) => id,
            KeyHandle::Token { .. } => return Ok(()),
        };
        let (standard, fingerprint) = Self::decode_key_id(key_id)?;

        // Delegate to the store layer's account-scoped delete: scoped to
        // this keystore's bound account, a cross-account remove deletes 0
        // rows.
        crate::delete_crypto_key(
            self.store,
            &self.account_id,
            standard.as_str(),
            fingerprint.as_str(),
        )
        .map_err(|e| CryptoError::backend(DbError(e)))?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl KeyStore for SqliteKeyStore<'_> {
    async fn put(&self, key: StoredKey) -> CryptoResult<KeyHandleRef> {
        self.put_sync(key)
    }

    async fn get(&self, handle: &KeyHandle) -> CryptoResult<Option<StoredKey>> {
        self.get_sync(handle)
    }

    async fn find_by_email(
        &self,
        standard: Standard,
        email: &str,
    ) -> CryptoResult<Vec<KeyHandleRef>> {
        self.find_by_email_sync(standard, email)
    }

    async fn remove(&self, handle: &KeyHandle) -> CryptoResult<()> {
        self.remove_sync(handle)
    }
}

#[cfg(test)]
#[path = "keystore_tests.rs"]
mod keystore_tests;
