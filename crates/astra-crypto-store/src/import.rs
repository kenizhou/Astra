// SPDX-License-Identifier: GPL-3.0-only
//! The public-cert import path (SP4d-1): the store write a user-facing key
//! import lands through. Public material only, by construction: every row
//! goes through [`upsert_crypto_key`] with `private_data: None`, never
//! through the backend `import_key` / keystore `put` path (which records a
//! private blob and would pollute the receive decryption-candidate filter
//! and the resolver's Personal/Ready branch).
//!
//! Two standards, one entry: [`import_public_cert`] sniffs the material
//! (OpenPGP armor-or-binary first, then an S/MIME PEM or DER cert) and
//! routes; [`import_public_cert_as`] pins the standard and refuses material
//! of the other one. Secret-bearing OpenPGP input is refused here: a
//! private key belongs to the backend import path, which wraps the secret
//! at rest under the account-bound AAD.

use base64::Engine as _;

use crate::keys::{get_crypto_key_public, upsert_crypto_key, CryptoKeyRecord, CryptoKeyRow};
use crate::store::CryptoStore;
use crate::trust::put_trust_decision;
use astra_crypto_openpgp::keymap::{cert_to_public_blob, fingerprint_of, parse_certs};

/// Import a peer's public cert into `account_id`, sniffing the standard:
/// OpenPGP (armored or binary, one or more certs) or S/MIME (a PEM
/// CERTIFICATE block or raw DER). Stores the public-only material through
/// [`upsert_crypto_key`] with `private_data: None` and returns the first
/// imported row. A re-import of the same fingerprint updates the one row.
///
/// # Errors
/// Refuses malformed input (nothing is staged), secret-bearing OpenPGP
/// material (the private import path owns it), and any parse or store
/// failure. Never panics on hostile bytes.
pub fn import_public_cert(
    store: &CryptoStore,
    account_id: &str,
    bytes: &[u8],
) -> Result<CryptoKeyRow, String> {
    if let Ok(certs) = parse_certs(bytes) {
        if !certs.is_empty() {
            return import_openpgp_public(store, account_id, &certs);
        }
    }
    let der = smime_cert_der(bytes)?;
    import_smime_public(store, account_id, &der)
}

/// Like [`import_public_cert`] but the standard is pinned: `openpgp` or
/// `smime`. Material of the other standard is refused, so a caller that
/// already knows the standard never lands a cross-standard row.
///
/// # Errors
/// Refuses an unknown standard, material that does not parse as the pinned
/// standard, and the same failures [`import_public_cert`] names.
pub fn import_public_cert_as(
    store: &CryptoStore,
    account_id: &str,
    standard: &str,
    bytes: &[u8],
) -> Result<CryptoKeyRow, String> {
    match standard {
        "openpgp" => {
            let certs = parse_certs(bytes).map_err(|e| e.to_string())?;
            if certs.is_empty() {
                return Err("no OpenPGP cert in the input".to_owned());
            }
            import_openpgp_public(store, account_id, &certs)
        }
        "smime" => {
            let der = smime_cert_der(bytes)?;
            import_smime_public(store, account_id, &der)
        }
        other => Err(format!("unsupported key standard: {other}")),
    }
}

/// Record the import surface's trust decision for a peer key: `verified`
/// when the person confirmed the fingerprint out of band, `unverified`
/// when they accepted it without verification. The append-only audit keeps
/// every decision; the latest one is what the send resolver reads. Shared
/// with the collected-key promote surface (SP4d-2).
///
/// # Errors
/// Fails when the decision insert cannot run.
pub fn record_import_trust(
    store: &CryptoStore,
    account_id: &str,
    peer_email: &str,
    standard: &str,
    fingerprint: &str,
    verified: bool,
) -> Result<(), String> {
    let decision = if verified { "verified" } else { "unverified" };
    put_trust_decision(
        store,
        account_id,
        peer_email,
        standard,
        fingerprint,
        decision,
        None,
    )
}

