// SPDX-License-Identifier: GPL-3.0-only
//! The gated delivery proofs against the live harness: the background
//! loop's two landing arms over the real wire.
//!
//! The IDLE proof stands the recipient's INBOX watch through the account
//! layer's own `open_watch`, delivers a mail through the harness's own
//! submission path while the watch stands, and pins the whole chain the
//! loop runs on a `Changed`: the event inside a bound, the account's
//! ordinary single-account pass landing the mail, and the core's new-mail
//! queue holding the notice (who it is from, what it is about, and the
//! preview where the commit's tier carried one; the IMAP metadata tier
//! commits none, and the host bridge's notification body falls back to
//! the subject alone).
//!
//! The poll proof pins the other arm's landing, the shape this suite
//! chooses and records: the loop's state machine (the cadence wait, the
//! backoff, the reconnect) is the fake clock's, proven offline over the
//! desktop host's scripted driver; the honest live half is the wiring the
//! arm rides, proven here in full: the account's cadence written through
//! the settings intent and answered by the settings snapshot the loop's
//! host reads it from, and the arm's pass (the same single-account sync
//! and queue drain the poll arm runs after its wait) landing a delivered
//! mail with the queue holding its notice.
//!
//! Both proofs withdraw their copies through the shared sweep, so the
//! harness keeps its seeded shape.

use std::{sync::Arc, time::Duration};

use astra_viewmodel::RowKey;
use engine_api::{AccountId, Engine, Provider};

use crate::protocol::SettingsIntent;
use crate::{reference::MessageRef, Account, App, AppObserver, FolderRef, Intent, NewMailBatch};

use super::proof::{self, document};
use super::{dial_with_accepted_certificate, harness, inbox_key, row_by_subject, CountingObserver};

/// The subject the IDLE proof pins: unique among the harness fixtures, so
/// a row carrying it is this proof's mail and nothing the seed delivered.
const IDLE_SUBJECT: &str = "Harness idle watch proof";

/// The words the IDLE proof's document carries; what the landed body opens
/// back and the notice's preview begins.
const IDLE_WORDS: &str = "The standing watch answers this delivery.";

/// How long the standing watch has to answer the delivery: seconds of
/// margin over a local delivery that lands in milliseconds, so a loaded
/// host never flakes the bound.
const WATCH_BOUND: Duration = Duration::from_secs(30);

/// The subject the poll proof pins: unique among the harness fixtures.
const POLL_SUBJECT: &str = "Harness poll arm proof";

/// The words the poll proof's document carries.
const POLL_WORDS: &str = "The cadence pass lands this note.";

/// Dials both harness accounts and returns the app over them beside the
/// recipient's retained config and connections: the standing watch opens
/// through the same connections the recipient's providers draw on, the way
/// the host's dial table holds an account's dials beside its providers.
async fn delivery_suite(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
) -> (
    App<Box<dyn Provider>>,
    AccountId,
    AccountId,
    astra_account::AccountConfig,
    Arc<astra_account::ImapConnections>,
) {
    let (sender, sender_providers, ..) =
        dial_with_accepted_certificate(host, port, user, password).await;
    let (recipient, recipient_providers, recipient_config, recipient_connections) =
        dial_with_accepted_certificate(host, port, proof::RECIPIENT, password).await;
    let observer: Arc<dyn AppObserver> = Arc::new(CountingObserver::default());
    let app = App::new(
        Arc::new(Engine::open_in_memory().expect("the engine store")),
        vec![
            Account {
                id: sender.clone(),
                providers: sender_providers,
                identity: engine_api::EmailAddress::new("astra1@example.test"),
            },
            Account {
                id: recipient.clone(),
                providers: recipient_providers,
                identity: engine_api::EmailAddress::new(proof::RECIPIENT),
            },
        ],
        None,
        observer,
    );
    (
        app,
        sender,
        recipient,
        recipient_config,
        recipient_connections,
    )
}

