// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the message-list half of the dispatch loop: the rows a folder selection
//! builds, the window it grows, the view modes it re-projects, and the read/flag writes
//! that flow through the engine's outbox. The fixtures live in `dispatch_fakes`, the
//! crate-root test module every dispatch-shaped test shares.

use std::sync::{Arc, Mutex};

use crate::{
    dispatch_fakes as fakes,
    reference::{FolderRef, MessageRef, RowRef, ThreadRef},
    Intent, Surface, ViewMode,
};

use fakes::{account, app, delivered_at, folder, inbox, mail, read, threaded};

/// The mailbox every list fixture selects: one account, an Inbox and an Archive.
async fn listed_mailbox(
    surfaces: &Arc<Mutex<Vec<Surface>>>,
    messages: Vec<engine_api::Message>,
) -> crate::App<fakes::FakeProvider> {
    let provider =
        fakes::FakeProvider::with_mail(vec![inbox(), folder("archive", "Archive")], messages);
    let app = app(vec![account("work", provider)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app
}

/// Selects the work Inbox, the folder every list test reads.
async fn open_inbox(app: &crate::App<fakes::FakeProvider>) {
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn select_folder_lists_that_folders_rows_newest_first() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = listed_mailbox(
        &surfaces,
        vec![
            delivered_at(mail("m1", "inbox", "Kickoff"), 8, 0),
            delivered_at(mail("m2", "inbox", "Follow-up"), 8, 5),
            delivered_at(mail("m3", "archive", "Filed away"), 9, 0),
        ],
    )
    .await;

    open_inbox(&app).await;

    let snapshot = app.mailbox_list();
    assert_eq!(
        fakes::row_keys(&snapshot),
        ["m2", "m1"],
        "only the folder's mail, newest first"
    );
    let newest = fakes::row(&snapshot, "m2");
    assert_eq!(newest.account_id.as_str(), "work");
    assert_eq!(newest.sender_name, "Ada Lovelace");
    assert_eq!(newest.sender_address, "ada@example.test");
    assert_eq!(newest.subject, "Follow-up");
    assert_eq!(newest.sent_at, "2026-10-09T08:05:00Z");
    assert!(newest.unread, "no $seen keyword arrived");
    assert!(!newest.flagged);
    assert!(!newest.has_attachments);
    assert_eq!(newest.thread_count, None, "a flat row counts nothing");
    assert_eq!(snapshot.visible_limit, u32::try_from(crate::PAGE).unwrap());
    assert_eq!(snapshot.view_mode, ViewMode::Flat);
    assert_eq!(snapshot.selection, None, "nothing is selected yet");

    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "archive".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    assert_eq!(
        fakes::row_keys(&app.mailbox_list()),
        ["m3"],
        "the other folder's mail lists instead"
    );
}

#[tokio::test]
async fn show_more_grows_the_visible_row_window() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let messages: Vec<engine_api::Message> = (0..crate::PAGE + 10)
        .map(|index| {
            delivered_at(
                mail(
                    &format!("m{index:03}"),
                    "inbox",
                    &format!("Subject {index}"),
                ),
                8 + u8::try_from(index / 60).unwrap(),
                u8::try_from(index % 60).unwrap(),
            )
        })
        .collect();
    let app = listed_mailbox(&surfaces, messages).await;

    open_inbox(&app).await;
    assert_eq!(
        app.mailbox_list().rows.len(),
        crate::PAGE,
        "the first page bounds the list"
    );

    app.dispatch(Intent::ShowMore).await.unwrap();
    let snapshot = app.mailbox_list();
    assert_eq!(
        snapshot.rows.len(),
        crate::PAGE + 10,
        "one more page reaches every row"
    );
    assert_eq!(
        snapshot.visible_limit,
        u32::try_from(crate::PAGE * 2).unwrap()
    );
}

#[tokio::test]
async fn the_threaded_view_groups_under_the_thread_root_and_counts() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = listed_mailbox(
        &surfaces,
        vec![
            threaded(
                delivered_at(mail("m-old", "inbox", "Re: Launch"), 8, 0),
                "th-1",
            ),
            threaded(
                delivered_at(mail("m-new", "inbox", "Re: Launch"), 8, 30),
                "th-1",
            ),
            delivered_at(mail("m-solo", "inbox", "Standalone"), 8, 15),
        ],
    )
    .await;

    open_inbox(&app).await;
    app.dispatch(Intent::SetViewMode(ViewMode::Threaded))
        .await
        .unwrap();

    let snapshot = app.mailbox_list();
    assert_eq!(
        fakes::row_keys(&snapshot),
        ["th-1", "m-solo"],
        "the conversation with the newest in-folder activity leads; a lone message is its \
         own row"
    );
    let conversation = fakes::row(&snapshot, "th-1");
    assert_eq!(conversation.thread_count, Some(2), "both members counted");
    assert_eq!(conversation.subject, "Re: Launch");
    assert_eq!(
        conversation.sent_at, "2026-10-09T08:30:00Z",
        "the conversation's face is its newest in-folder member"
    );
    assert!(conversation.unread, "a member is unread");
    let solo = fakes::row(&snapshot, "m-solo");
    assert_eq!(
        solo.thread_count, None,
        "a lone message is not dressed up as a conversation"
    );
    assert_eq!(snapshot.view_mode, ViewMode::Threaded);
}

