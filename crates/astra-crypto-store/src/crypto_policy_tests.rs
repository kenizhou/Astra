// SPDX-License-Identifier: GPL-3.0-only
//! Reds for the per-account crypto policy flags.

use super::*;
use crate::store::CryptoStore;

fn test_store() -> CryptoStore {
    let store = CryptoStore::open_in_memory().expect("open in-memory store");
    store.register_account("a1", "a1@x.com").unwrap();
    store
}

#[test]
fn both_flags_default_on_when_unset_or_absent() {
    let store = test_store();
    assert!(get_send_autocrypt_headers(&store, "a1").unwrap());
    assert!(get_protect_subject(&store, "a1").unwrap());
    // An unknown account reads the defaults too (no row, no error).
    assert!(get_send_autocrypt_headers(&store, "ghost").unwrap());
    assert!(get_protect_subject(&store, "ghost").unwrap());
}

#[test]
fn set_then_get_round_trips_and_preserves_siblings() {
    let store = test_store();
    set_policy_flag(&store, "a1", "protect_subject", false).unwrap();
    assert!(!get_protect_subject(&store, "a1").unwrap());
    assert!(
        get_send_autocrypt_headers(&store, "a1").unwrap(),
        "the sibling flag is untouched"
    );
    set_policy_flag(&store, "a1", "send_autocrypt_headers", false).unwrap();
    assert!(!get_send_autocrypt_headers(&store, "a1").unwrap());
    assert!(!get_protect_subject(&store, "a1").unwrap());

    // Setting one back must not clobber the other.
    set_policy_flag(&store, "a1", "protect_subject", true).unwrap();
    assert!(get_protect_subject(&store, "a1").unwrap());
    assert!(!get_send_autocrypt_headers(&store, "a1").unwrap());
}

#[test]
fn malformed_policy_json_reads_the_defaults() {
    let store = test_store();
    store
        .conn()
        .execute(
            "UPDATE accounts SET crypto_policy_json = 'not json' WHERE id = 'a1'",
            [],
        )
        .unwrap();
    assert!(get_protect_subject(&store, "a1").unwrap());
    assert!(get_send_autocrypt_headers(&store, "a1").unwrap());
}