/// Store every parsed OpenPGP cert as a public-only row and answer the
/// first one. Secret material anywhere in the input refuses the whole
/// import: the public path never wraps a private blob.
fn import_openpgp_public(
    store: &CryptoStore,
    account_id: &str,
    certs: &[sequoia_openpgp::Cert],
) -> Result<CryptoKeyRow, String> {
    if certs.iter().any(sequoia_openpgp::Cert::is_tsk) {
        return Err(
            "the input carries secret key material; use the private key import path".to_owned(),
        );
    }
    let mut first_fingerprint = String::new();
    for cert in certs {
        let fingerprint = fingerprint_of(cert).as_str().to_owned();
        let blob = cert_to_public_blob(cert).map_err(|e| e.to_string())?;
        let record = CryptoKeyRecord {
            row: CryptoKeyRow {
                account_id: account_id.to_owned(),
                standard: "openpgp".to_owned(),
                key_type: "public".to_owned(),
                email: email_of_cert(cert),
                fingerprint: fingerprint.clone(),
                origin: "imported".to_owned(),
                ..Default::default()
            },
            public_data: hex::encode(blob),
            private_data: None,
            policy_json: None,
        };
        upsert_crypto_key(store, &record)?;
        if first_fingerprint.is_empty() {
            first_fingerprint = fingerprint;
        }
    }
    get_crypto_key_public(store, account_id, "openpgp", &first_fingerprint)?
        .ok_or_else(|| "the imported row did not land".to_owned())
}

/// Store one S/MIME cert DER as a public-only row, filling the display
/// metadata (subject CN, validity window, SAN email) from the cert.
fn import_smime_public(
    store: &CryptoStore,
    account_id: &str,
    der: &[u8],
) -> Result<CryptoKeyRow, String> {
    let fingerprint =
        astra_crypto_smime::fingerprint_of_cert_der(der).map_err(|e| e.to_string())?;
    let summary = astra_crypto_smime::cert_summary_of_der(der).ok();
    let record = CryptoKeyRecord {
        row: CryptoKeyRow {
            account_id: account_id.to_owned(),
            standard: "smime".to_owned(),
            key_type: "cert".to_owned(),
            email: summary.as_ref().and_then(|s| s.email.clone()),
            fingerprint: fingerprint.clone(),
            origin: "imported".to_owned(),
            expires_at: summary.as_ref().map(|s| s.not_after_unix.to_string()),
            subject_cn: summary.as_ref().and_then(|s| s.subject_cn.clone()),
            not_before: summary.as_ref().map(|s| s.not_before_unix.to_string()),
            ..Default::default()
        },
        public_data: hex::encode(der),
        private_data: None,
        policy_json: None,
    };
    upsert_crypto_key(store, &record)?;
    get_crypto_key_public(store, account_id, "smime", &fingerprint)?
        .ok_or_else(|| "the imported row did not land".to_owned())
}

/// The PEM frame markers of an X.509 CERTIFICATE block.
const PEM_BEGIN: &str = "-----BEGIN CERTIFICATE-----";
const PEM_END: &str = "-----END CERTIFICATE-----";

/// The DER bytes of an S/MIME cert input: a PEM CERTIFICATE block decoded,
/// or the raw bytes when they are not PEM. The caller validates the DER.
fn smime_cert_der(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(bytes.to_vec());
    };
    if !text.trim_start().starts_with("-----BEGIN") {
        return Ok(bytes.to_vec());
    }
    let start = text
        .find(PEM_BEGIN)
        .ok_or_else(|| "no CERTIFICATE block in the PEM input".to_owned())?;
    let body = &text[start + PEM_BEGIN.len()..];
    let end = body
        .find(PEM_END)
        .ok_or_else(|| "unterminated CERTIFICATE block".to_owned())?;
    let b64: String = body[..end].chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("the CERTIFICATE block is not valid base64: {e}"))
}

/// The email of a cert's first user id carrying an `<addr>` form, when
/// present (the receive path's own extraction pattern).
fn email_of_cert(cert: &sequoia_openpgp::Cert) -> Option<String> {
    for ua in cert.userids() {
        let uid = ua.userid().to_string();
        if let Some(start) = uid.find('<') {
            if let Some(end) = uid[start + 1..].find('>') {
                let email = uid[start + 1..start + 1 + end].trim();
                if !email.is_empty() {
                    return Some(email.to_owned());
                }
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod import_tests;
