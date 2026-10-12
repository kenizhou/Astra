// SPDX-License-Identifier: GPL-3.0-only
//! Execution tests: matched actions run through the same mark-read and
//! flag write paths the intents drive (the provider's edit log is the
//! witness), each execution lands its audit row under the pass's
//! correlation id, and the row state the write path republishes is the
//! flipped one. An empty action list writes nothing.

use std::sync::{Arc, Mutex};

use astra_ai::{AiStore, CorrelationId};

use super::Action;
use crate::dispatch_fakes as fakes;
use crate::dispatch_fakes::ai::app_with_ai;
use crate::reference::MessageRef;
use crate::{App, Intent};

/// The reference to the one fixture message.
fn m1() -> MessageRef {
    MessageRef::from_parts("work", "m1".to_owned()).unwrap()
}

/// An app over one account whose Inbox holds the unread `m1` with a plain
/// source, synced and the Inbox selected so the list shows the row the
/// writes flip.
async fn synced_app() -> (
    App<fakes::FakeProvider>,
    Arc<Mutex<Vec<engine_provider::MailEdit>>>,
) {
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::delivered_at(
            fakes::mail("m1", "inbox", "Launch"),
            8,
            0,
        )],
        vec![(
            "m1".to_owned(),
            b"From: ada@example.test\r\nSubject: Launch\r\n\r\nWeekly notes.\r\n".to_vec(),
        )],
    );
    let edits = Arc::clone(&provider.edits);
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_ai(
        vec![fakes::account("work", provider)],
        &surfaces,
        fakes::ai::FixtureJudge::scripted(&[]),
        fakes::ai::FixtureComplete::scripted(&[]),
        Arc::new(AiStore::open_in_memory().unwrap()),
        astra_ai::DlpPolicy::Allow,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::SelectFolder {
        folder: crate::FolderRef::from_parts("work", "inbox".to_owned()).unwrap(),
    })
    .await
    .unwrap();
    (app, edits)
}

#[tokio::test]
async fn matched_actions_write_through_the_mark_and_flag_paths_audited() {
    let (app, edits) = synced_app().await;
    assert!(fakes::row(&app.mailbox_list(), "m1").unread);

    let audit = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    app.execute_actions(
        &m1(),
        &[Action::MarkRead, Action::SetFlagged],
        &correlation,
        &audit,
    )
    .await;

    assert_eq!(
        edits.lock().unwrap().as_slice(),
        [
            engine_provider::MailEdit::mark_seen(engine_api::ProviderKey::new("m1").unwrap(), true,),
            engine_provider::MailEdit::set_flagged(
                engine_api::ProviderKey::new("m1").unwrap(),
                true,
            ),
        ],
        "the closed set's writes went through the engine's edit path",
    );
    let snapshot = app.mailbox_list();
    let row = fakes::row(&snapshot, "m1");
    assert!(!row.unread, "the write path republished the flipped row");
    assert!(row.flagged);

    // The audit ledger: one row per execution, the pass's correlation on
    // each, and no mail content in the preview.
    let audits = audit
        .audit_rows_for_correlation(&correlation)
        .expect("the audit rows read");
    assert_eq!(audits.len(), 2, "each execution is audited");
    for (row, action) in audits.iter().zip(["mark_read", "set_flagged"]) {
        assert_eq!(row.correlation, correlation);
        assert_eq!(row.verb, "ai_rules execute");
        assert!(row.arg_preview.contains(action), "{}", row.arg_preview);
        assert!(row.arg_preview.contains("m1"), "{}", row.arg_preview);
        assert!(
            !row.arg_preview.contains("Weekly notes."),
            "no mail content rides the preview",
        );
        assert_eq!(row.outcome, "ok");
    }
}

#[tokio::test]
async fn an_empty_action_list_writes_and_audits_nothing() {
    let (app, edits) = synced_app().await;
    let audit = AiStore::open_in_memory().unwrap();
    app.execute_actions(&m1(), &[], &CorrelationId::generate(), &audit)
        .await;
    assert!(edits.lock().unwrap().is_empty(), "no action, no write");
    assert!(
        audit
            .audit_rows_for_correlation(&CorrelationId::new("none".to_owned()))
            .expect("reads")
            .is_empty(),
        "no action, no audit row",
    );
}

#[tokio::test]
async fn a_vanished_account_audits_the_error_outcome() {
    // The outcome word's other half: a dispatch that could not go through
    // (the account gone before the write reached it) audits `error`, the
    // same row shape, never silently skipping.
    let provider = fakes::FakeProvider::with_sources(
        vec![fakes::inbox()],
        vec![fakes::delivered_at(
            fakes::mail("m1", "inbox", "Launch"),
            8,
            0,
        )],
        vec![(
            "m1".to_owned(),
            b"From: ada@example.test\r\nSubject: Launch\r\n\r\nWeekly notes.\r\n".to_vec(),
        )],
    );
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_with_ai(
        vec![fakes::account("work", provider)],
        &surfaces,
        fakes::ai::FixtureJudge::scripted(&[]),
        fakes::ai::FixtureComplete::scripted(&[]),
        Arc::new(AiStore::open_in_memory().unwrap()),
        astra_ai::DlpPolicy::Allow,
    );
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let audit = AiStore::open_in_memory().unwrap();
    let correlation = CorrelationId::generate();
    // A message row naming an account the app does not hold: the dispatch
    // cannot route it, which is the vanished-account shape.
    let stranger = crate::MessageRef::from_parts("gone", "m1".to_owned()).unwrap();
    app.execute_actions(&stranger, &[Action::MarkRead], &correlation, &audit)
        .await;

    let audits = audit
        .audit_rows_for_correlation(&correlation)
        .expect("the audit rows read");
    assert_eq!(audits.len(), 1, "the failed dispatch is still audited");
    assert_eq!(audits[0].outcome, "error");
}
