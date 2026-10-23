// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the MIME-wrapping layer: byte shapes the send gate emits.

use super::*;

#[test]
fn split_message_routes_entity_headers_and_body() {
    let msg = b"From: a@x.com\r\nSubject: hi\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\nhello body";
    let (outer, entity) = split_message(msg).expect("split");
    assert_eq!(outer.headers, "From: a@x.com\r\nSubject: hi");
    assert_eq!(
        entity.0,
        b"Content-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\nhello body"
            .to_vec()
    );
}

#[test]
fn split_message_keeps_a_folded_content_type_continuation_in_the_entity() {
    // mail-builder folds long Content-Type values onto a continuation line;
    // the continuation must inherit the entity bucket, not leak outer.
    let msg =
        b"Subject: hi\r\nContent-Type: multipart/alternative;\r\n\tboundary=\"abc\"\r\n\r\nbody";
    let (outer, entity) = split_message(msg).expect("split");
    assert_eq!(outer.headers, "Subject: hi");
    assert_eq!(
        entity.0,
        b"Content-Type: multipart/alternative;\r\n\tboundary=\"abc\"\r\n\r\nbody".to_vec()
    );
}

#[test]
fn split_message_rejects_garbage() {
    assert!(split_message(b"no blank line here").is_err());
    assert!(split_message(&[0xff, 0xfe, 0x00]).is_err());
}

#[test]
fn ensure_one_trailing_crlf_collapses_and_appends() {
    assert_eq!(ensure_one_trailing_crlf(b"abc"), b"abc\r\n".to_vec());
    assert_eq!(ensure_one_trailing_crlf(b"abc\r\n"), b"abc\r\n".to_vec());
    assert_eq!(
        ensure_one_trailing_crlf(b"abc\r\n\r\n\r\n"),
        b"abc\r\n".to_vec(),
        "doubled trailing CRLFs collapse to exactly one"
    );
}

#[test]
fn wrap_multipart_signed_is_the_thunderbird_shape() {
    let part1 = b"Content-Type: text/plain\r\n\r\nbody\r\n";
    let out = wrap_multipart_signed(part1, b"\x01\x02\x03");
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with(
        "Content-Type: multipart/signed; protocol=\"application/pkcs7-signature\"; \
         micalg=sha-256; boundary=\"----=_astra_smime_signed_0001\"\r\n\r\n\
         This is a cryptographically signed message in MIME format.\r\n"
    ));
    // Part 1 is the exact signed slice, framed by the `--` + boundary
    // delimiter lines.
    assert!(
        text.contains(
            "------=_astra_smime_signed_0001\r\n\
         Content-Type: text/plain\r\n\r\nbody\r\n\
         ------=_astra_smime_signed_0001\r\n"
        ),
        "part 1 is byte-exact what the caller signed: {text}"
    );
    assert!(text.contains("Content-Description: S/MIME Cryptographic Signature\r\n\r\n"));
    assert!(text.contains("AQID\r\n"), "the signature is base64-wrapped");
    assert!(text.ends_with("------=_astra_smime_signed_0001--\r\n"));
}

#[test]
fn wrap_enveloped_is_the_pkcs7_mime_shape() {
    let out = wrap_enveloped(b"\x30\x82", "enveloped-data");
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with(
        "Content-Type: application/pkcs7-mime; smime-type=enveloped-data; name=\"smime.p7m\"\r\n\
         Content-Transfer-Encoding: base64\r\n\
         Content-Disposition: attachment; filename=\"smime.p7m\"\r\n\r\n"
    ));
    assert!(text.ends_with("MII=\r\n"));
}

#[test]
fn hide_subject_header_substitutes_the_placeholder_and_captures_the_real_subject() {
    let outer = "From: a@x.com\r\nSubject: secret plans\r\nMessage-ID: <m@x>";
    let (new_outer, block) = hide_subject_header(outer);
    assert_eq!(
        new_outer, "From: a@x.com\r\nSubject: ...\r\nMessage-ID: <m@x>",
        "the placeholder takes the Subject's position, threading stays clear"
    );
    assert_eq!(block.as_deref(), Some("Subject: secret plans"));
}

#[test]
fn hide_subject_header_captures_folded_continuations() {
    let outer = "Subject: a very long\r\n\tfolded subject\r\nFrom: a@x.com";
    let (new_outer, block) = hide_subject_header(outer);
    assert_eq!(new_outer, "Subject: ...\r\nFrom: a@x.com");
    assert_eq!(
        block.as_deref(),
        Some("Subject: a very long\r\n\tfolded subject"),
        "the folded real subject rides into the protected block"
    );
}

#[test]
fn hide_subject_header_treats_an_empty_subject_as_no_subject() {
    let outer = "Subject:\r\nFrom: a@x.com";
    let (new_outer, block) = hide_subject_header(outer);
    assert!(new_outer.contains("Subject: ..."));
    assert_eq!(block, None, "nothing real to protect inside");
}

#[test]
fn hide_subject_header_prepends_the_placeholder_when_no_subject_exists() {
    let outer = "From: a@x.com\r\nMessage-ID: <m@x>";
    let (new_outer, block) = hide_subject_header(outer);
    assert_eq!(
        new_outer,
        "Subject: ...\r\nFrom: a@x.com\r\nMessage-ID: <m@x>"
    );
    assert_eq!(block, None);
}

#[test]
fn wrap_protected_headers_carries_the_real_subject_inside() {
    let entity = b"Content-Type: text/plain\r\n\r\nbody";
    let out = wrap_protected_headers(entity, Some("Subject: secret plans"), true);
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.starts_with(
            "Content-Type: multipart/mixed; boundary=\"----=_astra_protected_headers_0001\";\r\n \
         protected-headers=\"v1\"; hp=\"cipher\"\r\n"
        ),
        "hp=\"cipher\" rides when signed: {text}"
    );
    assert!(text.contains("\r\nSubject: secret plans\r\n\r\n"));
    // The original entity is the sole subpart with exactly one trailing CRLF.
    assert!(text.contains(
        "------=_astra_protected_headers_0001\r\nContent-Type: text/plain\r\n\r\nbody\r\n------=_astra_protected_headers_0001--\r\n"
    ));
}

#[test]
fn wrap_protected_headers_without_subject_and_unsigned() {
    let entity = b"Content-Type: text/plain\r\n\r\nbody\r\n";
    let out = wrap_protected_headers(entity, None, false);
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with(
        "Content-Type: multipart/mixed; boundary=\"----=_astra_protected_headers_0001\";\r\n \
         protected-headers=\"v1\"\r\n"
    ));
    assert!(!text.contains("hp="));
    assert!(!text.contains("Subject:"));
}
