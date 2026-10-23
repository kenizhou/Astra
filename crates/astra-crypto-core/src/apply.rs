// SPDX-License-Identifier: GPL-3.0-only
//! The send gate: kylins' `mail/crypto/apply.rs` ported, with
//! `CryptoSendError`, the gate's draft slice, and the store-bound hooks
//! behind the [`SendCryptoPolicy`] port.
//!
//! Placement note (SP4b spec D2): the gate lives in the CORE crate so the
//! orchestration (trust gate -> protected headers -> gossip -> encrypt ->
//! wrap) stays beside the traits it drives. kylins threaded a raw db pool
//! through the gate; the Astra port injects [`SendCryptoPolicy`] instead, so
//! core keeps its dependency direction (the store implements the policy).
//! kylins' `ensure_token_registry_for_account` call is deliberately NOT
//! ported: the token registry is SP4d, and a token key with no registered
//! provider already fails typed (`TokenNotPresent`) in the backends.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CryptoError;
use crate::handle::{KeyHandle, KeyHandleRef, KeyUsage};
use crate::ids::{Fingerprint, KeyId, TokenKeyId};
use crate::keystore::KeyStore;
use crate::mime::{
    ensure_one_trailing_crlf, hide_subject_header, split_message, wrap_enveloped,
    wrap_multipart_signed, wrap_protected_headers,
};
use crate::policy::HashAlgorithm;
use crate::standard::Standard;
use crate::trust::RecipientTrustFailure;
use crate::{
    pgp_mime, CryptoBackend, EncryptOp, Part, PartId, PartKind, SerializationStrategy, SignOp,
};

/// Errors from the send-gate crypto layer.
#[derive(Debug)]
pub enum CryptoSendError {
    /// `sign` was requested but the account has no default signing key.
    NoSigningKey,
    /// Fail-closed: no recipient cert/key resolved for the email. The inner
    /// String is a fully-formatted, standard-aware message (e.g.
    /// `"no S/MIME cert for recipient bob@x.com"` or
    /// `"no OpenPGP key for recipient bob@x.com"`); the Display impl emits
    /// it verbatim so callers can match on the standard-specific phrase.
    MissingRecipientCert(String),
    /// A recipient cert was found in the keystore but failed chain
    /// validation (expired, wrong-issuer, missing EKU/SAN, or no anchor
    /// could be loaded to confirm it). Fail-closed: no ciphertext is
    /// produced.
    InvalidRecipientCert(String),
    /// Fail-closed (the trust gate): one or more recipients did not reach
    /// `Ready` when encryption was requested. Carries the per-recipient
    /// structured payload so the caller can surface it for the UI. No
    /// ciphertext is produced: `backend.encrypt` is never reached.
    RecipientTrust(Vec<RecipientTrustFailure>),
    /// Fail-closed (password mode): the draft requested password (symmetric
    /// SKESK) encryption but no usable passphrase was available. No
    /// ciphertext is produced; the composer must re-collect the passphrase
    /// and re-send.
    PasswordRequired,
    /// Fail-closed (password mode): the draft set BOTH `password_protected`
    /// and `encrypt`. The modes are mutually exclusive per draft.
    PasswordConflict,
    /// The crypto backend failed.
    Backend(CryptoError),
    /// The assembled message could not be split/parsed for wrapping.
    Mime(String),
}

impl std::fmt::Display for CryptoSendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSigningKey => write!(f, "no default signing key for the account"),
            // Inner String is already a fully-formatted, standard-aware message.
            Self::MissingRecipientCert(e) => write!(f, "{e}"),
            Self::InvalidRecipientCert(e) => {
                write!(f, "invalid recipient cert: {e}")
            }
            Self::RecipientTrust(failures) => {
                write!(f, "recipients not ready for encryption: ")?;
                for (i, failure) in failures.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{} ({})", failure.email, failure.state.as_str())?;
                }
                Ok(())
            }
            Self::PasswordRequired => write!(
                f,
                "password encryption requested but no passphrase is available; \
                 re-enter the password and send again"
            ),
            Self::PasswordConflict => write!(
                f,
                "password encryption and key encryption are mutually exclusive; \
                 the draft requested both"
            ),
            Self::Backend(e) => write!(f, "crypto backend: {e}"),
            Self::Mime(s) => write!(f, "mime: {s}"),
        }
    }
}

