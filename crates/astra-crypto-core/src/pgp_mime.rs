// SPDX-License-Identifier: GPL-3.0-only
//! PGP/MIME wrapping (RFC 3156): kylins' `mail/crypto/pgp_mime/wrap.rs`
//! ported. `wrap_encrypted` frames an OpenPGP message as
//! `multipart/encrypted; protocol="application/pgp-encrypted"` (§2);
//! `wrap_signed` frames a detached signature as `multipart/signed;
//! protocol="application/pgp-signature"` (§1). Binary OpenPGP packets are
//! base64-transported, mirroring the S/MIME path's handling of binary DER
//! (armor would double-encode; base64 is the MIME-native transport).
//!
//! Deviation from kylins: the boundary constants are renamed
//! `----=_kylins_…` -> `----=_astra_…` (opaque to verifiers by RFC 2046).

use base64::Engine;

use crate::mime::ensure_one_trailing_crlf;
use crate::policy::HashAlgorithm;

/// The `multipart/encrypted` boundary.
pub const ENCRYPTED_BOUNDARY: &str = "----=_astra_pgp_encrypted_0001";

/// The `multipart/signed` boundary (PGP/MIME).
pub const SIGNED_BOUNDARY: &str = "----=_astra_pgp_signed_0001";

/// Map a framework [`HashAlgorithm`] to its RFC 3156 `micalg` symbol (the
/// `pgp-<name>` value the `multipart/signed` `micalg` parameter carries).
///
/// Per RFC 4880 §9.4 + RFC 9580 hash algorithm table:
/// - SHA-256 -> `pgp-sha256`, SHA-384 -> `pgp-sha384`, SHA-512 -> `pgp-sha512`
/// - SHA3-256 -> `pgp-sha3-256` (RFC 9580)
/// - SM3 -> `pgp-sm3` (GnuPG extension; hash ID 11: no RFC-standard micalg
///   string exists, but `pgp-sm3` is the de-facto GnuPG spelling).
///
/// The framework [`HashAlgorithm`] does NOT include SHA-1 (banned for
/// signatures under the default baseline policy), so this match is total
/// over the framework's hash set.
///
/// # Caller responsibility: `micalg` MUST match the signature's actual hash
///
/// The `micalg` parameter is **caller-supplied** (via [`wrap_signed`]'s
/// `hash` argument). RFC 3156 §1 requires `micalg` to name the hash the
/// detached signature was actually computed with; a mismatch causes strict
/// PGP/MIME verifiers (Thunderbird, some Enigmail configs) to reject the
/// signature as malformed. This helper does NOT validate that the caller's
/// `hash` matches the signature packet: it only spells the symbol.
///
/// ## Current scope: Ed25519 signers (SHA-512)
///
/// The send path passes [`HashAlgorithm::Sha512`] unconditionally because
/// the engine's `generate` only produces Ed25519 signing keys, and Sequoia
/// picks SHA-512 as Ed25519's hash per RFC 8032. **Imported non-Ed25519
/// signing keys are out of scope** until the framework's `SignedEnvelope`
/// surfaces the signature's `hash_algo`.
pub fn micalg_name(hash: HashAlgorithm) -> &'static str {
    match hash {
        HashAlgorithm::Sha256 => "pgp-sha256",
        HashAlgorithm::Sha384 => "pgp-sha384",
        HashAlgorithm::Sha512 => "pgp-sha512",
        HashAlgorithm::Sha3_256 => "pgp-sha3-256",
        HashAlgorithm::Sm3 => "pgp-sm3",
    }
}

