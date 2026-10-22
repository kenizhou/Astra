// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the Autocrypt emit helpers: minimal export shape, header
//! folding, header injection, and the own-header gating.

use super::*;
use crate::store::CryptoStore;
use sequoia_openpgp::cert::amalgamation::key::PrimaryKey;

fn test_store() -> CryptoStore {
    crate::vault::set_test_master_key([0x42u8; 32]);
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store
}

/// Seed the account's default OpenPGP signing key (full material); returns
/// the cert's fingerprint.
fn seed_default_pgp_key(store: &CryptoStore, with_private: bool) -> String {
    let cert = astra_crypto_openpgp::engine::generate("A One <a1@x.com>").unwrap();
    let fp = astra_crypto_openpgp::keymap::fingerprint_of(&cert)
        .as_str()
        .to_string();
    crate::upsert_crypto_key(
        store,
        &crate::CryptoKeyRecord {
            row: crate::CryptoKeyRow {
                account_id: "a1".to_string(),
                standard: "openpgp".to_string(),
                key_type: "cert".to_string(),
                email: Some("a1@x.com".to_string()),
                fingerprint: fp.clone(),
                origin: "generated".to_string(),
                ..Default::default()
            },
            public_data: hex::encode(
                astra_crypto_openpgp::keymap::cert_to_public_blob(&cert).unwrap(),
            ),
            private_data: with_private.then(|| {
                hex::encode(astra_crypto_openpgp::keymap::cert_to_secret_blob(&cert).unwrap())
            }),
            policy_json: None,
        },
    )
    .unwrap();
    crate::set_default_signing_key(store, "a1", "openpgp", &fp).unwrap();
    fp
}

#[test]
fn minimal_export_keeps_encryption_subkeys_and_drops_secrets() {
    let cert = astra_crypto_openpgp::engine::generate("B Ob <b@x.com>").unwrap();
    let blob = minimal_export(&cert).expect("minimal export");
    let (fp, exported) = parse_first_cert(&blob).expect("the blob re-parses");
    assert_eq!(
        fp,
        astra_crypto_openpgp::keymap::fingerprint_of(&cert)
            .as_str()
            .to_string(),
        "the primary survives"
    );
    // Secret material is gone: the exported cert has no private keys.
    assert!(
        exported.keys().all(|ka| !ka.key().has_secret()),
        "no secret material rides the header"
    );
    // The generated cert's signing subkey is dropped (encryption-capable
    // subkeys only); at least the transport encryption subkey remains.
    let subkey_count = exported.keys().filter(|ka| !ka.primary()).count();
    assert_eq!(subkey_count, 1, "only the encryption subkey survives");
    // The blob is smaller than a full public export (self-sigs only).
    let full = astra_crypto_openpgp::keymap::cert_to_public_blob(&cert).unwrap();
    assert!(blob.len() <= full.len());
}

#[test]
fn emit_gossip_header_folds_and_carries_addr_and_keydata() {
    let header = emit_gossip_header("peer@x.com", b"key-bytes");
    assert!(header.starts_with("Autocrypt-Gossip: addr=peer@x.com; keydata="));
    for line in header.split("\r\n") {
        assert!(line.len() <= 72, "folded at ~72 cols: {line:?}");
    }
    assert!(header.contains("a2V5LWJ5dGVz"), "the key is base64");
}

#[test]
fn emit_autocrypt_header_carries_prefer_encrypt_mutual() {
    let header = emit_autocrypt_header("me@x.com", b"key-bytes");
    assert!(header.starts_with("Autocrypt: addr=me@x.com; prefer-encrypt=mutual; keydata="));
}

#[test]
fn inject_headers_inserts_before_the_blank_line() {
    let mime = b"From: a@x.com\r\nSubject: hi\r\n\r\nbody";
    let out = inject_headers(
        mime,
        &["X-Test: 1".to_string(), "X-Folded: a\r\n b".to_string()],
    );
    let text = String::from_utf8(out).unwrap();
    assert_eq!(
        text, "From: a@x.com\r\nSubject: hi\r\nX-Test: 1\r\nX-Folded: a\r\n b\r\n\r\nbody",
        "headers land at the end of the root block, folded lines preserved"
    );
}

#[test]
fn inject_headers_handles_lf_only_and_empty_and_separatorless() {
    // LF-only separator.
    let out = inject_headers(b"A: 1\n\nbody", &["X-T: 2".to_string()]);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "A: 1\r\nX-T: 2\r\n\r\nbody"
    );
    // Empty header list is a no-op.
    assert_eq!(
        inject_headers(b"A: 1\r\n\r\nb", &[]),
        b"A: 1\r\n\r\nb".to_vec()
    );
    // No separator: headers append (defensive).
    let out = inject_headers(b"no-separator", &["X-T: 2".to_string()]);
    assert_eq!(String::from_utf8(out).unwrap(), "no-separatorX-T: 2\r\n");
}

#[test]
fn own_header_requires_the_setting_a_default_key_and_private_material() {
    let store = test_store();
    // No default key: None.
    assert!(own_autocrypt_header_for_account(&store, "a1", "a1@x.com").is_none());

    seed_default_pgp_key(&store, true);
    let header = own_autocrypt_header_for_account(&store, "a1", "a1@x.com")
        .expect("full material + default on emits");
    // The fold may break before the keydata token; assert the parts, not a
    // single-line prefix.
    assert!(header.starts_with("Autocrypt: addr=a1@x.com; prefer-encrypt=mutual;"));
    assert!(header.contains("keydata="));
    for line in header.split("\r\n") {
        assert!(line.len() <= 72, "folded at ~72 cols: {line:?}");
    }

    // Setting off: None.
    crate::crypto_policy::set_policy_flag(&store, "a1", "send_autocrypt_headers", false).unwrap();
    assert!(own_autocrypt_header_for_account(&store, "a1", "a1@x.com").is_none());
    crate::crypto_policy::set_policy_flag(&store, "a1", "send_autocrypt_headers", true).unwrap();

    // Public-only row: not "our" key, None (advertising it would promise a
    // key we cannot sign/decrypt with).
    let store2 = test_store();
    seed_default_pgp_key(&store2, false);
    assert!(own_autocrypt_header_for_account(&store2, "a1", "a1@x.com").is_none());
}
