// SPDX-License-Identifier: GPL-3.0-only
//! The MIME-wrapping layer for the send gate: kylins' `mail/crypto/mime.rs`
//! send-side half ported. Pure byte construction, no mail-builder
//! dependency, so clear-sign part-1 byte exactness is fully under our
//! control (the signature must cover the exact part-1 bytes incl. the
//! trailing CRLF before the boundary).
//!
//! Byte structure validated against Thunderbird
//! (`mailnews/extensions/smime/nsMsgComposeSecure.cpp` + `nsCMS.cpp`):
//!   - part 1 = the full body MIME entity (Content-Type **and**
//!     Content-Transfer-Encoding)
//!   - the signed bytes include exactly one trailing CRLF (the body's own
//!     terminator; do NOT double-add: [`ensure_one_trailing_crlf`]
//!     guarantees it)
//!   - header emit: **unquoted** `micalg=sha-256`, **quoted**
//!     `boundary="…"`, the `"This is a cryptographically signed message in
//!     MIME format."` preamble, part-2 `Content-Description: S/MIME
//!     Cryptographic Signature`, base64 line-wrapped at column 72.
//!
//! Deviation from kylins: the boundary constants are renamed
//! `----=_kylins_…` -> `----=_astra_…` (honest provenance; boundary values
//! are arbitrary by RFC 2046 and every verifier treats them as opaque).

use base64::Engine;

use crate::apply::CryptoSendError;

/// The set of MIME *entity* headers that belong to a body part (move with
/// the body entity), not the outer RFC5322 message. Everything else
/// (From/To/Subject/Date/Message-ID/MIME-Version/…) stays outer.
const ENTITY_HEADERS: &[&str] = &[
    "content-type",
    "content-transfer-encoding",
    "content-disposition",
    "content-id",
    "content-description",
    "content-language",
];

/// Outer message header lines, each `Name: value\r\n`-terminated, NO entity
/// headers, NO trailing blank line.
#[derive(Debug, Clone)]
pub struct MessageHeaders {
    /// The rebuilt outer header block (CRLF-joined, no trailing CRLF).
    pub headers: String,
}

/// The body entity bytes (entity headers + blank line + body).
#[derive(Debug, Clone)]
pub struct EntityBytes(pub Vec<u8>);

/// Split a built RFC5322 message into the outer message headers (no entity
/// headers) and the body entity (entity headers + blank line + body).
///
/// Folding-aware: a header line beginning with space or tab is an RFC 5322
/// continuation of the previous header and inherits the SAME bucket (outer
/// vs entity) as the line it continues. This matters because mail-builder
/// folds long `Content-Type` values (multipart `boundary` attributes push
/// the line past 76 cols) onto a following `\r\n\t`-prefixed line; without
/// this rule the continuation would be mis-classified as an outer header
/// and leak out of the signed entity.
///
/// # Errors
/// [`CryptoSendError::Mime`] when the input is not UTF-8 or has no
/// header/body blank line.
pub fn split_message(full: &[u8]) -> Result<(MessageHeaders, EntityBytes), CryptoSendError> {
    let s =
        std::str::from_utf8(full).map_err(|e| CryptoSendError::Mime(format!("not utf-8: {e}")))?;
    let blank = s
        .find("\r\n\r\n")
        .ok_or_else(|| CryptoSendError::Mime("no header/body blank line".into()))?;
    let header_block = &s[..blank];
    let body = &s[blank + 4..];

    let mut outer = String::new();
    let mut entity_headers = String::new();
    // Track the bucket the previous header line was routed to so a folded
    // continuation line (starts with space/tab) inherits it. Before the
    // first header line there is no preceding bucket, so continuations
    // default outer.
    let mut last_was_entity = false;
    for line in header_block.split("\r\n") {
        let is_continuation = line.starts_with(' ') || line.starts_with('\t');
        let is_entity = if is_continuation {
            last_was_entity
        } else {
            let name = line
                .split(':')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            ENTITY_HEADERS.contains(&name.as_str())
        };
        let target = if is_entity {
            &mut entity_headers
        } else {
            &mut outer
        };
        if !target.is_empty() {
            target.push_str("\r\n");
        }
        target.push_str(line);
        last_was_entity = is_entity;
    }
    let entity = format!("{entity_headers}\r\n\r\n{body}").into_bytes();
    Ok((MessageHeaders { headers: outer }, EntityBytes(entity)))
}