impl std::error::Error for CryptoSendError {}

impl From<CryptoError> for CryptoSendError {
    fn from(e: CryptoError) -> Self {
        Self::Backend(e)
    }
}

/// One address on the draft the gate wraps.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddressSpec {
    /// Display name, when the composer set one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The mailbox the gate resolves keys for.
    pub email: String,
}

/// The slice of the composer's send draft the gate reads. The full draft
/// shape belongs to the app layer; the gate needs only the recipient sets,
/// the crypto toggles, and the per-draft standard override.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendDraft {
    /// To recipients.
    pub to: Vec<AddressSpec>,
    /// Cc recipients.
    #[serde(default)]
    pub cc: Vec<AddressSpec>,
    /// Bcc recipients (encrypted-to, never gossiped).
    #[serde(default)]
    pub bcc: Vec<AddressSpec>,
    /// Sign the message.
    #[serde(default)]
    pub sign: bool,
    /// Encrypt the message.
    #[serde(default)]
    pub encrypt: bool,
    /// Per-draft standard override (`'openpgp'`/`'smime'`/…); the caller owns
    /// standard SELECTION and must dispatch the backend for the winning
    /// standard: the gate fail-closes a mismatch (caller bug).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_override: Option<String>,
}

/// The account-level crypto method (`none | openpgp | smime`), the
/// `crypto_method` setting's typed form.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CryptoMethod {
    /// No crypto (the zero-cost default).
    #[default]
    None,
    /// S/MIME.
    Smime,
    /// OpenPGP.
    Openpgp,
}

impl CryptoMethod {
    /// DB column string. Matches [`Standard::as_str`] for the non-None
    /// variants (the send path dispatches on `standard()`).
    pub fn as_db_str(self) -> &'static str {
        match self {
            CryptoMethod::None => "none",
            CryptoMethod::Smime => "smime",
            CryptoMethod::Openpgp => "openpgp",
        }
    }

    /// Parse the DB column string. Unknown / empty -> `None` (defensive
    /// against legacy rows or future variants, e.g. `"sm"`).
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "smime" => CryptoMethod::Smime,
            "openpgp" => CryptoMethod::Openpgp,
            _ => CryptoMethod::None,
        }
    }

    /// Map to the dispatch enum. `None` -> `None` (no crypto backend
    /// selected); the other variants map 1:1.
    pub fn standard(self) -> Option<Standard> {
        match self {
            CryptoMethod::None => None,
            CryptoMethod::Smime => Some(Standard::Smime),
            CryptoMethod::Openpgp => Some(Standard::OpenPgp),
        }
    }
}

/// The gate's view of the account's default signing key (the store's
/// `DefaultKeyRow` converts into this; core must not name store types).
#[derive(Debug, Clone, Default)]
pub struct DefaultSigningKey {
    /// `'openpgp' | 'smime' | 'sm'`.
    pub standard: String,
    /// The default key's fingerprint.
    pub fingerprint: String,
    /// The identity the key signs for.
    pub email: Option<String>,
    /// Present when the key lives on a hardware token.
    pub token_serial: Option<String>,
    /// The key's id on the token, when token-backed.
    pub token_key_id: Option<String>,
}

/// Build a [`KeyHandleRef`] for the default signing key whose `KeyId`
/// matches the keystore bridge's canonical `"standard|fingerprint"`
/// encoding (so a later `backend.sign(... signing_key: this ...)` resolves
/// via `keystore.get`). A row with token fields set builds the Token handle
/// so the backend routes the private op to the device.
///
/// `algorithm` is fixed per standard: S/MIME -> `ECDSA-P256` (the only
/// signing algorithm the S/MIME backend's sign supports), PGP -> `default`
/// (the sentinel for the Ed25519 primary + signing subkey shape).
pub fn key_handle_ref(row: &DefaultSigningKey) -> KeyHandleRef {
    let standard = Standard::from_str(&row.standard).unwrap_or({
        // Defensive default: a row with an unknown standard fails the
        // downstream keystore.get lookup cleanly rather than panic here.
        Standard::Smime
    });
    let algorithm: &'static str = match standard {
        Standard::Smime => "ECDSA-P256",
        Standard::OpenPgp => "default",
        Standard::Sm => "unknown",
    };
    let handle = match (&row.token_serial, &row.token_key_id) {
        (Some(serial), Some(key_id)) => KeyHandle::Token {
            token_serial: serial.clone(),
            key_id: TokenKeyId(key_id.clone()),
        },
        _ => KeyHandle::Software(KeyId(format!("{}|{}", row.standard, row.fingerprint))),
    };
    KeyHandleRef {
        handle,
        standard,
        fingerprint: Fingerprint::new(&row.fingerprint),
        usage: KeyUsage::SignAndEncrypt,
        algorithm: algorithm.into(),
    }
}

