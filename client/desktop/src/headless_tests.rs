// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the headless slice's pure pieces: the tree assertion and the poll that
//! waits on it. The dial, add, and restart flows themselves are the live run
//! `scripts/dev/slice-check.sh` owns; what stays here is everything that decides
//! without a server.

use std::time::Duration;

use astra_viewmodel::{AccountFolderRow, FolderRow};
use engine_api::{AccountId, MailboxId};

use super::*;

/// An account id, as the snapshots carry one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// A mailbox id, as the folder rows key themselves.
fn mailbox(value: &str) -> MailboxId {
    MailboxId::try_from(value).unwrap()
}

/// A top-level folder row named `name`, unread at zero.
fn row(name: &str) -> FolderRow {
    FolderRow {
        key: mailbox(name),
        name: name.to_owned(),
        role: None,
        unread: 0,
        parent: None,
        depth: 0,
        has_children: false,
        expanded: false,
    }
}

/// A one-account snapshot over the named folders.
fn snapshot(folders: &[&str]) -> MailboxListSnapshot {
    MailboxListSnapshot {
        account_folders: vec![AccountFolderRow {
            account_id: account("slice@example.test"),
            display_name: "slice@example.test".to_owned(),
            expanded: true,
            folders: folders.iter().map(|name| row(name)).collect(),
        }],
    }
}

#[test]
fn the_tree_diff_is_empty_when_every_expected_folder_is_there() {
    let complete = snapshot(&["INBOX", "Sent", "Projects"]);
    assert!(tree_diff(&complete, &["INBOX", "Projects"]).is_empty());
}

#[test]
fn the_tree_diff_names_every_missing_folder() {
    let partial = snapshot(&["INBOX"]);
    assert_eq!(
        tree_diff(&partial, &["INBOX", "Projects"]),
        vec!["missing folder: Projects".to_owned()],
    );
    let empty = snapshot(&[]);
    assert_eq!(
        tree_diff(&empty, &["INBOX", "Projects"]),
        vec![
            "missing folder: INBOX".to_owned(),
            "missing folder: Projects".to_owned(),
        ],
    );
}

#[test]
fn the_tree_diff_counts_a_folder_held_by_any_account() {
    let split = MailboxListSnapshot {
        account_folders: vec![
            AccountFolderRow {
                account_id: account("one@example.test"),
                display_name: "one@example.test".to_owned(),
                expanded: true,
                folders: vec![row("INBOX")],
            },
            AccountFolderRow {
                account_id: account("two@example.test"),
                display_name: "two@example.test".to_owned(),
                expanded: true,
                folders: vec![row("Projects")],
            },
        ],
    };
    assert!(tree_diff(&split, &["INBOX", "Projects"]).is_empty());
}

#[test]
fn the_poll_returns_once_the_probe_holds_the_tree() {
    let complete = snapshot(&["INBOX", "Projects"]);
    let seen = complete.clone();
    let mut calls = 0;
    let outcome = poll_until_complete(Duration::from_secs(5), Duration::from_millis(1), || {
        calls += 1;
        if calls < 3 {
            snapshot(&["INBOX"])
        } else {
            seen.clone()
        }
    });
    let snapshot = outcome.expect("the poll completes once the probe holds the tree");
    assert_eq!(snapshot, complete);
    assert_eq!(calls, 3);
}

#[test]
fn the_poll_times_out_with_the_last_snapshot_and_its_diff() {
    let mut calls = 0;
    let outcome = poll_until_complete(Duration::from_millis(30), Duration::from_millis(1), || {
        calls += 1;
        snapshot(&["INBOX"])
    });
    let (_, diff) = outcome.expect_err("the probe never holds the tree");
    assert_eq!(diff, vec!["missing folder: Projects".to_owned()]);
    assert!(calls > 1, "the poll kept pulling until the deadline");
}
