// SPDX-License-Identifier: GPL-3.0-only
//! Autocrypt (Level 1) emit-side helpers: kylins' `mail/crypto/autocrypt.rs`
//! send half ported. [`emit_gossip_header`] builds the folded
//! `Autocrypt-Gossip:` headers the send gate prepends to the inner entity
//! BEFORE encryption (never in cleartext headers);
//! [`own_autocrypt_header_for_account`] builds the sender's own folded
//! `Autocrypt:` header for the outer block. [`minimal_export`] produces the
//! small key blob those headers carry: primary + encryption-capable subkeys,
//! self-signatures only (third-party certifications stripped), secret
//! material removed. The inbound parse/stage/consume half is the receive
//! slice (SP4c).

use base64::Engine as _;
use sequoia_openpgp as openpgp;

use openpgp::cert::prelude::*;
use openpgp::parse::Parse;
use openpgp::policy::StandardPolicy;
use openpgp::serialize::Serialize as _;
use openpgp::{KeyHandle, Packet};

use crate::store::CryptoStore;

/// OpenPGP standard string used across `collected_keys` / `crypto_keys`.
const STANDARD_OPENPGP: &str = "openpgp";

/// Parse the first OpenPGP cert out of `keydata` (armored or binary both
/// accepted: `CertParser::from_bytes` auto-detects). Returns `(fingerprint
/// lowercase-hex, cert)`. `None` when no cert parses.
pub fn parse_first_cert(keydata: &[u8]) -> Option<(String, openpgp::Cert)> {
    let parser = CertParser::from_bytes(keydata).ok()?;
    let cert = parser.flatten().next()?;
    Some((cert.fingerprint().to_hex().to_ascii_lowercase(), cert))
}