/// The store-bound hooks the gate needs, injected so core keeps its
/// dependency direction (the crypto store implements this for an
/// account-bound view). Every method mirrors a kylins query the gate called
/// on the raw pool.
#[async_trait::async_trait]
pub trait SendCryptoPolicy: Send + Sync {
    /// Run the trust gate over a recipient list: one
    /// [`RecipientTrustFailure`] per non-`Ready` recipient; an empty Vec
    /// means every recipient may be encrypted to. Never errors (DB failures
    /// degrade to fail-closed states).
    async fn recipient_trust_failures(
        &self,
        standard: Standard,
        emails: &[String],
    ) -> Vec<RecipientTrustFailure>;

    /// Pin-aware candidate selection: when MULTIPLE candidate keys exist
    /// for one email, prefer the contact's pinned fingerprint for this
    /// standard. Load-bearing: the trust gate and the encrypt recipient set
    /// MUST select the same handle.
    async fn pin_preferred_handle(
        &self,
        standard: Standard,
        email: &str,
        handles: &[KeyHandleRef],
    ) -> Option<KeyHandleRef>;

    /// The account's S/MIME trust-anchor DERs (every `key_type='cert'` row,
    /// with the harvested/LDAP exclusion semantics).
    ///
    /// # Errors
    /// Fails on a store error.
    async fn trust_anchor_ders(&self) -> Result<Vec<Vec<u8>>, String>;

    /// Validate recipient certs against the trust anchors at `now_unix`
    /// (the same chain validator the receive side runs). `Ok(())` when
    /// every recipient cert chains, is in its validity window, and has the
    /// S/MIME BR leaf shape.
    ///
    /// # Errors
    /// A message naming the first failing recipient cert.
    async fn validate_recipient_certs(
        &self,
        recipient_cert_ders: &[Vec<u8>],
        trust_anchor_ders: &[Vec<u8>],
        now_unix: u64,
    ) -> Result<(), String>;

    /// Whether the account protects the Subject header on outgoing
    /// ENCRYPTED mail (default ON, and fail-safe to ON on a read error at
    /// the call site).
    ///
    /// # Errors
    /// Fails on a store error.
    async fn protect_subject(&self) -> Result<bool, String>;

    /// The Autocrypt-Gossip injection (OpenPGP, sign+encrypt only): prepend
    /// one `Autocrypt-Gossip:` header per non-sender recipient to the inner
    /// entity when the account's setting and TB's gating allow. Returns the
    /// (possibly unchanged) entity. Best-effort: per-recipient failures
    /// skip that recipient and never block a send.
    async fn maybe_prepend_gossip_headers(
        &self,
        backend: &dyn CryptoBackend,
        entity: &[u8],
        account_email: &str,
        recipient_emails: &[String],
        recipients: &[KeyHandleRef],
    ) -> Vec<u8>;
}

/// Build the standard-aware "no {standard} cert/key for recipient {email}"
/// message carried by [`CryptoSendError::MissingRecipientCert`].
pub fn missing_recipient_msg(standard: Standard, email: &str) -> String {
    let noun = match standard {
        Standard::Smime => "S/MIME cert",
        Standard::OpenPgp => "OpenPGP key",
        Standard::Sm => "SM cert",
    };
    format!("no {noun} for recipient {email}")
}

