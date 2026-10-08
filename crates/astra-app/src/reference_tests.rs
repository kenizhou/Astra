// SPDX-License-Identifier: GPL-3.0-only
//! Tests for folder references: the single construction point validating its halves, and
//! the wire form both halves cross in.

use super::FolderRef;

#[test]
fn from_parts_needs_a_well_formed_account_half() {
    // A well-formed account id builds a reference carrying both halves; a blank
    // (malformed) one yields `None`, so the boundary drops it rather than carrying half a
    // reference that could route into the wrong account.
    let reference = FolderRef::from_parts("acct-1", "INBOX".to_owned()).unwrap();
    assert_eq!(reference.account.as_str(), "acct-1");
    assert_eq!(reference.key, "INBOX");
    assert!(FolderRef::from_parts("", "INBOX".to_owned()).is_none());
}

#[test]
fn the_folder_key_is_a_plain_identifier_and_is_not_validated() {
    // Unlike the account half, `key` is the snapshot's own row key: every value a real row
    // carries travels, so there is nothing to validate it against here.
    let reference = FolderRef::from_parts("acct-1", String::new()).unwrap();
    assert_eq!(reference.key, "");
}

#[test]
fn a_folder_reference_crosses_as_its_two_halves() {
    let reference = FolderRef::from_parts("acct-1", "INBOX".to_owned()).unwrap();
    let json = serde_json::to_string(&reference).unwrap();
    assert_eq!(json, r#"{"account":"acct-1","key":"INBOX"}"#);
    let back: FolderRef = serde_json::from_str(&json).unwrap();
    assert_eq!(back, reference);
}
