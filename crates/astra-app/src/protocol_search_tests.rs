// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the search family's intents: the exact JSON wire shape each crosses
//! the IPC in, and that each reads back identical.

use super::{Intent, SearchScope};

/// Asserts `intent` crosses as exactly `wire` and reads back identical.
fn round_trips(intent: &Intent, wire: &str) {
    assert_eq!(serde_json::to_string(intent).unwrap(), wire);
    let back: Intent = serde_json::from_str(wire).unwrap();
    assert_eq!(&back, intent);
}

#[test]
fn search_carries_the_query_or_clears_it() {
    round_trips(
        &Intent::Search {
            query: Some("from:ada monitor".to_owned()),
        },
        r#"{"search":{"query":"from:ada monitor"}}"#,
    );
    // No query clears: `None` crosses explicitly, the same shape the unified
    // view's absent account has, so clearing reads as one deliberate dispatch.
    round_trips(
        &Intent::Search { query: None },
        r#"{"search":{"query":null}}"#,
    );
}

#[test]
fn set_search_scope_carries_the_scope_as_a_plain_word() {
    round_trips(
        &Intent::SetSearchScope {
            scope: SearchScope::AllExceptTrash,
        },
        r#"{"set_search_scope":{"scope":"all_except_trash"}}"#,
    );
    round_trips(
        &Intent::SetSearchScope {
            scope: SearchScope::CurrentFolder,
        },
        r#"{"set_search_scope":{"scope":"current_folder"}}"#,
    );
    round_trips(
        &Intent::SetSearchScope {
            scope: SearchScope::CurrentAccount,
        },
        r#"{"set_search_scope":{"scope":"current_account"}}"#,
    );
    // The CamelCase spelling is not the wire, on either half.
    assert!(
        serde_json::from_str::<Intent>(r#"{"set_search_scope":{"scope":"AllExceptTrash"}}"#)
            .is_err()
    );
    assert!(
        serde_json::from_str::<Intent>(r#"{"Search":{"query":null}}"#).is_err(),
        "the variant's own name is renamed too"
    );
}
