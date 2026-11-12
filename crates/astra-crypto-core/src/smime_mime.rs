// SPDX-License-Identifier: GPL-3.0-only
//! S/MIME enveloped framing (RFC 8551): the receive-side mirror of
//! [`crate::mime::wrap_enveloped`]. Detects and unwraps
//! `application/pkcs7-mime` (and the legacy `application/x-pkcs7-mime`) with
//! `smime-type=enveloped-data`, base64-decoding the body to the EnvelopedData
//! DER. Mirrors [`crate::pgp_mime`]'s `is_pgp_encrypted` / `unwrap_encrypted`
//! split: a recognised-but-unextractable framing fails closed (the orchestrator
//! distinguishes the two answers) rather than rendering untouched.
//!
//! The CMS DER parse lives in astra-crypto-smime, never here: this module only
//! transports bytes.

use crate::mime::{
    content_type, ct_param, decode_cte, header_value, media_type, split_headers_body,
};

/// The two media types that carry an S/MIME enveloped body. The `x-` spelling
/// is the legacy form some older clients emit; both are recognised.
const ENVELOPED_MEDIA_TYPES: &[&str] = &["application/pkcs7-mime", "application/x-pkcs7-mime"];

/// Whether the entity is recognised as S/MIME enveloped (the Content-Type and
/// `smime-type=enveloped-data`), regardless of whether the ciphertext extracts.
/// A recognised but unextractable message fails closed rather than rendering
/// untouched (the [`crate::pgp_mime::is_pgp_encrypted`] precedent).
pub fn is_smime_enveloped(entity: &[u8]) -> bool {
    let Some((headers, _body)) = split_headers_body(entity) else {
        return false;
    };
    let ct = content_type(&headers);
    let mt = media_type(&ct);
    ENVELOPED_MEDIA_TYPES.contains(&mt.as_str())
        && ct_param(&ct, "smime-type")
            .map(|t| t.eq_ignore_ascii_case("enveloped-data"))
            .unwrap_or(false)
}

/// The inverse of [`crate::mime::wrap_enveloped`]: extract the EnvelopedData DER
/// bytes from an `application/pkcs7-mime; smime-type=enveloped-data` entity
/// (RFC 8551). `None` when the entity is not this crypto framing OR the body
/// will not base64-decode. The orchestrator pairs this with
/// [`is_smime_enveloped`] to fail closed on a recognised-but-unextractable body.
pub fn unwrap_enveloped(entity: &[u8]) -> Option<Vec<u8>> {
    let (headers, body) = split_headers_body(entity)?;
    let ct = content_type(&headers);
    let mt = media_type(&ct);
    if !ENVELOPED_MEDIA_TYPES.contains(&mt.as_str()) {
        return None;
    }
    if !ct_param(&ct, "smime-type")?.eq_ignore_ascii_case("enveloped-data") {
        return None;
    }
    let cte =
        header_value(&headers, "content-transfer-encoding").unwrap_or_else(|| "base64".to_string());
    let der = decode_cte(&body, &cte)?;
    // An empty body carries no EnvelopedData: unextractable, not a zero-byte
    // ciphertext (the fail-closed distinction).
    if der.is_empty() {
        None
    } else {
        Some(der)
    }
}

#[cfg(test)]
#[path = "smime_mime_tests.rs"]
mod smime_mime_tests;