/// Selects the work Archive, the folder whose scoping the threaded tests prove.
async fn open_archive(app: &crate::App<fakes::FakeProvider>) {
    app.dispatch(Intent::SelectFolder {
        folder: FolderRef::from_parts("work", "archive".to_owned()).unwrap(),
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn the_threaded_view_scopes_to_the_folder_and_groups_across_it() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = listed_mailbox(
        &surfaces,
        vec![
            // One conversation split across the two folders: the inbox half is the
            // newer, the archive half the older.
            threaded(
                delivered_at(mail("m-inb", "inbox", "Re: Split (inb)"), 8, 30),
                "th-split",
            ),
            threaded(
                delivered_at(mail("m-arch", "archive", "Re: Split (arch)"), 8, 0),
                "th-split",
            ),
            // One conversation wholly inside the inbox, and one wholly inside the
            // archive: each is fully outside the other folder's view.
            threaded(
                delivered_at(mail("i1", "inbox", "Re: Inbox only"), 7, 0),
                "th-inbox-only",
            ),
            threaded(
                delivered_at(mail("i2", "inbox", "Re: Inbox only"), 7, 10),
                "th-inbox-only",
            ),
            threaded(
                delivered_at(mail("a1", "archive", "Re: Archive only"), 9, 0),
                "th-archive-only",
            ),
            threaded(
                delivered_at(mail("a2", "archive", "Re: Archive only"), 8, 45),
                "th-archive-only",
            ),
        ],
    )
    .await;
    app.dispatch(Intent::SetViewMode(ViewMode::Threaded))
        .await
        .unwrap();

    open_inbox(&app).await;
    let inbox = app.mailbox_list();
    assert_eq!(
        fakes::row_keys(&inbox),
        ["th-split", "th-inbox-only"],
        "only conversations an inbox member touches, newest in-folder activity first"
    );
    let split = fakes::row(&inbox, "th-split");
    assert_eq!(split.subject, "Re: Split (inb)");
    assert_eq!(
        split.sent_at, "2026-10-09T08:30:00Z",
        "the face is the conversation's newest in-folder member, not its newest overall \
         when that one is filed elsewhere"
    );
    assert_eq!(
        split.thread_count,
        Some(2),
        "the out-of-folder member rides in the conversation it answers"
    );

    open_archive(&app).await;
    let archive = app.mailbox_list();
    assert_eq!(
        fakes::row_keys(&archive),
        ["th-archive-only", "th-split"],
        "only conversations an archive member touches, newest in-folder activity first"
    );
    let split = fakes::row(&archive, "th-split");
    assert_eq!(
        split.subject, "Re: Split (arch)",
        "the face follows the folder: the archive member fronts the conversation here"
    );
    assert_eq!(split.sent_at, "2026-10-09T08:00:00Z");
    assert_eq!(
        split.thread_count,
        Some(2),
        "the inbox member rides in from the other side of the same rule"
    );
}

#[tokio::test]
async fn switching_the_view_mode_reprojects_the_rows_and_resets_the_window() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = listed_mailbox(
        &surfaces,
        vec![
            delivered_at(mail("m1", "inbox", "One"), 8, 0),
            threaded(delivered_at(mail("m2", "inbox", "Two"), 8, 5), "th-2"),
            threaded(delivered_at(mail("m3", "inbox", "Three"), 8, 10), "th-2"),
        ],
    )
    .await;

    open_inbox(&app).await;
    app.dispatch(Intent::ShowMore).await.unwrap();
    assert_eq!(app.visible_limit(), crate::PAGE * 2);

    app.dispatch(Intent::SetViewMode(ViewMode::Threaded))
        .await
        .unwrap();

    assert_eq!(app.visible_limit(), crate::PAGE, "the window reset");
    let snapshot = app.mailbox_list();
    assert_eq!(
        fakes::row_keys(&snapshot),
        ["th-2", "m1"],
        "the same mail, regrouped"
    );
    assert_eq!(snapshot.visible_limit, u32::try_from(crate::PAGE).unwrap());
}

#[tokio::test]
async fn mark_read_writes_through_and_republishes_the_row() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox()],
        vec![
            delivered_at(mail("m1", "inbox", "One"), 8, 0),
            delivered_at(mail("m2", "inbox", "Two"), 8, 5),
        ],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    open_inbox(&app).await;
    assert!(fakes::row(&app.mailbox_list(), "m1").unread);
    let signals = surfaces.lock().unwrap().len();

    app.dispatch(Intent::MarkRead {
        row: RowRef::Message(MessageRef::from_parts("work", "m1".to_owned()).unwrap()),
        read: true,
    })
    .await
    .unwrap();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_provider::MailEdit::mark_seen(
            engine_api::ProviderKey::new("m1").unwrap(),
            true,
        )],
        "one keyword edit through the provider, exactly as the engine's edit_mail sends it"
    );
    let snapshot = app.mailbox_list();
    assert!(!fakes::row(&snapshot, "m1").unread, "the row flipped");
    assert!(fakes::row(&snapshot, "m2").unread, "the others did not");
    assert!(
        surfaces.lock().unwrap().len() > signals,
        "the flipped list is republished"
    );

    // The store shows the queued edit, so a re-sync that delivers nothing new cannot
    // undo it.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        !fakes::row(&app.mailbox_list(), "m1").unread,
        "the read state survives a refresh"
    );
}

