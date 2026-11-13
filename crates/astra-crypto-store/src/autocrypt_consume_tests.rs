// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the Autocrypt consume half: the emit-to-consume round-trip, the
//! Autocrypt Level 1 addr-to-From binding (a mismatch discards), malformed
//! keydata staging nothing, the gossip parse (the addr is the recipient peer,
//! no From binding), and the re-open dedup on the collected staging key.

use super::*;
use crate::store::CryptoStore;

fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store
}

/// A fresh cert for `email` and its [`minimal_export`] blob (the exact bytes
/// an Autocrypt header carries), with the primary fingerprint.
fn cert_blob(email: &str) -> (String, Vec<u8>) {
    let cert = astra_crypto_openpgp::engine::generate(&format!("Peer <{email}>")).unwrap();
    let fp = astra_crypto_openpgp::keymap::fingerprint_of(&cert)
        .as_str()
        .to_string();
    let blob = minimal_export(&cert).expect("minimal export");
    (fp, blob)
}

/// The staged rows for the account's OpenPGP standard.
fn staged(store: &CryptoStore) -> Vec<crate::CollectedKeyRow> {
    crate::list_collected_keys_for_account(store, "a1", "openpgp").unwrap()
}

#[test]
fn emit_consume_round_trip_stages_the_sender_key() {
    let store = test_store();
    let (fp, blob) = cert_blob("sender@x.com");
    let header = emit_autocrypt_header("sender@x.com", &blob);
    let outer = format!("From: Sender <sender@x.com>\r\n{header}\r\n");

    let n = consume_autocrypt_header(&store, "a1", outer.as_bytes(), Some("sender@x.com"));
    assert_eq!(n, 1, "the sender key stages once");

    let rows = staged(&store);
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.source.as_deref(), Some("autocrypt"));
    assert_eq!(row.standard.as_deref(), Some("openpgp"));
    assert_eq!(row.peer_email.as_deref(), Some("sender@x.com"));
    assert_eq!(row.fingerprint.as_deref(), Some(fp.as_str()));
    assert_eq!(
        row.public_data, blob,
        "the observed keydata blob is staged verbatim"
    );
}

#[test]
fn the_addr_binding_is_case_insensitive() {
    let store = test_store();
    let (_fp, blob) = cert_blob("sender@x.com");
    let header = emit_autocrypt_header("Sender@X.com", &blob);
    let outer = format!("From: sender@x.com\r\n{header}\r\n");
    assert_eq!(
        consume_autocrypt_header(&store, "a1", outer.as_bytes(), Some("sender@x.com")),
        1,
        "the addr matches From case-insensitively"
    );
}

#[test]
fn an_addr_not_matching_from_is_discarded() {
    let store = test_store();
    let (_fp, blob) = cert_blob("sender@x.com");
    let header = emit_autocrypt_header("sender@x.com", &blob);
    let outer = format!("From: Other <other@x.com>\r\n{header}\r\n");
    assert_eq!(
        consume_autocrypt_header(&store, "a1", outer.as_bytes(), Some("other@x.com")),
        0,
        "Autocrypt Level 1 discards a header whose addr is not From"
    );
    assert_eq!(staged(&store).len(), 0);
}

#[test]
fn a_missing_from_stages_nothing() {
    let store = test_store();
    let (_fp, blob) = cert_blob("sender@x.com");
    let header = emit_autocrypt_header("sender@x.com", &blob);
    let outer = format!("To: a1@x.com\r\n{header}\r\n");
    assert_eq!(
        consume_autocrypt_header(&store, "a1", outer.as_bytes(), None),
        0,
        "no From address means the binding cannot be checked"
    );
    assert_eq!(staged(&store).len(), 0);
}