/// Split a body MIME entity into `(content_type_value, body_bytes)` for the
/// PGP sign-only path. `pgp_mime::wrap_signed` takes these SEPARATELY (vs
/// the S/MIME `wrap_multipart_signed` which takes the whole part-1 entity as
/// one slice), because it reconstructs the part-1 entity internally as
/// `Content-Type: {mime}\r\n\r\n{canonicalized body}`.
///
/// The Content-Type value may be folded across continuation lines (RFC 5322
/// §2.2.3); folding is unfolded by concatenating continuation lines
/// (whitespace trimmed from the join). Other entity headers are NOT
/// preserved in the PGP signed part-1 (see the kylins doc note this ports).
///
/// # Errors
/// [`CryptoSendError::Mime`] when the entity is not UTF-8, has no blank
/// line, or carries no Content-Type header.
pub fn split_entity_for_pgp_signed(entity: &[u8]) -> Result<(String, Vec<u8>), CryptoSendError> {
    let s = std::str::from_utf8(entity)
        .map_err(|e| CryptoSendError::Mime(format!("entity not utf-8: {e}")))?;
    let blank = s
        .find("\r\n\r\n")
        .ok_or_else(|| CryptoSendError::Mime("entity has no header/body blank line".into()))?;
    let header_block = &s[..blank];
    let body = s.as_bytes()[blank + 4..].to_vec();

    // Walk header lines; unfold RFC 5322 continuations (lines starting with
    // space/tab) into the previous header's value.
    let mut content_type_value: Option<String> = None;
    let mut in_content_type = false;
    for line in header_block.split("\r\n") {
        let is_continuation = line.starts_with(' ') || line.starts_with('\t');
        if is_continuation {
            if in_content_type {
                if let Some(ref mut ct) = content_type_value {
                    ct.push_str(line.trim_start());
                }
            }
            // Continuations of non-CT headers are ignored.
        } else if let Some(rest) = line
            .strip_prefix("Content-Type:")
            .or_else(|| line.strip_prefix("content-type:"))
        {
            in_content_type = true;
            content_type_value = Some(rest.trim().to_string());
        } else {
            in_content_type = false;
        }
    }
    let content_type_value = content_type_value.ok_or_else(|| {
        CryptoSendError::Mime("entity has no Content-Type header (required for PGP sign)".into())
    })?;
    Ok((content_type_value, body))
}

/// Build the exact part-1 entity bytes that `pgp_mime::wrap_signed` emits,
/// for signing: `Content-Type: {content_type_value}\r\n\r\n{body_canonical}`
/// where `body_canonical` is the body with exactly one trailing CRLF (caller
/// pre-canonicalizes). The signature computed over these bytes verifies
/// against the part-1 region `wrap_signed` reconstructs.
pub fn build_pgp_signed_region(content_type_value: &str, body_canonical: &[u8]) -> Vec<u8> {
    let mut out = format!("Content-Type: {content_type_value}\r\n\r\n").into_bytes();
    out.extend_from_slice(body_canonical);
    out
}

