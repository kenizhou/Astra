// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the PGP/MIME wrappers (RFC 3156 shapes + micalg spellings).

use super::*;

#[test]
fn micalg_names_match_the_rfc_spellings() {
    assert_eq!(micalg_name(HashAlgorithm::Sha256), "pgp-sha256");
    assert_eq!(micalg_name(HashAlgorithm::Sha384), "pgp-sha384");
    assert_eq!(micalg_name(HashAlgorithm::Sha512), "pgp-sha512");
    assert_eq!(micalg_name(HashAlgorithm::Sha3_256), "pgp-sha3-256");
    assert_eq!(micalg_name(HashAlgorithm::Sm3), "pgp-sm3");
}

#[test]
fn wrap_encrypted_is_the_rfc3156_2_shape() {
    let out = wrap_encrypted(b"\xc1\x01\x02");
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with(
        "Content-Type: multipart/encrypted; \
         protocol=\"application/pgp-encrypted\"; \
         boundary=\"----=_astra_pgp_encrypted_0001\"\r\n\r\n\
         This is an OpenPGP/MIME encrypted message (RFC 3156)\r\n"
    ));
    assert!(text.contains(
        "----=_astra_pgp_encrypted_0001\r\nContent-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n"
    ));
    assert!(text.contains(
        "Content-Type: application/octet-stream; name=\"encrypted.asc\"\r\n\
         Content-Transfer-Encoding: base64\r\n\
         Content-Disposition: inline; filename=\"encrypted.asc\"\r\n\r\n"
    ));
    assert!(text.contains("wQEC\r\n"), "the message is base64-wrapped");
    assert!(text.ends_with("------=_astra_pgp_encrypted_0001--\r\n"));
}

#[test]
fn wrap_signed_is_the_rfc3156_1_shape_with_exact_part1() {
    let out = wrap_signed(
        b"body text",
        "text/plain; charset=utf-8",
        b"\x89\x02",
        HashAlgorithm::Sha512,
    );
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with(
        "Content-Type: multipart/signed; \
         protocol=\"application/pgp-signature\"; \
         micalg=pgp-sha512; \
         boundary=\"----=_astra_pgp_signed_0001\"\r\n\r\n\
         This is an OpenPGP/MIME signed message (RFC 3156)\r\n"
    ));
    // Part 1 = Content-Type header + blank + canonicalized body (exactly one
    // trailing CRLF, the region the signature must cover), framed by the
    // `--` + boundary delimiter lines.
    assert!(text.contains(
        "------=_astra_pgp_signed_0001\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nbody text\r\n------=_astra_pgp_signed_0001\r\n"
    ));
    assert!(text.contains("Content-Type: application/pgp-signature; name=\"signature.asc\""));
    assert!(text.ends_with("------=_astra_pgp_signed_0001--\r\n"));
}

#[test]
fn wrap_signed_canonicalization_is_idempotent() {
    let a = wrap_signed(b"body", "text/plain", b"s", HashAlgorithm::Sha512);
    let b = wrap_signed(b"body\r\n", "text/plain", b"s", HashAlgorithm::Sha512);
    let c = wrap_signed(b"body\r\n\r\n", "text/plain", b"s", HashAlgorithm::Sha512);
    assert_eq!(a, b);
    assert_eq!(b, c);
}
