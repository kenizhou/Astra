// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the search state: the exact JSON wire shape it crosses the IPC in,
//! the scope's wire words, and the snapshot's additive proof.

use engine_api::{AccountId, ThreadId};

use super::*;
use crate::rows::RowKey;
use crate::view::MailboxListSnapshot;

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// A thread-grouped result row, the shape a grouped search result arrives in.
fn thread_row() -> MessageRowView {
    MessageRowView {
        key: RowKey::Thread {
            account: account("work"),
            thread_id: ThreadId::try_from("th-7").unwrap(),
        },
        account_id: account("work"),
        sender_name: "Grace Hopper".to_owned(),
        sender_address: "grace@example.test".to_owned(),
        subject: "Re: Compiler design".to_owned(),
        sent_at: "2026-10-08T16:05:00Z".to_owned(),
        unread: false,
        flagged: true,
        has_attachments: false,
        thread_count: Some(3),
    }
}

#[test]
fn the_scopes_cross_as_plain_words() {
    assert_eq!(
        serde_json::to_string(&SearchScope::AllExceptTrash).unwrap(),
        r#""all_except_trash""#
    );
    assert_eq!(
        serde_json::to_string(&SearchScope::CurrentFolder).unwrap(),
        r#""current_folder""#
    );
    assert_eq!(
        serde_json::to_string(&SearchScope::CurrentAccount).unwrap(),
        r#""current_account""#
    );
    assert_eq!(
        serde_json::from_str::<SearchScope>(r#""current_account""#).unwrap(),
        SearchScope::CurrentAccount
    );
    // The CamelCase spelling is not the wire.
    assert!(serde_json::from_str::<SearchScope>(r#""AllExceptTrash""#).is_err());
    // The default scope is everything but Trash.
    assert_eq!(SearchScope::default(), SearchScope::AllExceptTrash);
}

#[test]
fn a_search_state_crosses_with_the_query_the_scope_the_rows_and_the_depth() {
    let state = SearchState {
        query: "from:ada monitor".to_owned(),
        scope: SearchScope::CurrentFolder,
        rows: vec![thread_row()],
        depth_months: 3,
    };

    let json = serde_json::to_string(&state).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"query":"from:ada monitor","scope":"current_folder","rows":["#,
            r#"{"key":{"thread":{"account":"work","thread_id":"th-7"}},"account_id":"work","#,
            r#""sender_name":"Grace Hopper","sender_address":"grace@example.test","#,
            r#""subject":"Re: Compiler design","sent_at":"2026-10-08T16:05:00Z","#,
            r#""unread":false,"flagged":true,"has_attachments":false,"thread_count":3}],"#,
            r#""depth_months":3}"#
        )
    );

    let back: SearchState = serde_json::from_str(&json).unwrap();
    assert_eq!(back, state);
}

#[test]
fn the_snapshot_carries_the_search_state_additively() {
    // Present, the search state crosses as one more field on the snapshot,
    // after the list fields, without touching any of them.
    let snapshot = MailboxListSnapshot {
        search: Some(SearchState {
            query: "monitor".to_owned(),
            scope: SearchScope::AllExceptTrash,
            rows: Vec::new(),
            depth_months: 6,
        }),
        ..MailboxListSnapshot::default()
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"account_folders":[],"search":{"query":"monitor","#,
            r#""scope":"all_except_trash","rows":[],"depth_months":6}}"#
        )
    );

    let back: MailboxListSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back.search, snapshot.search);
}

#[test]
fn a_wire_with_no_search_field_parses_to_the_unsearched_snapshot() {
    // The additive direction: an old emitter's JSON, carrying no search field
    // at all, parses to the unsearched snapshot rather than failing.
    let back: MailboxListSnapshot = serde_json::from_str(r#"{"account_folders":[]}"#).unwrap();
    assert_eq!(back.search, None);
}
