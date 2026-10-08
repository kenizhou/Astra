// SPDX-License-Identifier: GPL-3.0-only
//! The host's OS-secure-store port for account credentials: the one path by which a
//! stored account's config TOML is written to or erased from the platform store
//! (Windows Credential Manager, the macOS keychain, the freedesktop secret service on
//! Linux). Keeping the write native is deliberate: encryption, access control and
//! prompting are the platform's job; deciding *when* a credential must be durable, and
//! what happens when it cannot be, is the caller's.
//!
//! Every method reports a result, because every platform store can refuse: a
//! Credential Manager entry can exceed its size limit, a keychain can be locked. A
//! write that cannot report a failure is not a report, and the account-add flow rests
//! on this answer: it persists **after** a successful dial and rolls the whole add
//! back when the store refuses, so an account is never half there.

use keyring::Entry;

/// The service name every Astra Mail credential is stored under: the one constant
/// that scopes this product's entries inside each platform store, beside the
/// per-account user name that keys each entry.
const SERVICE: &str = "astra";

/// Why a write to the platform's secure store did not happen.
///
/// One variant carrying the host's own message: the caller never branches on the
/// reason (there is nothing it could do differently for a locked keychain versus a
/// full one), it only needs to know the write did not land and to be able to say why.
#[derive(Debug, thiserror::Error)]
#[error("credential store: {0}")]
pub struct CredentialStoreError(pub(crate) String);

/// The host's secure-store writer for account credentials, and the only path by
/// which an account's stored credential is created, replaced or erased.
///
/// Implementations must be safe to call from any thread; the caller is an account
/// dial or an account-add on the app's runtime.
pub(crate) trait AccountCredentialStore: Send + Sync {
    /// Persists `config_toml` for `account_id`, replacing the stored entry.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialStoreError`] when the platform store refused the write.
    /// Report the failure rather than swallowing it: the caller's next step depends
    /// on this answer.
    fn persist(&self, account_id: &str, config_toml: &str) -> Result<(), CredentialStoreError>;

    /// Erases the stored entry for `account_id`. Deleting an id that is not stored is
    /// a success, not an error: the desired end state is that nothing is stored for
    /// it.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialStoreError`] when the platform store refused the delete.
    fn delete(&self, account_id: &str) -> Result<(), CredentialStoreError>;
}

/// The desktop host's [`AccountCredentialStore`] over the `keyring` crate: one entry
/// per account id under the [`SERVICE`] name, the secret being the account's config
/// TOML string (endpoints plus credentials; the same document the account-add flow
/// parsed).
#[derive(Debug, Default)]
pub(crate) struct KeyringCredentialStore;

impl KeyringCredentialStore {
    /// A store over this platform's secure store.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self
    }

    /// The stored config TOML for `account_id`, or `None` when nothing is stored for
    /// it: the boot's read of what the last run saved. Not part of the
    /// [`AccountCredentialStore`] port, which is the write path only; this is the
    /// host reading its own store.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialStoreError`] when the platform store could not be read.
    pub(crate) fn load(account_id: &str) -> Result<Option<String>, CredentialStoreError> {
        let entry = Self::entry(account_id)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(CredentialStoreError(err.to_string())),
        }
    }

    /// The platform entry for `account_id`.
    fn entry(account_id: &str) -> Result<Entry, CredentialStoreError> {
        Entry::new(SERVICE, account_id).map_err(|err| CredentialStoreError(err.to_string()))
    }
}

impl AccountCredentialStore for KeyringCredentialStore {
    fn persist(&self, account_id: &str, config_toml: &str) -> Result<(), CredentialStoreError> {
        Self::entry(account_id)?
            .set_password(config_toml)
            .map_err(|err| CredentialStoreError(err.to_string()))
    }

    fn delete(&self, account_id: &str) -> Result<(), CredentialStoreError> {
        match Self::entry(account_id)?.delete_credential() {
            // Deleting an id that is not stored is the desired end state already.
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(CredentialStoreError(err.to_string())),
        }
    }
}
