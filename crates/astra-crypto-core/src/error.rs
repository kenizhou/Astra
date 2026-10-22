// SPDX-License-Identifier: GPL-3.0-only
use std::sync::Arc;

/// Type-erased crypto error. Backend-native errors are wrapped here so the
/// core contract exposes exactly one error type across all backends.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CryptoError {
    #[error("crypto backend error: {0}")]
    Backend(Arc<dyn std::error::Error + Send + Sync>),

    #[error("policy rejected algorithm: {0}")]
    Policy(String),

    #[error("key not found: {0}")]
    KeyNotFound(String),

    /// The ciphertext could not be decrypted by the key the caller supplied:
    /// no recipient/session-key slot matched ("wrong key"). This is distinct
    /// from a structural failure (corrupt packet, bad MDC, ...): a caller
    /// iterating candidate decryption keys SHOULD try the next key on this
    /// variant and reserve hard failure for every other variant.
    #[error("no matching key: {0}")]
    NoMatchingKey(String),

    /// Symmetric (passphrase) decryption failed because the passphrase did
    /// not fit any SKESK slot ("wrong passphrase"). Distinct from
    /// [`CryptoError::NoMatchingKey`] (a public-key slot mismatch): a caller
    /// holding a passphrase prompt SHOULD re-prompt on this variant, not try
    /// another key. W9 password-protected mail (SKESK).
    #[error("wrong passphrase: {0}")]
    WrongPassphrase(String),

    #[error("unsupported standard: {0}")]
    UnsupportedStandard(String),

    #[error("malformed input: {0}")]
    Malformed(String),

    /// W6 hardware tokens: the referenced token is not present (removed,
    /// never inserted, or the PKCS#11 module sees no such serial). Distinct
    /// from [`CryptoError::KeyNotFound`] (the key REGISTRATION exists but the
    /// device does not): the UI should prompt "insert your token" and offer a
    /// retry, not a key-import flow.
    #[error("token not present: {0}")]
    TokenNotPresent(String),

    /// W6 hardware tokens: a private op needs the token's PIN and no session
    /// PIN is cached. The UI should open the PIN dialog, then retry the op
    /// after a successful `unlock`.
    #[error("token PIN required: {0}")]
    TokenPinRequired(String),

    /// W6 hardware tokens: the PIN was presented to the device and rejected.
    /// The message SHOULD carry the remaining retry count when the device
    /// reports one (many tokens lock after N failures).
    #[error("token PIN incorrect: {0}")]
    TokenPinIncorrect(String),

    /// W6 hardware tokens: any other provider/device failure (module load,
    /// PC/SC transport, unsupported on-card algorithm). The typed variants
    /// above MUST be preferred whenever the condition is distinguishable.
    #[error("token error: {0}")]
    Token(String),

    #[error("not implemented: {0}")]
    NotImplemented(String),
}

impl CryptoError {
    /// Wrap a backend-native error behind the type-erased `Backend` variant.
    pub fn backend<E>(e: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        CryptoError::Backend(Arc::new(e))
    }
}

pub type Result<T> = std::result::Result<T, CryptoError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("boom: {0}")]
    struct FakeBackendErr(String);

    #[test]
    fn backend_error_is_type_erased_and_cloneable() {
        let err = CryptoError::backend(FakeBackendErr("detached tag mismatch".into()));
        let cloned = err.clone();
        // Display round-trips through the Arc'd inner error.
        let msg = format!("{cloned}");
        assert!(msg.contains("detached tag mismatch"));
        // It is NOT one of the other variants.
        assert!(!matches!(err, CryptoError::Policy(_)));
    }

    #[test]
    fn not_implemented_variant_displays() {
        let e = CryptoError::NotImplemented("Phase 1b receive".into());
        assert!(format!("{e}").contains("Phase 1b receive"));
        assert!(matches!(e, CryptoError::NotImplemented(_)));
    }

    #[test]
    fn no_matching_key_variant_is_typed_and_displays() {
        let e = CryptoError::NoMatchingKey("no PKESK matched".into());
        assert!(matches!(e, CryptoError::NoMatchingKey(_)));
        assert!(format!("{e}").contains("no PKESK matched"));
        // It is NOT conflated with a structural/Backend failure.
        assert!(!matches!(e, CryptoError::Backend(_)));
    }

    #[test]
    fn wrong_passphrase_variant_is_typed_and_displays() {
        let e = CryptoError::WrongPassphrase("no SKESK matched".into());
        assert!(matches!(e, CryptoError::WrongPassphrase(_)));
        assert!(format!("{e}").contains("no SKESK matched"));
        // It is NOT conflated with a wrong-key or structural failure.
        assert!(!matches!(e, CryptoError::NoMatchingKey(_)));
        assert!(!matches!(e, CryptoError::Backend(_)));
    }
}