/// Build a `multipart/encrypted` MIME **entity** (Content-Type header +
/// blank + multipart body) framing an OpenPGP message per RFC 3156 §2.
///
/// The caller supplies the raw OpenPGP message bytes (binary SEIPD packets
/// produced by the OpenPGP backend's encrypt). Part 1 is the fixed
/// `application/pgp-encrypted` body `Version: 1` (the RFC 3156 §2 control
/// part); part 2 carries the OpenPGP message as
/// `application/octet-stream; name="encrypted.asc"` with
/// `Content-Transfer-Encoding: base64`.
pub fn wrap_encrypted(openpgp_message: &[u8]) -> Vec<u8> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(openpgp_message);
    let mut out = Vec::new();
    out.extend_from_slice(
        format!(
            "Content-Type: multipart/encrypted; \
             protocol=\"application/pgp-encrypted\"; \
             boundary=\"{ENCRYPTED_BOUNDARY}\"\r\n\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(b"This is an OpenPGP/MIME encrypted message (RFC 3156)\r\n");
    // Part 1: the Version: 1 control part.
    out.extend_from_slice(format!("--{ENCRYPTED_BOUNDARY}\r\n").as_bytes());
    out.extend_from_slice(b"Content-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n");
    // Part 2: the OpenPGP message (base64-transported).
    out.extend_from_slice(format!("--{ENCRYPTED_BOUNDARY}\r\n").as_bytes());
    out.extend_from_slice(
        b"Content-Type: application/octet-stream; name=\"encrypted.asc\"\r\n\
          Content-Transfer-Encoding: base64\r\n\
          Content-Disposition: inline; filename=\"encrypted.asc\"\r\n\r\n",
    );
    for chunk in b64.as_bytes().chunks(72) {
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{ENCRYPTED_BOUNDARY}--\r\n").as_bytes());
    out
}

/// Build a `multipart/signed` MIME **entity** (Content-Type header + blank +
/// multipart body) framing a detached OpenPGP signature per RFC 3156 §1.
///
/// # Signed region (read before computing the signature)
///
/// - `payload` is the **body bytes only** (no MIME headers); `payload_mime`
///   is its `Content-Type` value (e.g. `"text/plain; charset=utf-8"`).
/// - **The detached signature MUST be computed over the exact part-1 MIME
///   entity bytes that `wrap_signed` emits**: `Content-Type:
///   <payload_mime>\r\n\r\n<canonicalized body>` (the Content-Type header,
///   a blank line, then the body with **exactly one** trailing CRLF).
///   Signing `payload` alone is WRONG: the signature will not verify
///   against the part-1 entity `wrap_signed` emits.
/// - `wrap_signed` applies [`ensure_one_trailing_crlf`] to `payload`
///   internally (idempotently), so a caller may either reproduce the part-1
///   layout and pass the original `payload`, or pre-canonicalize and pass
///   the canonicalized bytes. Both produce the same signature.
///
/// `detached_sig` is the raw detached-signature packet bytes (binary),
/// base64-transported in part 2. `hash` is the signature's hash algorithm;
/// it becomes the `micalg` parameter value per RFC 3156 §1 + RFC 4880 §9.4
/// (see [`micalg_name`] for the exact spellings and the Ed25519-only current
/// scope).
pub fn wrap_signed(
    payload: &[u8],
    payload_mime: &str,
    detached_sig: &[u8],
    hash: HashAlgorithm,
) -> Vec<u8> {
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(detached_sig);
    let micalg = micalg_name(hash);
    let body_canonical = ensure_one_trailing_crlf(payload);
    let mut out = Vec::new();
    out.extend_from_slice(
        format!(
            "Content-Type: multipart/signed; \
             protocol=\"application/pgp-signature\"; \
             micalg={micalg}; \
             boundary=\"{SIGNED_BOUNDARY}\"\r\n\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(b"This is an OpenPGP/MIME signed message (RFC 3156)\r\n");
    // Part 1: the signed body entity (Content-Type + blank + canonicalized
    // body, ending in exactly one CRLF).
    out.extend_from_slice(format!("--{SIGNED_BOUNDARY}\r\n").as_bytes());
    out.extend_from_slice(format!("Content-Type: {payload_mime}\r\n\r\n").as_bytes());
    out.extend_from_slice(&body_canonical);
    // Part 2: the detached signature (base64-transported).
    out.extend_from_slice(format!("--{SIGNED_BOUNDARY}\r\n").as_bytes());
    out.extend_from_slice(
        b"Content-Type: application/pgp-signature; name=\"signature.asc\"\r\n\
          Content-Transfer-Encoding: base64\r\n\
          Content-Disposition: attachment; filename=\"signature.asc\"\r\n\r\n",
    );
    for chunk in sig_b64.as_bytes().chunks(72) {
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{SIGNED_BOUNDARY}--\r\n").as_bytes());
    out
}

#[cfg(test)]
#[path = "pgp_mime_tests.rs"]
mod pgp_mime_tests;