/// Minimal public-key export for the `keydata` attribute: primary key +
/// user IDs + encryption-capable subkeys, with only self-signatures kept
/// (third-party certifications stripped) and all secret material removed.
/// This is what keeps the `Autocrypt:` header small (the same shape
/// Thunderbird sends via RNP's minimal export).
///
/// Binary OpenPGP serialization (NOT armored): the header base64s it.
///
/// # Errors
/// Fails when a packet cannot be serialized.
pub fn minimal_export(cert: &openpgp::Cert) -> Result<Vec<u8>, String> {
    // 1. Drop secret material, then keep only encryption-capable subkeys
    //    (transport or storage). Subkeys without a valid binding self-sig
    //    are dropped by the policy check (fail-closed to "not
    //    encryption-capable").
    let public = cert.clone().strip_secret_key_material();
    let policy = StandardPolicy::new();
    let public = public.retain_subkeys(|ka| {
        ka.with_policy(&policy, None)
            .ok()
            .and_then(|vka| vka.key_flags())
            .is_some_and(|flags| flags.for_transport_encryption() || flags.for_storage_encryption())
    });

    // 2. Packet-level filter: drop every Signature packet NOT issued by the
    //    primary key (third-party certifications). Self-sigs carry the key
    //    flags / expiry the receiver needs; everything else is bulk.
    let primary_fp = public.fingerprint();
    let primary_id = public.keyid();
    let mut out = Vec::new();
    for packet in public.into_packets() {
        match packet {
            Packet::Signature(sig) => {
                let self_issued = sig.issuer_fingerprints().any(|f| *f == primary_fp)
                    || sig.get_issuers().iter().any(|h| match h {
                        KeyHandle::KeyID(id) => *id == primary_id,
                        KeyHandle::Fingerprint(f) => *f == primary_fp,
                    });
                if !self_issued {
                    continue;
                }
                Packet::Signature(sig)
                    .serialize(&mut out)
                    .map_err(|e| e.to_string())?;
            }
            other => {
                other.serialize(&mut out).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(out)
}

/// Fold a `name: value` header into CRLF continuation lines at ~72 columns.
/// The first line carries `name: ` + as much of `value` as fits;
/// continuation lines start with a single space. `value` must be free of
/// CR/LF (base64 + attribute text, enforced by callers).
fn fold_header(name: &str, value: &str) -> String {
    const LIMIT: usize = 72;
    let mut out = String::new();
    let prefix = format!("{name}: ");
    let mut line_len = prefix.len();
    out.push_str(&prefix);
    for (i, token) in value.split(' ').enumerate() {
        // tokens are separated by single spaces in the attribute string
        let sep = usize::from(i != 0);
        if line_len + sep + token.len() > LIMIT && line_len > prefix.len() + 1 {
            out.push_str("\r\n ");
            line_len = 1;
        } else if i > 0 {
            out.push(' ');
            line_len += 1;
        }
        // Long tokens (the base64 keydata blob) are hard-chunked.
        let mut rest = token;
        while line_len + rest.len() > LIMIT {
            let take = LIMIT - line_len;
            out.push_str(&rest[..take]);
            out.push_str("\r\n ");
            rest = &rest[take..];
            line_len = 1;
        }
        out.push_str(rest);
        line_len += rest.len();
    }
    out
}

/// Build the folded `Autocrypt:` header line(s) (no trailing CRLF) for the
/// sender's own key. `key_bytes` is the [`minimal_export`] binary blob.
pub fn emit_autocrypt_header(addr: &str, key_bytes: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(key_bytes);
    fold_header(
        "Autocrypt",
        &format!("addr={addr}; prefer-encrypt=mutual; keydata={b64}"),
    )
}

/// Build the folded `Autocrypt-Gossip:` header line(s) (no trailing CRLF)
/// carrying `addr`'s key. Goes INSIDE the encrypted part only.
pub fn emit_gossip_header(addr: &str, key_bytes: &[u8]) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(key_bytes);
    fold_header("Autocrypt-Gossip", &format!("addr={addr}; keydata={b64}"))
}

/// Build the sender's own `Autocrypt:` header for the send path. Returns
/// `Some(header)` only when ALL of the following hold (soft-fail `None`
/// otherwise: a missing/misconfigured key must never block a send):
/// - the account's `send_autocrypt_headers` setting is on (default on), AND
/// - the account has a default OpenPGP signing key (the identity key), AND
/// - that key has private material (a public-only row is not "our" key).
///
/// The header carries the [`minimal_export`] of the key's public cert.
pub fn own_autocrypt_header_for_account(
    store: &CryptoStore,
    account_id: &str,
    account_email: &str,
) -> Option<String> {
    match crate::crypto_policy::get_send_autocrypt_headers(store, account_id) {
        Ok(true) => {}
        Ok(false) => return None,
        Err(e) => {
            log::warn!("[autocrypt] {account_id} read send_autocrypt_headers failed: {e}");
            return None;
        }
    }
    let key_row = match crate::get_default_signing_key(
        store,
        account_id,
        astra_crypto_core::Standard::OpenPgp,
    ) {
        Ok(Some(row)) => row,
        Ok(None) => return None,
        Err(e) => {
            log::warn!("[autocrypt] {account_id} default signing key lookup failed: {e}");
            return None;
        }
    };
    let record =
        match crate::get_crypto_key_full(store, account_id, STANDARD_OPENPGP, &key_row.fingerprint)
        {
            Ok(Some(rec)) => rec,
            Ok(None) => return None,
            Err(e) => {
                log::warn!("[autocrypt] {account_id} key read failed: {e}");
                return None;
            }
        };
    // Public-only row: not our key; emitting an Autocrypt header for it
    // would advertise a key we cannot decrypt/sign with.
    record.private_data.as_ref()?;
    let public_bytes = match hex::decode(&record.public_data) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("[autocrypt] {account_id} public_data hex decode failed: {e}");
            return None;
        }
    };
    let Some((_, cert)) = parse_first_cert(&public_bytes) else {
        log::warn!("[autocrypt] {account_id} public_data contained no cert");
        return None;
    };
    match minimal_export(&cert) {
        Ok(blob) => Some(emit_autocrypt_header(account_email, &blob)),
        Err(e) => {
            log::warn!("[autocrypt] {account_id} minimal export failed: {e}");
            None
        }
    }
}

/// Inject header line(s) into the ROOT header block of a MIME message: each
/// entry is inserted immediately before the blank line separating headers
/// from the body. Entries may themselves contain folded continuation lines
/// (CRLF + WSP) but must not contain a bare CRLF pair. Returns the input
/// with the headers appended at the end of the block; when the message has
/// no recognizable header/body separator, the headers are appended at the
/// end (defensive: a well-formed built message always has one).
pub fn inject_headers(mime: &[u8], headers: &[String]) -> Vec<u8> {
    if headers.is_empty() {
        return mime.to_vec();
    }
    let sep = mime
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| (i, 4usize))
        .or_else(|| {
            mime.windows(2)
                .position(|w| w == b"\n\n")
                .map(|i| (i, 2usize))
        });
    let Some((idx, sep_len)) = sep else {
        let mut out = mime.to_vec();
        for h in headers {
            out.extend_from_slice(h.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
        return out;
    };
    let mut out =
        Vec::with_capacity(mime.len() + headers.iter().map(|h| h.len() + 2).sum::<usize>());
    out.extend_from_slice(&mime[..idx]);
    out.extend_from_slice(b"\r\n");
    for h in headers {
        out.extend_from_slice(h.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    // The blank line separating the (now extended) header block from the body.
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(&mime[idx + sep_len..]);
    out
}

#[cfg(test)]
#[path = "autocrypt_tests.rs"]
mod autocrypt_tests;
