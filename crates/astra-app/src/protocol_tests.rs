// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the protocol: the exact JSON wire shape every intent, folder intent, view
//! mode, and surface crosses the IPC in, and that each reads back identical.

use engine_api::AccountId;

use super::{FolderIntent, Intent, Surface, ViewMode};
use crate::reference::FolderRef;

/// A well-formed account id, the way every test names one.
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

/// Asserts `intent` crosses as exactly `wire` and reads back identical.
fn round_trips(intent: &Intent, wire: &str) {
    assert_eq!(serde_json::to_string(intent).unwrap(), wire);
    let back: Intent = serde_json::from_str(wire).unwrap();
    assert_eq!(&back, intent);
}

#[test]
fn refresh_mail_crosses_as_a_bare_renamed_word() {
    round_trips(&Intent::RefreshMail, r#""refresh_mail""#);
    // The CamelCase spelling is not the wire: a host must send the renamed word.
    assert!(serde_json::from_str::<Intent>(r#""RefreshMail""#).is_err());
}

#[test]
fn select_account_carries_the_account_or_the_unified_view() {
    round_trips(
        &Intent::SelectAccount(Some(account("work"))),
        r#"{"select_account":"work"}"#,
    );
    round_trips(&Intent::SelectAccount(None), r#"{"select_account":null}"#);
}

#[test]
fn set_account_expanded_crosses_as_a_tagged_record() {
    round_trips(
        &Intent::SetAccountExpanded {
            account: account("work"),
            expanded: true,
        },
        r#"{"set_account_expanded":{"account":"work","expanded":true}}"#,
    );
}

#[test]
fn set_folder_expanded_carries_the_folder_reference() {
    round_trips(
        &Intent::SetFolderExpanded {
            folder: FolderRef::from_parts("work", "Clients".to_owned()).unwrap(),
            expanded: false,
        },
        r#"{"set_folder_expanded":{"folder":{"account":"work","key":"Clients"},"expanded":false}}"#,
    );
}

#[test]
fn select_folder_names_the_folder_by_reference() {
    round_trips(
        &Intent::SelectFolder {
            folder: FolderRef::from_parts("home", "INBOX".to_owned()).unwrap(),
        },
        r#"{"select_folder":{"folder":{"account":"home","key":"INBOX"}}}"#,
    );
}

#[test]
fn set_view_mode_carries_the_mode_as_a_plain_word() {
    round_trips(
        &Intent::SetViewMode(ViewMode::Threaded),
        r#"{"set_view_mode":"threaded"}"#,
    );
    round_trips(
        &Intent::SetViewMode(ViewMode::Flat),
        r#"{"set_view_mode":"flat"}"#,
    );
}

#[test]
fn show_more_crosses_as_a_bare_renamed_word() {
    round_trips(&Intent::ShowMore, r#""show_more""#);
}

#[test]
fn folders_carries_the_folder_intent() {
    // A top-level create omits `parent` entirely; a nested one carries it.
    round_trips(
        &Intent::Folders(FolderIntent::Create {
            account: account("work"),
            parent: None,
            name: "Archive".to_owned(),
        }),
        r#"{"folders":{"create":{"account":"work","name":"Archive"}}}"#,
    );
    round_trips(
        &Intent::Folders(FolderIntent::Create {
            account: account("work"),
            parent: Some("INBOX".to_owned()),
            name: "Archive".to_owned(),
        }),
        r#"{"folders":{"create":{"account":"work","parent":"INBOX","name":"Archive"}}}"#,
    );
}

#[test]
fn surfaces_cross_as_renamed_words() {
    assert_eq!(
        serde_json::to_string(&Surface::MailboxList).unwrap(),
        r#""mailbox_list""#
    );
    assert_eq!(
        serde_json::to_string(&Surface::SyncProgress).unwrap(),
        r#""sync_progress""#
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""mailbox_list""#).unwrap(),
        Surface::MailboxList
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""sync_progress""#).unwrap(),
        Surface::SyncProgress
    );
}

#[test]
fn view_modes_cross_as_plain_words() {
    assert_eq!(serde_json::to_string(&ViewMode::Flat).unwrap(), r#""flat""#);
    assert_eq!(
        serde_json::to_string(&ViewMode::Threaded).unwrap(),
        r#""threaded""#
    );
    assert_eq!(
        serde_json::from_str::<ViewMode>(r#""threaded""#).unwrap(),
        ViewMode::Threaded
    );
}

#[test]
fn a_malformed_account_id_is_refused_on_the_wire() {
    // The account half of a reference validates on the way in, so a blank id cannot ride
    // an intent into the core and route a write to an account that does not exist.
    let folder = r#"{"select_folder":{"folder":{"account":"","key":"INBOX"}}}"#;
    assert!(serde_json::from_str::<Intent>(folder).is_err());
    assert!(serde_json::from_str::<FolderRef>(r#"{"account":"","key":"INBOX"}"#).is_err());
}
