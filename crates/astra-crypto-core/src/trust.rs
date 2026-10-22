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

/// Per-recipient readiness for encryption (the Thunderbird Key Assistant
/// model). Serializes KEBAB-CASE over IPC: the frontend union type must
/// match exactly. Computed by the store's recipient resolver; carried by the
/// send gate's [`crate::apply::CryptoSendError::RecipientTrust`] payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecipientTrustState {
    /// Key present + `Verified`/`Personal`/`Unverified` decision (or own
    /// key): may encrypt to.
    Ready,
    /// Nothing in the keystore (or `collected_keys`) for this email. When a
    /// collected (staged, not-yet-imported) key exists, `fingerprint`
    /// carries it as a hint so the UI can offer one-click import.
    NoKey,
    /// Key present, no explicit acceptance decision.
    Undecided,
    /// Key present, explicitly accepted WITHOUT fingerprint verification.
    /// LEGACY/DISPLAY-ONLY: the resolver maps the `unverified` decision to
    /// `Ready` (accepted-without-verification qualifies for encryption; the
    /// raw decision rides on `RecipientTrustStatus.decision` for display).
    /// The variant stays in the wire enum for backward compatibility.
    Unverified,
    /// Key present, explicitly rejected: hard block.
    Rejected,
    /// Key present for this address, but acceptance (Verified) exists only
    /// for OTHER email address(es) on the same fingerprint: the user must
    /// explicitly accept the key for THIS address (Thunderbird
    /// `key-accepted-other`).
    AcceptedOtherEmail,
    /// The only candidate key is expired (S/MIME cert notAfter / OpenPGP
    /// `alive()` under the write policy).
    Expired,
}

impl RecipientTrustState {
    /// Stable string form (kebab-case, matching the serde wire value).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NoKey => "no-key",
            Self::Undecided => "undecided",
            Self::Unverified => "unverified",
            Self::Rejected => "rejected",
            Self::AcceptedOtherEmail => "accepted-other-email",
            Self::Expired => "expired",
        }
    }
}

/// Structured per-recipient resolution result: the contract the composer's
/// recipient-status UI consumes. JSON keys are camelCase; `state` is
/// kebab-case.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipientTrustStatus {
    /// The recipient address this status answers for.
    pub email: String,
    /// `'smime' | 'openpgp'`.
    pub standard: String,
    /// The resolved readiness state.
    pub state: RecipientTrustState,
    /// Best candidate key/cert fingerprint, when one was located (keystore
    /// hit, or the collected-keys hint for the `NoKey` case).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// The raw latest trust-decision string for the resolved
    /// `(email, fingerprint)` pair (`'verified' | 'unverified' | 'rejected' |
    /// 'undecided' | 'personal'`), when one exists. Carried alongside
    /// `state` so display surfaces keep the "only display differs"
    /// distinction: an `unverified` ACCEPTED key resolves `state=ready`
    /// but the chip can still render the not-yet-verified glyph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    /// Emails this fingerprint IS verified for: populated for
    /// `AcceptedOtherEmail` so the UI can say "accepted for alice@a.com,
    /// but not for alice@b.com".
    #[serde(default)]
    pub accepted_elsewhere: Vec<String>,
    /// True when the resolved state is `Expired`.
    pub key_expired: bool,
}

/// One failing recipient in a send-side trust-gate rejection. Carried by the
/// gate's `RecipientTrust` error so the frontend can open the recipient
/// resolution UI with per-recipient reasons instead of a bare error toast.
/// JSON keys are camelCase; `state` is kebab-case.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipientTrustFailure {
    /// The recipient address that failed the gate.
    pub email: String,
    /// Why it failed (anything but `Ready`).
    pub state: RecipientTrustState,
    /// The candidate fingerprint, when one was located.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
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