#[tokio::test]
async fn set_flagged_writes_through_and_republishes_the_row() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox()],
        vec![delivered_at(mail("m1", "inbox", "One"), 8, 0)],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    open_inbox(&app).await;

    app.dispatch(Intent::SetFlagged {
        row: RowRef::Message(MessageRef::from_parts("work", "m1".to_owned()).unwrap()),
        flagged: true,
    })
    .await
    .unwrap();

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [engine_provider::MailEdit::set_flagged(
            engine_api::ProviderKey::new("m1").unwrap(),
            true,
        )],
    );
    assert!(fakes::row(&app.mailbox_list(), "m1").flagged);
}

#[tokio::test]
async fn mark_read_on_a_thread_row_edits_every_member() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(
        vec![inbox()],
        vec![
            threaded(
                delivered_at(mail("m1", "inbox", "Re: Launch"), 8, 0),
                "th-1",
            ),
            threaded(
                read(delivered_at(mail("m2", "inbox", "Re: Launch"), 8, 30)),
                "th-1",
            ),
        ],
    );
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    open_inbox(&app).await;
    app.dispatch(Intent::SetViewMode(ViewMode::Threaded))
        .await
        .unwrap();
    assert!(fakes::row(&app.mailbox_list(), "th-1").unread);

    app.dispatch(Intent::MarkRead {
        row: RowRef::Thread(ThreadRef::from_parts("work", "th-1".to_owned()).unwrap()),
        read: true,
    })
    .await
    .unwrap();

    let recorded = edits.lock().unwrap().clone();
    let mut targets: Vec<&str> = recorded
        .iter()
        .map(|edit| match edit {
            engine_provider::MailEdit::SetKeywords { target, .. } => target.as_str(),
            other => panic!("expected a keyword edit, got {other:?}"),
        })
        .collect();
    targets.sort_unstable();
    assert_eq!(targets, ["m1", "m2"], "every member of the conversation");
    let snapshot = app.mailbox_list();
    assert!(!fakes::row(&snapshot, "th-1").unread, "the row flipped");
}

#[tokio::test]
async fn a_superseded_rebuild_keeps_the_older_row_window() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let messages: Vec<engine_api::Message> = (0..crate::PAGE + 10)
        .map(|index| {
            delivered_at(
                mail(
                    &format!("m{index:03}"),
                    "inbox",
                    &format!("Subject {index}"),
                ),
                8 + u8::try_from(index / 60).unwrap(),
                u8::try_from(index % 60).unwrap(),
            )
        })
        .collect();
    let app = listed_mailbox(&surfaces, messages).await;

    open_inbox(&app).await;
    let superseded = app.list_generation();
    app.dispatch(Intent::ShowMore).await.unwrap();
    assert_eq!(app.mailbox_list().rows.len(), crate::PAGE + 10);
    let signals = surfaces.lock().unwrap().len();

    app.rebuild_snapshot_for(superseded).await;

    assert_eq!(
        app.mailbox_list().rows.len(),
        crate::PAGE + 10,
        "a rebuild a newer intent superseded must not repaint the older window"
    );
    assert_eq!(
        surfaces.lock().unwrap().len(),
        signals,
        "a superseded rebuild must not publish at all"
    );
}
