// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the typed references: the single construction point validating the halves
//! of each, and the wire form every reference crosses in.

use super::{CalendarRef, EventRef, FolderRef, MessageRef, QueuedRef, RowRef, ThreadRef};

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
fn a_message_reference_needs_both_halves_well_formed() {
    // A provider key is validated like the account: a blank one yields `None`, so a
    // malformed half never rides an intent into the core.
    let reference = MessageRef::from_parts("acct-1", "imap:v1:u1@INBOX".to_owned()).unwrap();
    assert_eq!(reference.account.as_str(), "acct-1");
    assert_eq!(reference.key.as_str(), "imap:v1:u1@INBOX");
    assert!(MessageRef::from_parts("", "imap:v1:u1@INBOX".to_owned()).is_none());
    assert!(MessageRef::from_parts("acct-1", String::new()).is_none());
}

#[test]
fn a_thread_reference_needs_a_well_formed_account_half() {
    // The thread half is a plain identifier and is not validated, exactly as a folder
    // key is not: every value a real row carried travels.
    let reference = ThreadRef::from_parts("acct-1", "th-7".to_owned()).unwrap();
    assert_eq!(reference.account.as_str(), "acct-1");
    assert_eq!(reference.thread_id, "th-7");
    assert!(ThreadRef::from_parts("", "th-7".to_owned()).is_none());
    assert!(ThreadRef::from_parts("acct-1", String::new()).is_some());
}

#[test]
fn a_row_reference_crosses_as_its_tagged_half() {
    // Externally tagged, snake_case: the variant names the half that follows.
    let message =
        RowRef::Message(MessageRef::from_parts("acct-1", "imap:v1:u1".to_owned()).unwrap());
    let json = serde_json::to_string(&message).unwrap();
    assert_eq!(
        json,
        r#"{"message":{"account":"acct-1","key":"imap:v1:u1"}}"#
    );
    let back: RowRef = serde_json::from_str(&json).unwrap();
    assert_eq!(back, message);

    let thread = RowRef::Thread(ThreadRef::from_parts("acct-1", "th-7".to_owned()).unwrap());
    let json = serde_json::to_string(&thread).unwrap();
    assert_eq!(
        json,
        r#"{"thread":{"account":"acct-1","thread_id":"th-7"}}"#
    );
    let back: RowRef = serde_json::from_str(&json).unwrap();
    assert_eq!(back, thread);
}

#[test]
fn a_row_reference_names_its_account() {
    // The one accessor a write routes by: whichever half a row carries, the account it
    // belongs to is one call away.
    let message =
        RowRef::Message(MessageRef::from_parts("acct-1", "imap:v1:u1".to_owned()).unwrap());
    let thread = RowRef::Thread(ThreadRef::from_parts("acct-2", "th-7".to_owned()).unwrap());
    assert_eq!(message.account().as_str(), "acct-1");
    assert_eq!(thread.account().as_str(), "acct-2");
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

#[test]
fn an_event_reference_needs_both_halves_well_formed() {
    // A provider key is validated like the account, exactly as a message reference: a
    // blank half never rides a calendar intent into the core.
    let reference =
        EventRef::from_parts("acct-1", "/dav/cal/acct-1/default/evt-1.ics".to_owned()).unwrap();
    assert_eq!(reference.account.as_str(), "acct-1");
    assert_eq!(reference.key.as_str(), "/dav/cal/acct-1/default/evt-1.ics");
    assert!(EventRef::from_parts("", "/dav/evt-1.ics".to_owned()).is_none());
    assert!(EventRef::from_parts("acct-1", String::new()).is_none());
}

#[test]
fn a_calendar_reference_needs_both_halves_well_formed() {
    // The calendar half routes the write (two accounts can each hold a calendar whose
    // key is `default`), so it validates exactly as the event half does.
    let reference = CalendarRef::from_parts("acct-1", "default".to_owned()).unwrap();
    assert_eq!(reference.account.as_str(), "acct-1");
    assert_eq!(reference.key.as_str(), "default");
    assert!(CalendarRef::from_parts("", "default".to_owned()).is_none());
    assert!(CalendarRef::from_parts("acct-1", String::new()).is_none());
}

#[test]
fn an_event_and_a_calendar_reference_cross_as_their_two_halves() {
    let event =
        EventRef::from_parts("acct-1", "/dav/cal/acct-1/default/evt-1.ics".to_owned()).unwrap();
    let json = serde_json::to_string(&event).unwrap();
    assert_eq!(
        json,
        r#"{"account":"acct-1","key":"/dav/cal/acct-1/default/evt-1.ics"}"#
    );
    let back: EventRef = serde_json::from_str(&json).unwrap();
    assert_eq!(back, event);

    let calendar = CalendarRef::from_parts("acct-1", "default".to_owned()).unwrap();
    let json = serde_json::to_string(&calendar).unwrap();
    assert_eq!(json, r#"{"account":"acct-1","key":"default"}"#);
    let back: CalendarRef = serde_json::from_str(&json).unwrap();
    assert_eq!(back, calendar);
}

#[test]
fn a_queued_reference_crosses_as_its_account_and_op_number() {
    // The op half is the store-assigned integer, bare on the wire: a `PendingOpId`
    // serialises as the number it wraps, so the reference crosses like every other
    // reference's two halves.
    let reference = QueuedRef::from_parts("acct-1", 3).unwrap();
    assert_eq!(reference.op.get(), 3);
    let json = serde_json::to_string(&reference).unwrap();
    assert_eq!(json, r#"{"account":"acct-1","op":3}"#);
    let back: QueuedRef = serde_json::from_str(&json).unwrap();
    assert_eq!(back, reference);
}
