// SPDX-License-Identifier: GPL-3.0-only
use serde::{Deserialize, Serialize};

/// Five-value acceptance ladder (Thunderbird model). `Verified`, `Personal`,
/// and `Unverified` (accepted WITHOUT fingerprint verification) qualify a
/// recipient key for encryption - Thunderbird's `unverified` means
/// "accepted-without-verification" and is encryption-usable; only the display
/// differs from `Verified`. `Undecided` routes the recipient to the Key
/// Assistant; `Rejected` hard-blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustState {
    Rejected,
    Undecided,
    Unverified,
    Verified,
    Personal,
}

impl TrustState {
    /// A key at this level may be used to encrypt to a recipient without an
    /// explicit per-send confirmation. `Unverified` qualifies (Thunderbird
    /// model: accepted-without-verification is sendable); `Undecided` and
    /// `Rejected` do not.
    ///
    /// NOTE: this is the ENCRYPTION-gate predicate only. Signature DISPLAY
    /// (`valid-verified` vs `valid-unverified`) deliberately uses the stricter
    /// `Verified | Personal` check at the display-mapping sites - an
    /// unverified-accepted signer must never render as fully verified.
    pub fn may_encrypt_to(self) -> bool {
        matches!(
            self,
            TrustState::Unverified | TrustState::Verified | TrustState::Personal
        )
    }

    /// The stricter DISPLAY predicate: only `Verified` / `Personal` render a
    /// signature as `valid-verified`. Kept separate from
    /// [`TrustState::may_encrypt_to`] so loosening the encryption gate (the
    /// Thunderbird `unverified`-qualifies model) can never inflate signature
    /// display trust.
    pub fn displays_as_verified(self) -> bool {
        matches!(self, TrustState::Verified | TrustState::Personal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unverified_verified_and_personal_qualify_for_encryption() {
        // Thunderbird model: accepted-without-verification qualifies.
        assert!(TrustState::Unverified.may_encrypt_to());
        assert!(TrustState::Verified.may_encrypt_to());
        assert!(TrustState::Personal.may_encrypt_to());
        assert!(!TrustState::Undecided.may_encrypt_to());
        assert!(!TrustState::Rejected.may_encrypt_to());
    }

    #[test]
    fn only_verified_and_personal_display_as_verified() {
        assert!(!TrustState::Unverified.displays_as_verified());
        assert!(TrustState::Verified.displays_as_verified());
        assert!(TrustState::Personal.displays_as_verified());
        assert!(!TrustState::Undecided.displays_as_verified());
        assert!(!TrustState::Rejected.displays_as_verified());
    }
}
