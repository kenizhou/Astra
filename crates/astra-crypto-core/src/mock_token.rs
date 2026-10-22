// SPDX-License-Identifier: GPL-3.0-only
//! Closure-driven [`TokenProvider`] mock for hardware-free testing (W6).
//!
//! Compiled only under `cfg(test)` or the `testing` Cargo feature (sibling
//! crates enable it via dev-dependencies). The mock emulates the device-side
//! contract - presence gating, PIN session state, typed errors - while the
//! actual cryptography is injected per test as closures, so each consumer
//! crate signs/decrypts with its own stack (`p256`/`rsa` in crypto-smime,
//! Sequoia in crypto-openpgp) and exercises the REAL token-routed code paths
//! end to end.
//!
//! ```ignore
//! let mock = MockTokenProvider::pkcs11("TOKEN-1", "Correct Horse");
//! mock.add_key(TokenKeyInfo { key_id: TokenKeyId("01".into()), .. });
//! mock.on_sign(move |key, mech, digest| Ok(rsa_sign(digest)));
//! // …route a SmimeBackend sign through `mock`…
//! ```

use std::collections::HashSet;
use std::sync::Mutex;

use crate::error::{CryptoError, Result};
use crate::handle::KeyUsage;
use crate::ids::TokenKeyId;
use crate::secret::SecretBox;
use crate::token::{
    TokenInfo, TokenKeyInfo, TokenKeyRef, TokenKind, TokenProvider, TokenSignMechanism,
};

type SignFn = dyn Fn(&TokenKeyRef, TokenSignMechanism, &[u8]) -> Result<Vec<u8>> + Send + Sync;
type DecryptFn = dyn Fn(&TokenKeyRef, &[u8]) -> Result<Vec<u8>> + Send + Sync;

struct Entry {
    info: TokenInfo,
    keys: Vec<TokenKeyInfo>,
}

/// A configurable software stand-in for a hardware token provider.
pub struct MockTokenProvider {
    kind: TokenKind,
    /// Expected PIN (shared by every emulated token).
    pin: String,
    entries: Mutex<Vec<Entry>>,
    /// Serials with a cached session PIN.
    unlocked: Mutex<HashSet<String>>,
    /// Serials whose cached PIN is single-use (dropped after one private op).
    single_use: Mutex<HashSet<String>>,
    sign_fn: Mutex<Option<Box<SignFn>>>,
    decrypt_fn: Mutex<Option<Box<DecryptFn>>>,
    /// Every `sign` call witnessed, for assertions.
    sign_calls: Mutex<Vec<(TokenKeyRef, TokenSignMechanism, Vec<u8>)>>,
    /// Every `decrypt` call witnessed, for assertions.
    decrypt_calls: Mutex<Vec<(TokenKeyRef, Vec<u8>)>>,
}

impl MockTokenProvider {
    /// Create a mock with one present token (`serial`, label derived).
    pub fn pkcs11(serial: &str, pin: &str) -> Self {
        Self::new(TokenKind::Pkcs11, serial, pin)
    }

    /// Create a mock OpenPGP-card provider with one present card.
    pub fn openpgp_card(serial: &str, pin: &str) -> Self {
        Self::new(TokenKind::OpenPgpCard, serial, pin)
    }

    fn new(kind: TokenKind, serial: &str, pin: &str) -> Self {
        let entries = vec![Entry {
            info: TokenInfo {
                serial: serial.to_string(),
                label: format!("Mock token {serial}"),
                kind,
                unlocked: false,
            },
            keys: Vec::new(),
        }];
        Self {
            kind,
            pin: pin.to_string(),
            entries: Mutex::new(entries),
            unlocked: Mutex::new(HashSet::new()),
            single_use: Mutex::new(HashSet::new()),
            sign_fn: Mutex::new(None),
            decrypt_fn: Mutex::new(None),
            sign_calls: Mutex::new(Vec::new()),
            decrypt_calls: Mutex::new(Vec::new()),
        }
    }

    /// Register a key on the (first) emulated token.
    pub fn add_key(&self, key: TokenKeyInfo) {
        let mut entries = self.entries.lock().expect("entries poisoned");
        entries
            .first_mut()
            .expect("mock always has one token at construction")
            .keys
            .push(key);
    }

    /// Convenience: register a key with just an id + usage.
    pub fn add_key_id(&self, key_id: &str, algorithm: &str, usage: KeyUsage) {
        self.add_key(TokenKeyInfo {
            key_id: TokenKeyId(key_id.to_string()),
            label: format!("key {key_id}"),
            algorithm: algorithm.to_string(),
            usage,
            public_data: None,
        });
    }

    /// Inject the sign operation (the "device's" crypto).
    pub fn on_sign<F>(&self, f: F)
    where
        F: Fn(&TokenKeyRef, TokenSignMechanism, &[u8]) -> Result<Vec<u8>> + Send + Sync + 'static,
    {
        *self.sign_fn.lock().expect("sign_fn poisoned") = Some(Box::new(f));
    }

    /// Inject the decrypt operation (the "device's" crypto).
    pub fn on_decrypt<F>(&self, f: F)
    where
        F: Fn(&TokenKeyRef, &[u8]) -> Result<Vec<u8>> + Send + Sync + 'static,
    {
        *self.decrypt_fn.lock().expect("decrypt_fn poisoned") = Some(Box::new(f));
    }