/// The converging preamble both delivery proofs open with: the first pass
/// imports the harness state and settles the queue's import suppression,
/// the sweep withdraws a prior interrupted run's copies of this proof's
/// subject, the second pass lands the deletes, and the queue drains to a
/// known empty.
async fn converge(
    app: &App<Box<dyn Provider>>,
    sender: &AccountId,
    recipient: &AccountId,
    subject: &str,
) {
    app.dispatch(Intent::RefreshMail).await.unwrap();
    proof::delete_subject_mail(app, sender, &[subject]).await;
    proof::delete_subject_mail(app, recipient, &[subject]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    let settled = app.drain_new_mail();
    assert!(
        settled.messages.is_empty(),
        "the settled state holds no notices: {settled:?}"
    );
}

/// The withdrawal tail both delivery proofs close with: both copies leave
/// through the shared sweep, the next passes land and observe the deletes,
/// and neither account's folders hold the proof's mail.
async fn withdraw(
    app: &App<Box<dyn Provider>>,
    sender: &AccountId,
    recipient: &AccountId,
    subject: &str,
) {
    proof::delete_subject_mail(app, sender, &[subject]).await;
    proof::delete_subject_mail(app, recipient, &[subject]).await;
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .all(|row| row.subject != subject),
        "the filed copy is withdrawn from the sender's folders"
    );
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(app, recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    assert!(
        app.mailbox_list()
            .rows
            .iter()
            .all(|row| row.subject != subject),
        "the delivered copy is withdrawn from the recipient's INBOX"
    );
}

/// Pins the queue's notice for `subject`: the account it landed in, who
/// the mail is from, and what it is about, the facts the host bridge's OS
/// notification carries. The preview ("how it begins") carries what the
/// commit's tier carried: the IMAP family commits metadata with no snippet
/// (the preview is the JMAP-carried field), so the notice holds `None`
/// here and the bridge's body falls back to the subject alone.
fn notice_for(batch: &NewMailBatch, subject: &str, recipient: &AccountId) {
    let notice = batch
        .messages
        .iter()
        .find(|notice| notice.subject.as_deref() == Some(subject))
        .unwrap_or_else(|| panic!("the queue holds the delivered notice: {batch:?}"));
    assert_eq!(
        notice.account, *recipient,
        "the notice names the account the mail landed in"
    );
    assert_eq!(
        notice.from.as_ref().expect("the envelope's sender").email,
        "astra1@example.test",
        "the notice says who the mail is from"
    );
    assert!(
        notice.preview.is_none(),
        "the IMAP metadata tier commits no snippet, so the notice holds no preview: {:?}",
        notice.preview
    );
}

/// The INBOX watch proof against the live harness: a mail delivered while
/// the recipient's INBOX watch stands answers `Changed` within a bound, the
/// account's ordinary pass lands the mail, and the core's queue holds its
/// notice, withdrawn at the end like every delivery proof.
#[tokio::test]
async fn the_inbox_watch_answers_changed_and_its_pass_lands_the_mail() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient, recipient_config, recipient_connections) =
        delivery_suite(&host, &port, &user, &password).await;
    converge(&app, &sender, &recipient, IDLE_SUBJECT).await;

    // The watch stands before the delivery rides: the recipient's INBOX
    // watch through the account layer's own opening, its `next` issued
    // (the IDLE entered) ahead of the mail.
    let mut watch = match astra_account::open_watch(&recipient_connections, &recipient_config).await
    {
        Ok(astra_account::WatchFace::Watch(watch)) => watch,
        Ok(face) => panic!("the harness INBOX answers a standing watch, not {face:?}"),
        Err(err) => panic!("the harness INBOX watch opens: {err}"),
    };
    let standing = tokio::spawn(async move { watch.next().await });

    // The delivery, through the harness's own submission path: the send
    // fixtures' gesture, queued and drained inside the dispatch.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: IDLE_SUBJECT.to_owned(),
        document: document(IDLE_WORDS),
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    })
    .await
    .unwrap();
    assert!(
        app.sending().rows.is_empty(),
        "a delivered send leaves nothing in the queue"
    );

    // The watch answers within the bound: a real push for a real delivery.
    let answered = tokio::time::timeout(WATCH_BOUND, standing)
        .await
        .expect("the standing watch answers within the bound")
        .expect("the watch task joins");
    assert_eq!(
        answered.expect("the watch session stands"),
        astra_account::WatchEvent::Changed,
        "the delivery woke the INBOX watch"
    );

    // The Changed arm's pass: the account's ordinary single-account sync,
    // the same seam the loop drives, landing the mail.
    app.sync_account(&recipient).await;
    let batch = app.drain_new_mail();
    assert_eq!(batch.overflow, 0, "one delivery is one notice");
    notice_for(&batch, IDLE_SUBJECT, &recipient);

    // And the mail itself landed: the recipient's INBOX holds the row, and
    // the words open through the product's own reading pipeline.
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, IDLE_SUBJECT);
    assert_eq!(
        row.sender_address, "astra1@example.test",
        "the landed mail is from the sending account"
    );
    assert!(row.unread, "a mail nobody opened reads unread");
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::OpenMessage {
        message: MessageRef { account, key },
    })
    .await
    .unwrap();
    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(IDLE_WORDS)),
        "the landed body is the composer's words: {:?}",
        reading.plain
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    withdraw(&app, &sender, &recipient, IDLE_SUBJECT).await;
}

