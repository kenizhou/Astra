// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the protocol: the exact JSON wire shape every intent, folder intent, view
//! mode, and surface crosses the IPC in, and that each reads back identical.

use engine_api::AccountId;

use super::{FolderIntent, Intent, Surface, ViewMode};
use crate::reference::{FolderRef, MessageRef, RowRef, ThreadRef};

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
fn mark_read_carries_the_row_and_the_new_state() {
    round_trips(
        &Intent::MarkRead {
            row: RowRef::Message(MessageRef::from_parts("work", "imap:v1:u1".to_owned()).unwrap()),
            read: true,
        },
        r#"{"mark_read":{"row":{"message":{"account":"work","key":"imap:v1:u1"}},"read":true}}"#,
    );
    round_trips(
        &Intent::MarkRead {
            row: RowRef::Thread(ThreadRef::from_parts("work", "th-7".to_owned()).unwrap()),
            read: false,
        },
        r#"{"mark_read":{"row":{"thread":{"account":"work","thread_id":"th-7"}},"read":false}}"#,
    );
}

#[test]
fn set_flagged_carries_the_row_and_the_new_state() {
    round_trips(
        &Intent::SetFlagged {
            row: RowRef::Message(MessageRef::from_parts("home", "jmap:e2".to_owned()).unwrap()),
            flagged: true,
        },
        r#"{"set_flagged":{"row":{"message":{"account":"home","key":"jmap:e2"}},"flagged":true}}"#,
    );
    round_trips(
        &Intent::SetFlagged {
            row: RowRef::Thread(ThreadRef::from_parts("home", "th-7".to_owned()).unwrap()),
            flagged: false,
        },
        r#"{"set_flagged":{"row":{"thread":{"account":"home","thread_id":"th-7"}},"flagged":false}}"#,
    );
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
fn open_message_carries_the_message_reference() {
    round_trips(
        &Intent::OpenMessage {
            message: MessageRef::from_parts("work", "imap:v1:u1".to_owned()).unwrap(),
        },
        r#"{"open_message":{"message":{"account":"work","key":"imap:v1:u1"}}}"#,
    );
}

#[test]
fn close_reading_crosses_as_a_bare_renamed_word() {
    round_trips(&Intent::CloseReading, r#""close_reading""#);
    // The CamelCase spelling is not the wire.
    assert!(serde_json::from_str::<Intent>(r#""CloseReading""#).is_err());
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
        serde_json::to_string(&Surface::Reading).unwrap(),
        r#""reading""#
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""mailbox_list""#).unwrap(),
        Surface::MailboxList
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""sync_progress""#).unwrap(),
        Surface::SyncProgress
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""reading""#).unwrap(),
        Surface::Reading
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

#[test]
fn a_malformed_row_reference_is_refused_on_the_wire() {
    // The message row's key half validates too, exactly as the account does: a blank key
    // cannot name a message, and a thread row's account is the half that routes.
    let blank_account = r#"{"mark_read":{"row":{"message":{"account":"","key":"x"}},"read":true}}"#;
    assert!(serde_json::from_str::<Intent>(blank_account).is_err());
    let blank_key =
        r#"{"set_flagged":{"row":{"message":{"account":"work","key":""}},"flagged":true}}"#;
    assert!(serde_json::from_str::<Intent>(blank_key).is_err());
    let thread =
        r#"{"mark_read":{"row":{"thread":{"account":"","thread_id":"th-7"}},"read":true}}"#;
    assert!(serde_json::from_str::<Intent>(thread).is_err());
}
