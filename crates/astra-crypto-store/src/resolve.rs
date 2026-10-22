// SPDX-License-Identifier: GPL-3.0-only
//! Recipient trust resolution: kylins' `mail/crypto/trust.rs` ported onto
//! the sync store. The resolution order (Thunderbird's Key Assistant
//! taxonomy) is the send gate's contract; every branch comment is
//! load-bearing.

use astra_crypto_core::{KeyHandleRef, Standard};

use crate::keystore::SqliteKeyStore;
use crate::store::CryptoStore;

/// Per-recipient readiness for encryption (the Thunderbird Key Assistant
/// model). Serializes KEBAB-CASE over IPC: the frontend union type must
/// match exactly.
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
    /// LEGACY/DISPLAY-ONLY: [`resolve_recipient_status_at`] maps the
    /// `unverified` decision to `Ready` (accepted-without-verification
    /// qualifies for encryption; the raw decision rides on
    /// `RecipientTrustStatus.decision` for display). The variant stays in
    /// the wire enum for backward compatibility.
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

/// Whether the candidate key material is expired at `now_unix`.
///
/// - S/MIME: `public_data` is hex-encoded cert DER (the keystore-bridge
///   encoding); parse via `x509-parser` and compare `notAfter`.
/// - OpenPGP: `public_data` is hex-encoded binary-or-armored cert material;
///   parse via Sequoia and run the cert under the WRITE policy at `now`:
///   `alive()` fails for an expired primary key (and for a revoked one;
///   both are "not usable for new encryption", the semantics the send gate
///   needs).
///
/// Parse failures return `false` (not expired): a corrupt blob fails later
/// at encrypt time with a precise backend error; the trust gate must not
/// invent an expiry.
pub fn key_material_expired(standard: Standard, public_data_hex: &str, now_unix: u64) -> bool {
    let Ok(bytes) = hex::decode(public_data_hex) else {
        return false;
    };
    match standard {
        Standard::Smime => {
            let Ok((_rem, cert)) = x509_parser::parse_x509_certificate(&bytes) else {
                return false;
            };
            let not_after = cert.validity().not_after.timestamp();
            u64::try_from(not_after).is_ok_and(|t| t <= now_unix)
        }
        Standard::OpenPgp => {
            let Ok(certs) = astra_crypto_openpgp::keymap::parse_certs(&bytes) else {
                return false;
            };
            let Some(cert) = certs.into_iter().next() else {
                return false;
            };
            let policy = astra_crypto_openpgp::policy::PgpPolicy::default();
            let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(now_unix);
            cert.with_policy(policy.write_policy(), now)
                .and_then(|vc| vc.alive())
                .is_err()
        }
        Standard::Sm => false,
    }
}

/// Pin-aware candidate selection (shared by [`resolve_recipient_status_at`]
/// and the send gate's recipient loop): when MULTIPLE candidate keys exist
/// for one email, prefer the contact's pinned fingerprint for this standard
/// (the `contact_pinned_keys` table). The pin picks WHICH key;
/// expiry/acceptance evaluation is the caller's job, so a pin to an expired
/// key surfaces `Expired` rather than masking it behind another candidate. A
/// pin that no longer resolves (key deleted) falls back to the first
/// candidate. Pin-read failures degrade to the unpinned order (never mask a
/// key as missing). Single-candidate inputs return the first handle
/// unchanged (no pin lookup).
///
/// Load-bearing: the trust gate and the actual encrypt recipient set MUST
/// select the same handle; a gate that passes on pinned K2 while encrypting
/// to first-sorted K1 (rejected/expired) produces ciphertext the recipient
/// can never read.
pub fn pin_preferred_handle(
    store: &CryptoStore,
    standard: Standard,
    email: &str,
    handles: &[KeyHandleRef],
) -> Option<KeyHandleRef> {
    let first = handles.first()?;
    if handles.len() == 1 {
        return Some(first.clone());
    }
    let pinned_fp = crate::list_pinned_keys(store, email)
        .unwrap_or_default()
        .into_iter()
        .find(|p| p.standard == standard.as_str())
        .map(|p| p.fingerprint);
    let chosen = pinned_fp
        .and_then(|fp| {
            handles
                .iter()
                .find(|h| h.fingerprint.as_str().eq_ignore_ascii_case(&fp))
        })
        .unwrap_or(first);
    Some(chosen.clone())
}