#[test]
fn malformed_keydata_stages_nothing_and_never_panics() {
    let store = test_store();

    // Invalid base64 in the keydata attribute.
    let bad = "Autocrypt: addr=sender@x.com; prefer-encrypt=mutual; keydata=!!!not-base64!!!";
    let outer = format!("From: sender@x.com\r\n{bad}\r\n");
    assert_eq!(
        consume_autocrypt_header(&store, "a1", outer.as_bytes(), Some("sender@x.com")),
        0
    );

    // Valid base64 that is not an OpenPGP cert.
    let not_cert = base64::engine::general_purpose::STANDARD.encode(b"this is not an openpgp cert");
    let hdr = format!("Autocrypt: addr=sender@x.com; keydata={not_cert}");
    let outer = format!("From: sender@x.com\r\n{hdr}\r\n");
    assert_eq!(
        consume_autocrypt_header(&store, "a1", outer.as_bytes(), Some("sender@x.com")),
        0
    );

    // A header with no keydata attribute at all.
    let no_key = "Autocrypt: addr=sender@x.com; prefer-encrypt=mutual";
    let outer = format!("From: sender@x.com\r\n{no_key}\r\n");
    assert_eq!(
        consume_autocrypt_header(&store, "a1", outer.as_bytes(), Some("sender@x.com")),
        0
    );

    // An empty addr.
    let empty_addr = format!(
        "Autocrypt: addr=; keydata={}",
        base64::engine::general_purpose::STANDARD.encode(b"whatever")
    );
    let outer = format!("From: sender@x.com\r\n{empty_addr}\r\n");
    assert_eq!(
        consume_autocrypt_header(&store, "a1", outer.as_bytes(), Some("sender@x.com")),
        0
    );

    assert!(
        staged(&store).is_empty(),
        "no malformed header stages a row"
    );
}

#[test]
fn gossip_headers_stage_the_recipient_peers_without_a_from_binding() {
    let store = test_store();
    let (fp3, blob3) = cert_blob("peer3@x.com");
    let (fp4, blob4) = cert_blob("peer4@x.com");
    let h3 = emit_gossip_header("peer3@x.com", &blob3);
    let h4 = emit_gossip_header("peer4@x.com", &blob4);
    // The gossip headers ride the inner entity's root header block.
    let inner = format!("Content-Type: text/plain\r\n{h3}\r\n{h4}\r\n\r\nbody");

    let n = consume_gossip_headers(&store, "a1", inner.as_bytes());
    assert_eq!(n, 2, "both peer keys stage");

    let rows = staged(&store);
    assert_eq!(rows.len(), 2);
    let mut peers: Vec<String> = rows.iter().map(|r| r.peer_email.clone().unwrap()).collect();
    peers.sort();
    assert_eq!(
        peers,
        vec!["peer3@x.com".to_string(), "peer4@x.com".to_string()]
    );
    for row in &rows {
        assert_eq!(row.source.as_deref(), Some("autocrypt"));
        assert_eq!(row.standard.as_deref(), Some("openpgp"));
    }
    let fps: Vec<String> = rows
        .iter()
        .map(|r| r.fingerprint.clone().unwrap())
        .collect();
    assert!(fps.contains(&fp3) && fps.contains(&fp4));
}

#[test]
fn re_consuming_the_same_header_does_not_duplicate() {
    let store = test_store();
    let (_fp, blob) = cert_blob("sender@x.com");
    let header = emit_autocrypt_header("sender@x.com", &blob);
    let outer = format!("From: sender@x.com\r\n{header}\r\n");

    assert_eq!(
        consume_autocrypt_header(&store, "a1", outer.as_bytes(), Some("sender@x.com")),
        1
    );
    assert_eq!(
        consume_autocrypt_header(&store, "a1", outer.as_bytes(), Some("sender@x.com")),
        0,
        "the (account, standard, fingerprint, source) dedup holds on re-open"
    );
    assert_eq!(staged(&store).len(), 1);
}

#[test]
fn parse_extracts_addr_prefer_encrypt_and_keydata_through_the_fold() {
    let (_fp, blob) = cert_blob("sender@x.com");
    // A real key blob folds across many lines; the consume must unfold and
    // strip the fold whitespace before the base64 decode.
    let header = emit_autocrypt_header("sender@x.com", &blob);
    let values = astra_crypto_core::mime::header_values(header.as_bytes(), "autocrypt");
    assert_eq!(values.len(), 1);
    let parsed = parse_autocrypt_value(&values[0]).expect("the emitted header parses");
    assert_eq!(parsed.addr, "sender@x.com");
    assert_eq!(parsed.prefer_encrypt.as_deref(), Some("mutual"));
    assert_eq!(
        parsed.keydata, blob,
        "the folded keydata round-trips byte-exact"
    );

    // Gossip carries no prefer-encrypt attribute.
    let gossip = emit_gossip_header("peer@x.com", &blob);
    let gvalues = astra_crypto_core::mime::header_values(gossip.as_bytes(), "autocrypt-gossip");
    let gparsed = parse_autocrypt_value(&gvalues[0]).expect("the gossip header parses");
    assert_eq!(gparsed.addr, "peer@x.com");
    assert_eq!(gparsed.prefer_encrypt, None);
    assert_eq!(gparsed.keydata, blob);
}
