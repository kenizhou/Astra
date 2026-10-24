// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the spam-report writes: the verdict mapping the two verbs pin,
//! the filing destination each verdict resolves, the capability gate, the
//! honest no-folder refusal, the idempotency that keeps a junk report and its
//! not-junk correction two ops, and the thread fan-out. The fixtures live in
//! `dispatch_fakes`; the report leg records verdict and filing the way the
//! edit leg records its edits.

use std::sync::{Arc, Mutex};

use engine_api::{Mailbox, MailboxId, MailboxRole, MessageReport, ProviderKey, ReportVerdict};

use crate::{
    dispatch_fakes as fakes,
    reference::{MessageRef, RowRef, ThreadRef},
    Intent, ReportWriteError, Surface,
};

use fakes::{account, app, delivered_at, folder, inbox, mail, threaded};

/// The role-bearing Junk folder.
fn junk() -> Mailbox {
    let mut mailbox = folder("junk", "Junk");
    mailbox.role = Some(MailboxRole::Junk);
    mailbox
}

/// One inbox message under `key`, at a fixed instant.
fn inbox_mail(key: &str) -> engine_api::Message {
    delivered_at(mail(key, "inbox", "One"), 8, 0)
}

/// One message under `key` sitting in the Junk folder, the mail the NotJunk
/// verb is offered on.
fn junk_mail(key: &str) -> engine_api::Message {
    delivered_at(mail(key, "junk", "Misfiled"), 8, 0)
}

/// A message row on `account` under `key`.
fn message_row(account: &str, key: &str) -> RowRef {
    RowRef::Message(MessageRef::from_parts(account, key.to_owned()).unwrap())
}

/// The report a junk verdict on `key` files into `junk`: the shape every
/// destination assertion names.
fn junk_report(key: &str, destination: &str) -> MessageReport {
    MessageReport::new(
        ProviderKey::new(key).unwrap(),
        ReportVerdict::Junk,
        MailboxId::try_from(destination).unwrap(),
    )
}

#[tokio::test]
async fn marking_junk_reports_the_junk_verdict_and_files_into_the_junk_folder() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = reporting(vec![inbox(), junk()], vec![inbox_mail("m1")]);
    let reports = Arc::clone(&provider.reports);
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::MarkJunk {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        reports.lock().unwrap().as_slice(),
        [junk_report("m1", "junk")],
        "one report through the provider: the Junk verdict, filed into the role's folder"
    );
    assert!(
        edits.lock().unwrap().is_empty(),
        "the report files the message itself; the caller queues no move beside it"
    );
    assert!(
        surfaces.lock().unwrap().contains(&Surface::MailboxList),
        "the filing changes the list, so the write republishes it"
    );
}

#[tokio::test]
async fn marking_not_junk_reports_the_not_junk_verdict_and_files_back_to_the_inbox() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = reporting(vec![inbox(), junk()], vec![junk_mail("m1")]);
    let reports = Arc::clone(&provider.reports);
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::MarkNotJunk {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        reports.lock().unwrap().as_slice(),
        [MessageReport::new(
            ProviderKey::new("m1").unwrap(),
            ReportVerdict::NotJunk,
            MailboxId::try_from("inbox").unwrap(),
        )],
        "the correction reports NotJunk, filed back into the Inbox"
    );
    assert!(
        edits.lock().unwrap().is_empty(),
        "the report files the message back; no move edit rides beside it"
    );
}

#[tokio::test]
async fn junk_resolution_falls_back_to_the_conventional_folder_name_when_no_role_is_tagged() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    // Many servers tag neither Junk nor Archive: an untagged folder named the
    // conventional name is the junk destination, as it is for the archive.
    let provider = reporting(
        vec![inbox(), folder("spambox", "Spam")],
        vec![inbox_mail("m1")],
    );
    let reports = Arc::clone(&provider.reports);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::MarkJunk {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        reports.lock().unwrap().as_slice(),
        [junk_report("m1", "spambox")],
        "the conventional name resolves the filing destination when no role is tagged"
    );
}