/// Time-injectable core of [`resolve_recipient_status`]: `now_unix` is the
/// reference time for the expiry check. Split out so tests can reach the
/// [`RecipientTrustState::Expired`] arm without fixture certs whose validity
/// window has actually elapsed.
///
/// Resolution order (mirrors Thunderbird's Key Assistant taxonomy):
/// 1. Keystore lookup by email (account-scoped fetchable). Miss: check
///    `collected_keys`; presence gives `NoKey` WITH the staged fingerprint
///    as an import hint, absence gives `NoKey` with no hint.
/// 2. Key found: expiry FIRST (an expired key is unusable regardless of
///    acceptance), then [`RecipientTrustState::Expired`].
/// 3. Own private key: `Ready` (Personal).
/// 4. Latest trust decision for `(account, email, standard, fp)`:
///    verified/personal/unverified: `Ready`; rejected: `Rejected`;
///    undecided: `Undecided`. (Thunderbird model: `unverified` =
///    accepted-without-verification and QUALIFIES for encryption; only the
///    display differs, so the raw decision rides on `status.decision`.)
/// 5. No decision: if the same fingerprint has `verified` decisions for
///    OTHER peer emails, `AcceptedOtherEmail` with `accepted_elsewhere`
///    populated; else `Undecided` (the implicit default; no row is written).
pub fn resolve_recipient_status_at(
    store: &CryptoStore,
    account_id: &str,
    standard: Standard,
    email: &str,
    now_unix: u64,
) -> RecipientTrustStatus {
    let mut status = RecipientTrustStatus {
        email: email.to_string(),
        standard: standard.as_str().to_string(),
        state: RecipientTrustState::NoKey,
        fingerprint: None,
        decision: None,
        accepted_elsewhere: Vec::new(),
        key_expired: false,
    };

    // 1. Keystore lookup (account-scoped fetchable: a key this account
    //    cannot serve is a miss, same as the send path). The store is sync,
    //    so the bridge's sync inherent method serves here directly.
    let keystore = SqliteKeyStore::from_store(store, account_id);
    let handles = keystore
        .find_by_email_sync(standard, email)
        .unwrap_or_default();
    let Some(handle) = pin_preferred_handle(store, standard, email, &handles) else {
        // Collected-keys hint: a staged-but-unimported key stays `NoKey`,
        // but carries its fingerprint so the UI can offer one-click import.
        if let Ok(collected) =
            crate::list_collected_keys_for_peer(store, account_id, email, standard.as_str())
        {
            status.fingerprint = collected.first().and_then(|row| row.fingerprint.clone());
        }
        return status;
    };

    let fp = handle.fingerprint.as_str().to_string();
    status.fingerprint = Some(fp.clone());

    // 2. Expiry (before acceptance: acceptance of an expired key is moot).
    //    One account-scoped fetch yields both the public blob (expiry parse)
    //    and the `has_private` flag (step 3).
    let key_row = crate::get_crypto_key_public_with_blob(store, account_id, standard.as_str(), &fp)
        .ok()
        .flatten();
    if let Some((_, public_hex)) = &key_row {
        if key_material_expired(standard, public_hex, now_unix) {
            status.state = RecipientTrustState::Expired;
            status.key_expired = true;
            return status;
        }
    }

    // 3. Own key with private material: Personal, so Ready.
    if key_row.as_ref().is_some_and(|(row, _)| row.has_private) {
        status.state = RecipientTrustState::Ready;
        return status;
    }

    // 4. Latest explicit decision for THIS (email, fingerprint) pair.
    let decision =
        crate::get_latest_trust_decision(store, account_id, email, standard.as_str(), &fp)
            .ok()
            .flatten()
            .map(|row| row.decision);
    match decision.as_deref() {
        // Thunderbird model: `unverified` = accepted-without-verification
        // and QUALIFIES for encryption alongside verified/personal; only the
        // display differs. The state resolves `Ready`; the raw decision
        // rides along so display surfaces can still render the
        // not-yet-verified glyph.
        Some("verified" | "personal" | "unverified") => {
            status.decision.clone_from(&decision);
            status.state = RecipientTrustState::Ready;
            return status;
        }
        Some("rejected") => {
            status.decision.clone_from(&decision);
            status.state = RecipientTrustState::Rejected;
            return status;
        }
        // "undecided" (explicit) falls through to the same state as no row;
        // unknown legacy strings are treated as no decision.
        _ => {}
    }

    // 5. Accepted-for-a-different-address detection: the same fingerprint is
    //    `verified` for other peer email(s).
    if let Ok(latest_per_email) =
        crate::list_latest_decisions_for_key(store, account_id, standard.as_str(), &fp)
    {
        status.accepted_elsewhere = latest_per_email
            .into_iter()
            .filter(|row| {
                row.decision == "verified" && !row.peer_email.is_empty() && row.peer_email != email
            })
            .map(|row| row.peer_email)
            .collect();
    }
    status.state = if status.accepted_elsewhere.is_empty() {
        RecipientTrustState::Undecided
    } else {
        RecipientTrustState::AcceptedOtherEmail
    };
    status
}

/// Resolve the per-recipient encryption-readiness status for `(account_id,
/// standard, email)` at the current time. Never errors: DB failures degrade
/// to the fail-closed `NoKey`/`Undecided` states rather than masking a hard
/// failure as sendable. See [`resolve_recipient_status_at`] for the
/// resolution order.
pub fn resolve_recipient_status(
    store: &CryptoStore,
    account_id: &str,
    standard: Standard,
    email: &str,
) -> RecipientTrustStatus {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    resolve_recipient_status_at(store, account_id, standard, email, now_unix)
}

/// Run the trust gate over a recipient list. Returns the list of
/// [`RecipientTrustFailure`]s (one per non-`Ready` recipient); an empty Vec
/// means every recipient may be encrypted to. Shared by the fail-closed send
/// gate; the batch resolver uses [`resolve_recipient_status`] directly.
pub fn recipient_trust_failures(
    store: &CryptoStore,
    account_id: &str,
    standard: Standard,
    emails: &[String],
) -> Vec<RecipientTrustFailure> {
    let mut failures = Vec::new();
    for email in emails {
        let status = resolve_recipient_status(store, account_id, standard, email);
        if status.state != RecipientTrustState::Ready {
            failures.push(RecipientTrustFailure {
                email: status.email,
                state: status.state,
                fingerprint: status.fingerprint,
            });
        }
    }
    failures
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod resolve_tests;
