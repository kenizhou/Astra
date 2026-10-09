// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the protocol: the exact JSON wire shape every intent, folder intent, view
//! mode, and surface crosses the IPC in, and that each reads back identical.

use astra_composer::{Block, ComposerDocument, DraftBlobHandle, InlineContent, Paragraph, TextRun};
use engine_api::AccountId;

use super::{
    ComposeIntent, ComposerBlob, CompositionId, DraftsIntent, FolderIntent, Intent, OutboxIntent,
    Surface, ViewMode,
};
use crate::reference::{FolderRef, MessageRef, QueuedRef, RowRef, ThreadRef};

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

/// A composition id, the way every drafts test names one.
fn composition(value: &str) -> CompositionId {
    CompositionId::new(value).unwrap()
}

/// A one-paragraph document carrying `text`, the smallest document a save can hold.
fn document(text: &str) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![Block::Paragraph(Paragraph {
            content: vec![InlineContent::Text(TextRun {
                text: text.to_owned(),
                bold: false,
                italic: false,
                underline: false,
                font_size: None,
                color: None,
                highlight: None,
                link: None,
            })],
        })],
        attachments: Vec::new(),
    }
}

#[test]
fn a_drafts_save_carries_the_composition_the_document_and_the_close_flag() {
    round_trips(
        &Intent::Drafts(DraftsIntent::Save {
            composition: composition("compose-1"),
            from: account("work"),
            to: "you@test.local".to_owned(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Later".to_owned(),
            document: document("Half written"),
            then_close: false,
        }),
        concat!(
            r#"{"drafts":{"save":{"composition":"compose-1","from":"work","#,
            r#""to":"you@test.local","cc":"","bcc":"","subject":"Later","#,
            r#""document":{"blocks":[{"Paragraph":{"content":"#,
            r#"[{"Text":{"text":"Half written","bold":false,"italic":false,"underline":false,"#,
            r#""font_size":null,"color":null,"highlight":null,"link":null}}]}}],"#,
            r#""attachments":[]},"then_close":false}}}"#
        ),
    );
    // The leaving flag is pinned the other way round too: a save that closes the
    // composer behind it is the same wire with `then_close` true.
    round_trips(
        &Intent::Drafts(DraftsIntent::Save {
            composition: composition("compose-1"),
            from: account("work"),
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: String::new(),
            document: document("On my way out"),
            then_close: true,
        }),
        concat!(
            r#"{"drafts":{"save":{"composition":"compose-1","from":"work","#,
            r#""to":"","cc":"","bcc":"","subject":"","#,
            r#""document":{"blocks":[{"Paragraph":{"content":"#,
            r#"[{"Text":{"text":"On my way out","bold":false,"italic":false,"underline":false,"#,
            r#""font_size":null,"color":null,"highlight":null,"link":null}}]}}],"#,
            r#""attachments":[]},"then_close":true}}}"#
        ),
    );
}

#[test]
fn a_drafts_discard_and_close_name_the_composition() {
    round_trips(
        &Intent::Drafts(DraftsIntent::Discard {
            composition: composition("compose-1"),
        }),
        r#"{"drafts":{"discard":{"composition":"compose-1"}}}"#,
    );
    round_trips(
        &Intent::Drafts(DraftsIntent::Close {
            composition: composition("compose-1"),
        }),
        r#"{"drafts":{"close":{"composition":"compose-1"}}}"#,
    );
}

#[test]
fn a_blank_composition_id_is_refused_on_the_wire() {
    // The constructor rejects blanks, so a blank id would merge every composer's
    // draft into one entry and each save would supersede a different composer's copy.
    assert!(CompositionId::new("").is_none());
    assert!(CompositionId::new("   ").is_none());
    assert!(CompositionId::new("compose-1").is_some());
    // And so the wire refuses one, exactly as a reference refuses a blank key.
    let blank = r#"{"drafts":{"close":{"composition":""}}}"#;
    assert!(serde_json::from_str::<Intent>(blank).is_err());
    let spaces = r#"{"drafts":{"close":{"composition":"   "}}}"#;
    assert!(serde_json::from_str::<Intent>(spaces).is_err());
    assert_eq!(
        composition("compose-1").as_str(),
        "compose-1",
        "the id reads back as the host gave it"
    );
}

#[test]
fn draft_status_crosses_as_a_renamed_word() {
    assert_eq!(
        serde_json::to_string(&Surface::DraftStatus).unwrap(),
        r#""draft_status""#
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""draft_status""#).unwrap(),
        Surface::DraftStatus
    );
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

#[test]
fn submit_rich_mail_carries_the_recipients_the_document_and_the_blobs() {
    round_trips(
        &Intent::SubmitRichMail {
            from: account("work"),
            to: "you@test.local".to_owned(),
            cc: String::new(),
            bcc: String::new(),
            subject: "On its way".to_owned(),
            document: document("Sent words"),
            blobs: Vec::new(),
        },
        concat!(
            r#"{"submit_rich_mail":{"from":"work","to":"you@test.local","#,
            r#""cc":"","bcc":"","subject":"On its way","#,
            r#""document":{"blocks":[{"Paragraph":{"content":"#,
            r#"[{"Text":{"text":"Sent words","bold":false,"italic":false,"underline":false,"#,
            r#""font_size":null,"color":null,"highlight":null,"link":null}}]}}],"#,
            r#""attachments":[]},"blobs":[]}}"#
        ),
    );
    // A submit with host-resolved attachment bytes carries them beside the document.
    round_trips(
        &Intent::SubmitRichMail {
            from: account("work"),
            to: "you@test.local".to_owned(),
            cc: String::new(),
            bcc: String::new(),
            subject: String::new(),
            document: document("With a file"),
            blobs: vec![ComposerBlob::new(
                DraftBlobHandle::new("blob-1").unwrap(),
                vec![1, 2, 3],
            )],
        },
        concat!(
            r#"{"submit_rich_mail":{"from":"work","to":"you@test.local","#,
            r#""cc":"","bcc":"","subject":"","#,
            r#""document":{"blocks":[{"Paragraph":{"content":"#,
            r#"[{"Text":{"text":"With a file","bold":false,"italic":false,"underline":false,"#,
            r#""font_size":null,"color":null,"highlight":null,"link":null}}]}}],"#,
            r#""attachments":[]},"blobs":[{"handle":"blob-1","bytes":[1,2,3]}]}}"#
        ),
    );
}

#[test]
fn the_outbox_family_crosses_with_the_queued_reference() {
    round_trips(&Intent::Outbox(OutboxIntent::Show), r#"{"outbox":"show"}"#);
    round_trips(
        &Intent::Outbox(OutboxIntent::Cancel {
            queued: queued("work", 3),
        }),
        r#"{"outbox":{"cancel":{"queued":{"account":"work","op":3}}}}"#,
    );
    round_trips(
        &Intent::Outbox(OutboxIntent::SendNow {
            queued: queued("home", 1),
        }),
        r#"{"outbox":{"send_now":{"queued":{"account":"home","op":1}}}}"#,
    );
    round_trips(
        &Intent::Outbox(OutboxIntent::ConfirmSent {
            queued: queued("work", 7),
        }),
        r#"{"outbox":{"confirm_sent":{"queued":{"account":"work","op":7}}}}"#,
    );
    round_trips(
        &Intent::Outbox(OutboxIntent::ConfirmNotSent {
            queued: queued("home", 2),
        }),
        r#"{"outbox":{"confirm_not_sent":{"queued":{"account":"home","op":2}}}}"#,
    );
    // The CamelCase spelling is not the wire.
    assert!(serde_json::from_str::<Intent>(r#"{"outbox":"Show"}"#).is_err());
}

#[test]
fn a_malformed_queued_reference_is_refused_on_the_wire() {
    // The account half validates on the way in, exactly as a folder or message reference:
    // a blank id cannot name the outbox an action routes to.
    let cancel = r#"{"outbox":{"cancel":{"queued":{"account":"","op":3}}}}"#;
    assert!(serde_json::from_str::<Intent>(cancel).is_err());
    assert!(QueuedRef::from_parts("", 3).is_none());
    assert_eq!(
        queued("work", 3).op.get(),
        3,
        "the op id reads back as the store assigned it"
    );
}

#[test]
fn sending_crosses_as_a_renamed_word() {
    assert_eq!(
        serde_json::to_string(&Surface::Sending).unwrap(),
        r#""sending""#
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""sending""#).unwrap(),
        Surface::Sending
    );
}

#[test]
fn the_compose_family_names_the_message_each_variant_answers() {
    round_trips(
        &Intent::Compose(ComposeIntent::Reply {
            message: MessageRef::from_parts("work", "m1".to_owned()).unwrap(),
        }),
        r#"{"compose":{"reply":{"message":{"account":"work","key":"m1"}}}}"#,
    );
    round_trips(
        &Intent::Compose(ComposeIntent::ReplyAll {
            message: MessageRef::from_parts("work", "m1".to_owned()).unwrap(),
        }),
        r#"{"compose":{"reply_all":{"message":{"account":"work","key":"m1"}}}}"#,
    );
    round_trips(
        &Intent::Compose(ComposeIntent::Forward {
            message: MessageRef::from_parts("home", "jmap:e2".to_owned()).unwrap(),
        }),
        r#"{"compose":{"forward":{"message":{"account":"home","key":"jmap:e2"}}}}"#,
    );
    // The CamelCase spelling is not the wire.
    assert!(serde_json::from_str::<Intent>(r#"{"compose":"Reply"}"#).is_err());
}

#[test]
fn compose_request_crosses_as_a_renamed_word() {
    assert_eq!(
        serde_json::to_string(&Surface::ComposeRequest).unwrap(),
        r#""compose_request""#
    );
    assert_eq!(
        serde_json::from_str::<Surface>(r#""compose_request""#).unwrap(),
        Surface::ComposeRequest
    );
}

/// A queued-send reference, the way every outbox test names one.
fn queued(account_value: &str, op: u64) -> QueuedRef {
    QueuedRef::from_parts(account_value, op).unwrap()
}