/// Ensure `bytes` ends with exactly one `\r\n`. Clear-sign part-1 byte
/// exactness. Collapses any doubled trailing CRLFs (`…\r\n\r\n` -> `…\r\n`)
/// then adds a single CRLF if none was present.
pub fn ensure_one_trailing_crlf(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    while out.ends_with(b"\r\n\r\n") {
        out.truncate(out.len() - 2); // collapse doubled trailing CRLF
    }
    if !out.ends_with(b"\r\n") {
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// The `multipart/signed` boundary (S/MIME clear-sign shape).
pub const SIGNED_BOUNDARY: &str = "----=_astra_smime_signed_0001";

/// Build a `multipart/signed` MIME **entity** (Content-Type header + blank +
/// multipart body). Part 1 is EXACTLY `inner_entity_with_crlf`: the caller
/// signs that exact slice and passes the resulting detached SignedData DER.
///
/// Layout (Thunderbird-faithful):
/// ```text
/// Content-Type: multipart/signed; protocol="application/pkcs7-signature";
///    micalg=sha-256; boundary="----=_astra_smime_signed_0001"
/// <blank>
/// This is a cryptographically signed message in MIME format.
/// --{boundary}
/// {part 1 = inner_entity_with_crlf, ending in exactly one CRLF}
/// --{boundary}
/// Content-Type: application/pkcs7-signature; name="smime.p7s"
/// Content-Transfer-Encoding: base64
/// Content-Disposition: attachment; filename="smime.p7s"
/// Content-Description: S/MIME Cryptographic Signature
/// <blank>
/// {base64(sha-256 detached signature), wrapped at 72 cols}
/// --{boundary}--
/// ```
pub fn wrap_multipart_signed(inner_entity_with_crlf: &[u8], signed_der: &[u8]) -> Vec<u8> {
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(signed_der);
    let mut out = Vec::new();
    out.extend_from_slice(
        format!(
            "Content-Type: multipart/signed; protocol=\"application/pkcs7-signature\"; \
             micalg=sha-256; boundary=\"{SIGNED_BOUNDARY}\"\r\n\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(b"This is a cryptographically signed message in MIME format.\r\n");
    // Part 1: the signed body entity (exact bytes).
    out.extend_from_slice(format!("--{SIGNED_BOUNDARY}\r\n").as_bytes());
    out.extend_from_slice(inner_entity_with_crlf);
    // Part 2: the detached signature.
    out.extend_from_slice(format!("--{SIGNED_BOUNDARY}\r\n").as_bytes());
    out.extend_from_slice(
        b"Content-Type: application/pkcs7-signature; name=\"smime.p7s\"\r\n\
          Content-Transfer-Encoding: base64\r\n\
          Content-Disposition: attachment; filename=\"smime.p7s\"\r\n\
          Content-Description: S/MIME Cryptographic Signature\r\n\r\n",
    );
    for chunk in sig_b64.as_bytes().chunks(72) {
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{SIGNED_BOUNDARY}--\r\n").as_bytes());
    out
}

/// Build an `application/pkcs7-mime` MIME **entity** (headers + blank +
/// base64 body, 72-col wrapped). `smime_type` = `"enveloped-data"`
/// (encryption) or `"signed-data"` (opaque signing).
pub fn wrap_enveloped(cms_der: &[u8], smime_type: &str) -> Vec<u8> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(cms_der);
    let mut out = Vec::new();
    out.extend_from_slice(
        format!(
            "Content-Type: application/pkcs7-mime; smime-type={smime_type}; name=\"smime.p7m\"\r\n\
             Content-Transfer-Encoding: base64\r\n\
             Content-Disposition: attachment; filename=\"smime.p7m\"\r\n\r\n"
        )
        .as_bytes(),
    );
    for chunk in b64.as_bytes().chunks(72) {
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\r\n");
    }
    out
}

// ──────────────────────────────────────────────────────────────────────────
// Protected Headers (encrypted Subject)
// draft-ietf-lamps-header-protection, Thunderbird production format
// (`mail/extensions/openpgp/content/modules/mimeEncrypt.sys.mjs`
// `writeSecureHeaders` + `encryptedHeaders`).
// ──────────────────────────────────────────────────────────────────────────

/// Thunderbird's outer placeholder for a protected subject
/// (`EnigmailFuncs.getProtectedSubjectText()`, literally three dots). Using
/// the same placeholder maximises interop recognition (other clients
/// special-case `"..."`).
pub const PROTECTED_SUBJECT_PLACEHOLDER: &str = "...";

/// Boundary for the inner `multipart/mixed; protected-headers="v1"` entity.
pub const PROTECTED_HEADERS_BOUNDARY: &str = "----=_astra_protected_headers_0001";

/// Substitute the outer `Subject:` header with the protected placeholder
/// (`Subject: ...`), returning the rebuilt outer header block plus the
/// removed RAW Subject header block (folded continuation lines preserved, no
/// trailing CRLF) for the inner protected-headers entity. When no Subject
/// header exists, the placeholder is prepended and the returned block is
/// `None` (mirrors TB: the outer placeholder is always emitted when
/// protecting, but the inner `Subject:` only when there was a real subject).
///
/// Folding-aware: a line beginning with space/tab continues the previous
/// header (same rule as [`split_message`]); a folded Subject is captured
/// (and removed) together with its continuation lines.
pub fn hide_subject_header(outer: &str) -> (String, Option<String>) {
    let mut rebuilt: Vec<&str> = Vec::new();
    let mut subject_lines: Vec<&str> = Vec::new();
    let mut in_subject = false;
    let mut placeholder_emitted = false;
    for line in outer.split("\r\n") {
        let is_continuation = line.starts_with(' ') || line.starts_with('\t');
        if is_continuation {
            if in_subject {
                subject_lines.push(line);
            } else {
                rebuilt.push(line);
            }
            continue;
        }
        let name = line
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if name == "subject" {
            // First Subject: capture + drop-in the placeholder at the same
            // position. Any duplicate Subject headers (RFC 5322 forbids
            // them) are captured + dropped too: the real subject must not
            // survive anywhere in the outer block.
            if !placeholder_emitted {
                rebuilt.push("Subject: ...");
                placeholder_emitted = true;
            }
            in_subject = true;
            subject_lines.push(line);
        } else {
            in_subject = false;
            rebuilt.push(line);
        }
    }
    let new_outer = if placeholder_emitted {
        rebuilt.join("\r\n")
    } else {
        // No Subject header at all: prepend the placeholder (TB emits the
        // outer placeholder first, before its Content-Type).
        format!("Subject: {PROTECTED_SUBJECT_PLACEHOLDER}\r\n{outer}")
    };
    // Treat an empty Subject value (`Subject:` / `Subject: \r\n `) as no
    // real subject: nothing to protect inside (TB skips the inner Subject
    // when `originalSubject` is empty).
    let subject_block = if subject_lines.is_empty() {
        None
    } else {
        let value: String = subject_lines
            .iter()
            .map(|l| {
                l.split_once(':')
                    .map(|(_, v)| v.trim())
                    .unwrap_or_else(|| l.trim())
            })
            .collect::<Vec<_>>()
            .join(" ");
        if value.trim().is_empty() {
            None
        } else {
            Some(subject_lines.join("\r\n"))
        }
    };
    (new_outer, subject_block)
}

/// Wrap a body MIME entity in the Lamps/Thunderbird protected-headers inner
/// structure: a `multipart/mixed; protected-headers="v1"` entity whose own
/// header block carries the real `Subject:` (and whose single subpart is the
/// original body entity). This is the shape Thunderbird PRODUCES on send and
/// REQUIRES on receive, so emitting it is what makes TB restore our
/// subjects.
///
/// `signed` adds TB's `hp="cipher"` parameter (TB emits it when the message
/// is also signed; receive ignores it: cosmetic parity).
///
/// Layout:
/// ```text
/// Content-Type: multipart/mixed; boundary="----=_astra_protected_headers_0001";
///  protected-headers="v1"[; hp="cipher"]
/// Subject: {real subject, folded exactly as it appeared in the outer block}
/// <blank>
/// --{boundary}
/// {original body entity, exactly one trailing CRLF}
/// --{boundary}--
/// ```
pub fn wrap_protected_headers(entity: &[u8], subject_block: Option<&str>, signed: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut ct = format!(
        "Content-Type: multipart/mixed; boundary=\"{PROTECTED_HEADERS_BOUNDARY}\";\r\n protected-headers=\"v1\""
    );
    if signed {
        ct.push_str("; hp=\"cipher\"");
    }
    out.extend_from_slice(ct.as_bytes());
    out.extend_from_slice(b"\r\n");
    if let Some(block) = subject_block {
        out.extend_from_slice(block.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    // Blank line ends the wrapper's header block; the original entity
    // becomes the sole subpart (its own headers + body, exactly one
    // trailing CRLF).
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(format!("--{PROTECTED_HEADERS_BOUNDARY}\r\n").as_bytes());
    out.extend_from_slice(&ensure_one_trailing_crlf(entity));
    out.extend_from_slice(format!("--{PROTECTED_HEADERS_BOUNDARY}--\r\n").as_bytes());
    out
}

#[cfg(test)]
#[path = "mime_tests.rs"]
mod mime_tests;