/// The poll arm proof against the live harness: the account's cadence
/// written through the settings intent and answered by the settings
/// snapshot the loop's host reads it from, and the arm's pass (the same
/// single-account sync and queue drain the poll arm runs after its wait)
/// landing a delivered mail with the queue holding its notice, withdrawn
/// at the end like every delivery proof.
#[tokio::test]
async fn a_poll_arm_pass_lands_the_delivered_mail_and_the_queue_holds_it() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let (app, sender, recipient, ..) = delivery_suite(&host, &port, &user, &password).await;
    converge(&app, &sender, &recipient, POLL_SUBJECT).await;

    // The cadence the loop's host waits for the account: written through
    // the settings intent, answered by the settings snapshot row the host
    // reads the cadence from.
    app.dispatch(Intent::Settings(SettingsIntent::SetSyncSettings {
        account: recipient.clone(),
        depth_months: None,
        body_cap: None,
        poll_minutes: Some(15),
    }))
    .await
    .unwrap();
    let snapshot = app.settings();
    let row = snapshot
        .accounts
        .iter()
        .find(|row| row.account == recipient)
        .expect("the account renders a settings row");
    let sync = row.sync.expect("the account carries its sync settings");
    assert_eq!(sync.poll_minutes, 15, "the cadence the loop waits");

    // The delivery, with no watch standing anywhere: the poll arm's world.
    app.dispatch(Intent::SubmitRichMail {
        from: sender.clone(),
        to: proof::RECIPIENT.to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: POLL_SUBJECT.to_owned(),
        document: document(POLL_WORDS),
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    })
    .await
    .unwrap();
    assert!(
        app.sending().rows.is_empty(),
        "a delivered send leaves nothing in the queue"
    );

    // The poll arm's pass: the same single-account sync and queue drain
    // the arm runs after its cadence wait, proven here without the wait
    // (the wait itself is the fake clock's, offline in the driver).
    app.sync_account(&recipient).await;
    let batch = app.drain_new_mail();
    assert_eq!(batch.overflow, 0, "one delivery is one notice");
    notice_for(&batch, POLL_SUBJECT, &recipient);

    // And the mail itself landed, its words opening through the product's
    // own reading pipeline.
    let inbox = FolderRef::from_parts(recipient.as_str(), inbox_key(&app, &recipient))
        .expect("the recipient's INBOX reference");
    app.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = app.mailbox_list();
    let row = row_by_subject(&snapshot, POLL_SUBJECT);
    assert_eq!(
        row.sender_address, "astra1@example.test",
        "the landed mail is from the sending account"
    );
    let RowKey::Message { account, key } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    app.dispatch(Intent::OpenMessage {
        message: MessageRef { account, key },
    })
    .await
    .unwrap();
    let reading = app
        .reading()
        .expect("the open publishes a reading snapshot");
    assert!(
        reading
            .plain
            .as_deref()
            .is_some_and(|body| body.contains(POLL_WORDS)),
        "the landed body is the composer's words: {:?}",
        reading.plain
    );
    app.dispatch(Intent::CloseReading).await.unwrap();

    withdraw(&app, &sender, &recipient, POLL_SUBJECT).await;
}