/// Apply sign/encrypt to a built MIME message under the supplied `standard`.
/// Returns the wrapped bytes (or the input unchanged when neither `sign`
/// nor `encrypt` is set; standard-selection is the caller's job).
///
/// The single `&dyn CryptoBackend` code path dispatches encrypt/sign via
/// the trait; the per-standard difference is only in the MIME WRAPPING of
/// the backend's output bytes:
/// - **S/MIME** encrypt -> `application/pkcs7-mime; smime-type=enveloped-data`
///   via [`wrap_enveloped`]; sign -> `multipart/signed` via
///   [`wrap_multipart_signed`] (Thunderbird-faithful).
/// - **OpenPGP** encrypt -> `multipart/encrypted;
///   protocol="application/pgp-encrypted"` via [`pgp_mime::wrap_encrypted`]
///   (RFC 3156 §2); sign -> `multipart/signed;
///   protocol="application/pgp-signature"` via [`pgp_mime::wrap_signed`]
///   (RFC 3156 §1).
///
/// - `sign` -> detached-sign the body entity and wrap as `multipart/signed`.
/// - `encrypt` -> encrypt the (possibly signed) body entity; the sender
///   (`account_email`) is added as a recipient (encrypt-to-self).
/// - sign + encrypt -> `EncryptOp.sign_with = signer` (the backend composes
///   sign-over-plaintext-then-encrypt internally: S/MIME opaque SignedData
///   inside EnvelopedData; PGP inline-signed ciphertext). Clear-signing
///   first and then encrypting would drop the signature on receive, so that
///   composition is never used.
///
/// Fail-closed on missing OR invalid recipient certs/keys: any To/Cc/Bcc
/// (or the sender) whose key `find_by_email` cannot resolve produces
/// `MissingRecipientCert`; under S/MIME, any resolved recipient cert that
/// does NOT chain to a configured trust anchor for the account (or is
/// expired / missing the S/MIME BR leaf shape) produces
/// `InvalidRecipientCert`. PGP inherits the missing-recipient fail-closed
/// but NOT the cert-chain validation (Web-of-Trust, deferred). In all
/// failure cases no ciphertext is emitted: `backend.encrypt` is never
/// reached.
///
/// # Errors
/// [`CryptoSendError`] per the fail-closed arms above.
#[allow(clippy::too_many_arguments)] // orchestrator surface; bundling adds ceremony without clarity
pub async fn apply_crypto(
    backend: &dyn CryptoBackend,
    standard: Standard,
    keystore: &dyn KeyStore,
    mime: &[u8],
    draft: &SendDraft,
    account_email: &str,
    default_signing_key: Option<&DefaultSigningKey>,
    policy: &dyn SendCryptoPolicy,
) -> Result<Vec<u8>, CryptoSendError> {
    if !draft.sign && !draft.encrypt {
        return Ok(mime.to_vec());
    }

    // Contract guard: the caller owns standard SELECTION (it resolves
    // `draft.standard_override` against the account default and builds the
    // backend for the winning standard). If a draft with an override
    // reaches here under a DIFFERENT standard, the caller dispatched wrong:
    // fail-closed rather than emit crypto under a standard the user did not
    // pick.
    if let Some(override_standard) = draft
        .standard_override
        .as_deref()
        .map(CryptoMethod::from_db_str)
        .and_then(CryptoMethod::standard)
    {
        if override_standard != standard {
            return Err(CryptoSendError::Backend(CryptoError::UnsupportedStandard(
                format!(
                    "draft standard_override '{}' does not match the dispatch standard \
                     '{standard}' (caller bug: the caller must dispatch on the override)",
                    override_standard.as_str()
                ),
            )));
        }
    }

    let (mut outer, mut entity) = split_message(mime)?;

    // Resolve the signing key up-front. Used by BOTH the encrypt-with-sign
    // arm (opaque SignedData inside EnvelopedData for S/MIME; inline-signed
    // ciphertext for PGP) and the sign-only detached-sign arm below. `None`
    // when not signing.
    //
    // CRITICAL: when BOTH sign + encrypt we must NOT clear-sign-then-encrypt.
    // Clear-signing wraps the body in a `multipart/signed` MIME entity; the
    // receive orchestrator only recurses into signature verification for an
    // opaque CMS `SignedData`, NOT for a `multipart/signed` MIME structure,
    // so a clear-signed-then-encrypted message would be received as
    // encrypted-but-unsigned (the signature silently dropped). Passing
    // `sign_with` to `backend.encrypt` instead composes the signed-inside
    // shape the receiver recognizes.
    let signer: Option<KeyHandleRef> = if draft.sign {
        let signer_row = default_signing_key.ok_or(CryptoSendError::NoSigningKey)?;
        Some(key_handle_ref(signer_row))
    } else {
        None
    };

    // --- encrypt ---
    if draft.encrypt {
        // Recipient set = sender (encrypt-to-self) + To/Cc/Bcc. Dedup is
        // the keystore's job (duplicate handles are harmless to the
        // backend).
        let recipient_emails: Vec<String> = std::iter::once(account_email.to_string())
            .chain(draft.to.iter().map(|a| a.email.clone()))
            .chain(draft.cc.iter().map(|a| a.email.clone()))
            .chain(draft.bcc.iter().map(|a| a.email.clone()))
            .collect();
        // The trust gate (fail-closed): EVERY recipient must resolve to
        // `Ready` (key present + Verified/Personal acceptance, OR
        // `unverified` acceptance which the Thunderbird model treats as
        // encryption-qualified and the resolver maps to `Ready`). Any
        // non-Ready recipient aborts the send with a typed, per-recipient
        // payload BEFORE any ciphertext is produced. No plaintext
        // downgrade, no auto-encrypt-to-unaccepted-key.
        let trust_failures = policy
            .recipient_trust_failures(standard, &recipient_emails)
            .await;
        if !trust_failures.is_empty() {
            return Err(CryptoSendError::RecipientTrust(trust_failures));
        }
        let mut recipients = Vec::with_capacity(recipient_emails.len());
        for email in &recipient_emails {
            let handles = keystore
                .find_by_email(standard, email)
                .await
                .map_err(CryptoSendError::Backend)?;
            // Pin-aware selection: encrypt to the SAME handle the trust
            // gate just evaluated; both sides share
            // [`SendCryptoPolicy::pin_preferred_handle`], so a gate that
            // passes on the pinned key can never produce ciphertext for a
            // different (rejected/expired) first-sorted candidate.
            let h = policy
                .pin_preferred_handle(standard, email, &handles)
                .await
                .ok_or_else(|| {
                    CryptoSendError::MissingRecipientCert(missing_recipient_msg(standard, email))
                })?;
            recipients.push(h);
        }
        // Fail-closed: S/MIME-only cert-chain validation. An expired /
        // wrong-issuer / wrong-EKU recipient cert would otherwise produce
        // ciphertext that the recipient can never decrypt. PGP does NOT run
        // this gate (Web-of-Trust, deferred; and `find_by_email` already
        // guarantees the recipient key is present, sufficient for PGP
        // encrypt).
        if standard == Standard::Smime {
            let mut recipient_cert_ders = Vec::with_capacity(recipients.len());
            for h in &recipients {
                let der = backend
                    .export_public(&h.handle)
                    .await
                    .map_err(CryptoSendError::Backend)?;
                recipient_cert_ders.push(der);
            }
            let trust_anchor_ders = policy
                .trust_anchor_ders()
                .await
                .map_err(CryptoSendError::InvalidRecipientCert)?;
            let now_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            policy
                .validate_recipient_certs(&recipient_cert_ders, &trust_anchor_ders, now_unix)
                .await
                .map_err(CryptoSendError::InvalidRecipientCert)?;
        }
        // Protected Headers (encrypted Subject), gated on the account's
        // `protect_subject` crypto-policy flag (default ON, mirrors
        // Thunderbird's `mail.identity.default.protectSubject`).
        // Encrypt-only gate: sign-only messages keep the cleartext subject.
        // When ON:
        //   1. the outer `Subject:` is replaced by TB's placeholder (`...`),
        //      preserving In-Reply-To/References in the clear for threading;
        //   2. the body entity is wrapped in `multipart/mixed;
        //      protected-headers="v1"` carrying the real Subject in its
        //      header block.
        // A policy read ERROR defaults to ON (fail-safe: never leak the
        // subject because of a store hiccup).
        //
        // Runs BEFORE the Autocrypt-Gossip injection below so gossip
        // headers land in the wrapper's root header block (TB's layout),
        // which is also where `consume_gossip_headers` reads them on
        // receive.
        match policy.protect_subject().await {
            Ok(false) => {}
            Ok(true) => {
                let (new_outer, subject_block) = hide_subject_header(&outer.headers);
                outer.headers = new_outer;
                entity.0 = wrap_protected_headers(&entity.0, subject_block.as_deref(), draft.sign);
            }
            Err(e) => {
                log::warn!("[crypto] protect_subject read failed ({e}); defaulting to ON");
                let (new_outer, subject_block) = hide_subject_header(&outer.headers);
                outer.headers = new_outer;
                entity.0 = wrap_protected_headers(&entity.0, subject_block.as_deref(), draft.sign);
            }
        }
        // Autocrypt-Gossip emission (OpenPGP only, INSIDE the encrypted
        // part, never in cleartext headers). TB's gating: encrypted AND
        // signed AND the identity allows Autocrypt AND >=2 non-sender
        // recipients AND every recipient has a known key; the trust gate
        // above already guarantees the last condition. The headers are
        // prepended to the inner entity's header block BEFORE
        // `backend.encrypt`, so they ride inside the ciphertext as
        // protected headers.
        if standard == Standard::OpenPgp && draft.sign {
            entity.0 = policy
                .maybe_prepend_gossip_headers(
                    backend,
                    &entity.0,
                    account_email,
                    &recipient_emails,
                    &recipients,
                )
                .await;
        }
        let env = backend
            .encrypt(EncryptOp {
                parts: &[Part {
                    id: PartId("body".into()),
                    kind: PartKind::Body,
                    data: entity.0.clone(),
                }],
                serialization: SerializationStrategy::SingleMimeBlob,
                recipients: &recipients,
                // `sign_with` = the resolved signer when `draft.sign`:
                // opaque SignedData inside EnvelopedData (S/MIME
                // sign-then-encrypt) or inline-signed ciphertext (PGP).
                // `None` -> encrypt-only.
                sign_with: signer.clone(),
            })
            .await?;
        let envelope_bytes = env
            .parts
            .first()
            .expect("encrypt returns exactly one part")
            .ciphertext
            .clone();
        // Standard-branched MIME wrapping of the encrypted blob.
        let wrapped_entity = match standard {
            Standard::Smime => wrap_enveloped(&envelope_bytes, "enveloped-data"),
            Standard::OpenPgp => pgp_mime::wrap_encrypted(&envelope_bytes),
            Standard::Sm => {
                return Err(CryptoSendError::Backend(CryptoError::UnsupportedStandard(
                    "sm not implemented".into(),
                )))
            }
        };
        // Final message = outer headers (no entity Content-Type) + the
        // wrapped entity (which carries its own Content-Type).
        let mut out = outer.headers.into_bytes();
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&wrapped_entity);
        return Ok(out);
    }

    // --- sign-only (detached sign) ---
    // Reached only when `draft.sign && !draft.encrypt` (the encrypt branch
    // above returns). Produces a `multipart/signed` MIME entity.
    if let Some(signer) = signer {
        match standard {
            Standard::Smime => {
                // S/MIME: sign the WHOLE part-1 entity (Content-Type +
                // blank + body, with exactly one trailing CRLF) and wrap
                // via `wrap_multipart_signed`: the byte-exact S/MIME
                // clear-sign shape (Thunderbird-interoperable).
                let part1 = ensure_one_trailing_crlf(&entity.0);
                let signed = backend
                    .sign(SignOp {
                        payload: &part1,
                        signing_key: signer,
                        detached: true,
                    })
                    .await?;
                entity.0 = wrap_multipart_signed(&part1, &signed.signature.signature);
            }
            Standard::OpenPgp => {
                // PGP: `pgp_mime::wrap_signed` takes `(payload_body,
                // payload_mime, detached_sig, hash)` SEPARATELY (vs
                // S/MIME's whole-entity slice) because it reconstructs the
                // part-1 entity internally. The detached signature MUST be
                // computed over the EXACT part-1 entity `wrap_signed`
                // emits, so we split the entity into its Content-Type
                // value + body, canonicalize the body, rebuild the part-1
                // entity bytes, sign THAT, then hand the body + mime + sig
                // to `wrap_signed` (which re-canonicalizes idempotently
                // and reconstructs the same part-1).
                let (content_type_value, body_bytes) = split_entity_for_pgp_signed(&entity.0)?;
                let body_canonical = ensure_one_trailing_crlf(&body_bytes);
                let part1_entity = build_pgp_signed_region(&content_type_value, &body_canonical);
                let signed = backend
                    .sign(SignOp {
                        payload: &part1_entity,
                        signing_key: signer,
                        detached: true,
                    })
                    .await?;
                // micalg scope: Ed25519-only (RFC 8032 -> SHA-512). See
                // `pgp_mime::micalg_name`'s "Current scope" doc: imported
                // non-Ed25519 signers are out of scope until the framework
                // surfaces the signature's hash_algo.
                entity.0 = pgp_mime::wrap_signed(
                    &body_canonical,
                    &content_type_value,
                    &signed.signature.signature,
                    HashAlgorithm::Sha512,
                );
            }
            Standard::Sm => {
                return Err(CryptoSendError::Backend(CryptoError::UnsupportedStandard(
                    "sm not implemented".into(),
                )))
            }
        }
    }

    // sign-only: outer headers + the multipart/signed entity (its own
    // Content-Type).
    let mut out = outer.headers.into_bytes();
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(&entity.0);
    Ok(out)
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod apply_tests;