#[tokio::test]
async fn a_junk_report_and_its_not_junk_correction_are_two_ops() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = reporting(vec![inbox(), junk()], vec![inbox_mail("m1")]);
    let reports = Arc::clone(&provider.reports);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::MarkJunk {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();
    app.dispatch(Intent::MarkNotJunk {
        row: message_row("work", "m1"),
    })
    .await
    .unwrap();

    assert_eq!(
        reports.lock().unwrap().as_slice(),
        [
            junk_report("m1", "junk"),
            MessageReport::new(
                ProviderKey::new("m1").unwrap(),
                ReportVerdict::NotJunk,
                MailboxId::try_from("inbox").unwrap(),
            ),
        ],
        "each report is its own intent: the idempotency key never collapses the pair"
    );
}

#[tokio::test]
async fn reporting_a_thread_row_reports_every_member() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = reporting(
        vec![inbox(), junk()],
        vec![
            threaded(inbox_mail("m1"), "th-1"),
            threaded(delivered_at(mail("m2", "inbox", "Two"), 8, 30), "th-1"),
            inbox_mail("m3"),
        ],
    );
    let reports = Arc::clone(&provider.reports);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    app.dispatch(Intent::MarkJunk {
        row: RowRef::Thread(ThreadRef::from_parts("work", "th-1".to_owned()).unwrap()),
    })
    .await
    .unwrap();

    let recorded = reports.lock().unwrap().clone();
    let mut targets: Vec<(&str, &str)> = recorded
        .iter()
        .map(|report| (report.target.as_str(), report.destination.as_str()))
        .collect();
    targets.sort_unstable();
    assert_eq!(
        targets,
        [("m1", "junk"), ("m2", "junk")],
        "every member of the conversation is reported, the untouched row is not"
    );
}

#[tokio::test]
async fn a_report_on_a_provider_without_the_capability_is_refused_typed() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with_mail(vec![inbox(), junk()], vec![inbox_mail("m1")]);
    let reports = Arc::clone(&provider.reports);
    let edits = Arc::clone(&provider.edits);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let junk = app.mark_junk(message_row("work", "m1")).await.unwrap_err();
    let not_junk = app
        .mark_not_junk(message_row("work", "m1"))
        .await
        .unwrap_err();

    assert_eq!(junk, ReportWriteError::NotCapable);
    assert_eq!(not_junk, ReportWriteError::NotCapable);
    assert!(
        reports.lock().unwrap().is_empty() && edits.lock().unwrap().is_empty(),
        "a refused report queues nothing"
    );
}

#[tokio::test]
async fn a_report_with_no_folder_to_file_into_is_refused_typed() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = reporting(vec![inbox()], vec![inbox_mail("m1")]);
    let reports = Arc::clone(&provider.reports);
    let app = app(vec![account("work", provider)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let outcome = app.mark_junk(message_row("work", "m1")).await.unwrap_err();

    assert_eq!(outcome, ReportWriteError::NoTargetFolder);
    assert!(
        reports.lock().unwrap().is_empty(),
        "no report is issued without a destination to file into"
    );
}

#[tokio::test]
async fn a_report_on_an_unconfigured_account_reports_the_error() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = reporting(vec![inbox(), junk()], vec![inbox_mail("m1")]);
    let app = app(vec![account("work", provider)], &surfaces);

    let outcome = app.mark_junk(message_row("gone", "m1")).await.unwrap_err();

    assert_eq!(outcome, ReportWriteError::UnknownAccount);
}

#[tokio::test]
async fn a_report_on_an_account_without_a_provider_reports_the_error() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = reporting(vec![inbox(), junk()], vec![inbox_mail("m1")]);
    let mut bare = account("bare", provider);
    bare.providers = Vec::new();
    let app = app(vec![bare], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();

    let outcome = app.mark_junk(message_row("bare", "m1")).await.unwrap_err();

    assert_eq!(outcome, ReportWriteError::NoProvider);
}

/// The reporting provider shape every behaviour test drives: the JMAP-shaped
/// fake with the report capability scripted, so the report leg records
/// verdict and filing.
fn reporting(mailboxes: Vec<Mailbox>, messages: Vec<engine_api::Message>) -> fakes::FakeProvider {
    fakes::FakeProvider::with_mail(mailboxes, messages).with_reports()
}
