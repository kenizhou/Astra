// SPDX-License-Identifier: GPL-3.0-only
//! The S/MIME enveloped framing proofs: the unwrap round-trip (both the
//! `application/pkcs7-mime` and the legacy `application/x-pkcs7-mime` media
//! types), the recognised-but-unextractable fail-closed distinction, and the
//! `From`-extraction helper the receive orchestrator resolves the signer
//! identity through.

use super::*;
use crate::mime;
use base64::Engine;

/// A stand-in EnvelopedData DER blob (the framing layer is byte-agnostic; the
/// CMS parse happens in the smime crate, not here).
const CMS_DER: &[u8] = b"\x30\x80\x06\x09binary enveloped data\xff";

#[test]
fn unwrap_enveloped_round_trips_the_cms_der() {
    let wrapped = mime::wrap_enveloped(CMS_DER, "enveloped-data");
    assert_eq!(
        unwrap_enveloped(&wrapped),
        Some(CMS_DER.to_vec()),
        "unwrap recovers the exact EnvelopedData DER bytes"
    );
}

#[test]
fn is_smime_enveloped_recognises_the_pkcs7_media_type() {
    let wrapped = mime::wrap_enveloped(CMS_DER, "enveloped-data");
    assert!(is_smime_enveloped(&wrapped));
}

#[test]
fn unwrap_enveloped_accepts_the_legacy_x_pkcs7_media_type() {
    // The legacy `application/x-pkcs7-mime` spelling some older clients emit.
    let b64 = base64::engine::general_purpose::STANDARD.encode(CMS_DER);
    let mut entity = b"Content-Type: application/x-pkcs7-mime; smime-type=enveloped-data\r\n\
                       Content-Transfer-Encoding: base64\r\n\r\n"
        .to_vec();
    entity.extend_from_slice(b64.as_bytes());
    assert!(is_smime_enveloped(&entity));
    assert_eq!(unwrap_enveloped(&entity), Some(CMS_DER.to_vec()));
}

#[test]
fn unwrap_enveloped_answers_none_for_other_framing() {
    assert_eq!(
        unwrap_enveloped(b"Content-Type: text/plain\r\n\r\nhello"),
        None
    );
    // A signed-data smime-type is not the enveloped framing.
    let signed = mime::wrap_enveloped(CMS_DER, "signed-data");
    assert!(!is_smime_enveloped(&signed));
    assert_eq!(unwrap_enveloped(&signed), None);
    // PGP/MIME is a different crypto framing entirely.
    let pgp = crate::pgp_mime::wrap_encrypted(b"packets");
    assert!(!is_smime_enveloped(&pgp));
    assert_eq!(unwrap_enveloped(&pgp), None);
}

#[test]
fn recognised_but_unextractable_framing_is_distinguished() {
    // The media type + smime-type are recognised, but the body is not valid
    // base64, so nothing extracts. The orchestrator fails closed on this arm
    // (is_smime_enveloped true, unwrap None) rather than rendering untouched.
    let entity = b"Content-Type: application/pkcs7-mime; smime-type=enveloped-data\r\n\
                   Content-Transfer-Encoding: base64\r\n\r\n!!!not base64!!!";
    assert!(
        is_smime_enveloped(entity),
        "the framing is recognised even when the body will not decode"
    );
    assert_eq!(
        unwrap_enveloped(entity),
        None,
        "an unextractable body answers None (the fail-closed distinction)"
    );
}

#[test]
fn unwrap_enveloped_answers_none_when_the_body_is_absent() {
    let entity = b"Content-Type: application/pkcs7-mime; smime-type=enveloped-data\r\n\r\n";
    assert!(is_smime_enveloped(entity));
    assert_eq!(unwrap_enveloped(entity), None);
}

// --- The From-extraction helper (the From↔SAN binding input) ---

#[test]
fn from_email_extracts_the_angle_bracket_address() {
    let source = b"From: Alice <a1@x.com>\r\nTo: a2@x.com\r\nSubject: hi\r\n\r\nbody";
    assert_eq!(mime::from_email(source).as_deref(), Some("a1@x.com"));
}

#[test]
fn from_email_extracts_the_bare_address() {
    let source = b"From: a1@x.com\r\nSubject: hi\r\n\r\nbody";
    assert_eq!(mime::from_email(source).as_deref(), Some("a1@x.com"));
}

#[test]
fn from_email_is_none_when_the_header_is_absent() {
    let source = b"To: a2@x.com\r\nSubject: hi\r\n\r\nbody";
    assert_eq!(mime::from_email(source), None);
}

#[test]
fn from_email_unfolds_a_continuation_line() {
    let source = b"From: Alice\r\n <a1@x.com>\r\nSubject: hi\r\n\r\nbody";
    assert_eq!(mime::from_email(source).as_deref(), Some("a1@x.com"));
}