    /// Simulate hot-plug removal: every subsequent op on `serial` fails with
    /// `TokenNotPresent` and any cached session PIN is dropped.
    pub fn remove_token(&self, serial: &str) {
        self.entries
            .lock()
            .expect("entries poisoned")
            .retain(|e| e.info.serial != serial);
        self.unlocked
            .lock()
            .expect("unlocked poisoned")
            .remove(serial);
    }

    /// Re-insert a previously removed token (no keys).
    pub fn reinsert_token(&self, serial: &str, label: &str) {
        self.entries.lock().expect("entries poisoned").push(Entry {
            info: TokenInfo {
                serial: serial.to_string(),
                label: label.to_string(),
                kind: self.kind,
                unlocked: false,
            },
            keys: Vec::new(),
        });
    }

    /// Recorded `sign` calls (key, mechanism, digest), oldest first.
    pub fn sign_calls(&self) -> Vec<(TokenKeyRef, TokenSignMechanism, Vec<u8>)> {
        self.sign_calls.lock().expect("sign_calls poisoned").clone()
    }

    /// Recorded `decrypt` calls (key, ciphertext), oldest first.
    pub fn decrypt_calls(&self) -> Vec<(TokenKeyRef, Vec<u8>)> {
        self.decrypt_calls
            .lock()
            .expect("decrypt_calls poisoned")
            .clone()
    }

    fn require_present(&self, serial: &str) -> Result<()> {
        if self
            .entries
            .lock()
            .expect("entries poisoned")
            .iter()
            .any(|e| e.info.serial == serial)
        {
            Ok(())
        } else {
            Err(CryptoError::TokenNotPresent(format!(
                "mock token {serial} is not inserted"
            )))
        }
    }

    /// Gate a private op on presence + PIN session; consumes a single-use
    /// PIN on success.
    fn gate_private_op(&self, key: &TokenKeyRef) -> Result<()> {
        self.require_present(&key.token_serial)?;
        if !self
            .unlocked
            .lock()
            .expect("unlocked poisoned")
            .contains(&key.token_serial)
        {
            return Err(CryptoError::TokenPinRequired(format!(
                "mock token {} requires PIN",
                key.token_serial
            )));
        }
        Ok(())
    }

    fn consume_single_use(&self, serial: &str) {
        let mut single = self.single_use.lock().expect("single_use poisoned");
        if single.remove(serial) {
            self.unlocked
                .lock()
                .expect("unlocked poisoned")
                .remove(serial);
        }
    }
}

impl TokenProvider for MockTokenProvider {
    fn kind(&self) -> TokenKind {
        self.kind
    }

    fn list_tokens(&self) -> Result<Vec<TokenInfo>> {
        let unlocked = self.unlocked.lock().expect("unlocked poisoned");
        Ok(self
            .entries
            .lock()
            .expect("entries poisoned")
            .iter()
            .map(|e| TokenInfo {
                unlocked: unlocked.contains(&e.info.serial),
                ..e.info.clone()
            })
            .collect())
    }

    fn list_keys(&self, token_serial: &str) -> Result<Vec<TokenKeyInfo>> {
        self.require_present(token_serial)?;
        Ok(self
            .entries
            .lock()
            .expect("entries poisoned")
            .iter()
            .find(|e| e.info.serial == token_serial)
            .map(|e| e.keys.clone())
            .unwrap_or_default())
    }

    fn unlock(&self, token_serial: &str, pin: SecretBox<String>, remember: bool) -> Result<()> {
        self.require_present(token_serial)?;
        let supplied = secrecy::ExposeSecret::expose_secret(&pin);
        if supplied.as_str() != self.pin.as_str() {
            return Err(CryptoError::TokenPinIncorrect(format!(
                "mock token {token_serial} rejected the PIN (3 attempts remaining)"
            )));
        }
        self.unlocked
            .lock()
            .expect("unlocked poisoned")
            .insert(token_serial.to_string());
        if !remember {
            self.single_use
                .lock()
                .expect("single_use poisoned")
                .insert(token_serial.to_string());
        }
        Ok(())
    }

    fn lock(&self, token_serial: &str) -> Result<()> {
        self.unlocked
            .lock()
            .expect("unlocked poisoned")
            .remove(token_serial);
        Ok(())
    }

    fn sign(
        &self,
        key: &TokenKeyRef,
        mechanism: TokenSignMechanism,
        digest: &[u8],
    ) -> Result<Vec<u8>> {
        self.gate_private_op(key)?;
        self.sign_calls.lock().expect("sign_calls poisoned").push((
            key.clone(),
            mechanism,
            digest.to_vec(),
        ));
        let f = self.sign_fn.lock().expect("sign_fn poisoned");
        let f = f
            .as_ref()
            .ok_or_else(|| CryptoError::Token("mock: no sign closure injected (on_sign)".into()))?;
        let out = f(key, mechanism, digest);
        if out.is_ok() {
            self.consume_single_use(&key.token_serial);
        }
        out
    }

    fn decrypt(&self, key: &TokenKeyRef, ciphertext: &[u8]) -> Result<Vec<u8>> {
        self.gate_private_op(key)?;
        self.decrypt_calls
            .lock()
            .expect("decrypt_calls poisoned")
            .push((key.clone(), ciphertext.to_vec()));
        let f = self.decrypt_fn.lock().expect("decrypt_fn poisoned");
        let f = f.as_ref().ok_or_else(|| {
            CryptoError::Token("mock: no decrypt closure injected (on_decrypt)".into())
        })?;
        let out = f(key, ciphertext);
        if out.is_ok() {
            self.consume_single_use(&key.token_serial);
        }
        out
    }
}
