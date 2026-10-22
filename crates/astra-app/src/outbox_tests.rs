// SPDX-License-Identifier: GPL-3.0-only
//! Tests for sending: the submit arm through the durable outbox, the outbox
//! family over the engine's queue ops, and the Sending surface that publishes
//! the queue itself. The fake's SMTP leg scripts every honest outcome a send
//! can reach (delivered, unreachable, refused for good, handed over with no
//! answer), so each arm is pinned against the engine's real op lifecycle.

use std::sync::{Arc, Mutex};

use astra_composer::{Block, ComposerDocument, InlineContent, Paragraph, TextRun};
use astra_viewmodel::QueuedState;

use crate::{
    dispatch_fakes as fakes, dispatch_fakes::ScriptedSend, App, Intent, OutboxIntent, QueuedRef,
    Surface,
};

/// A well-formed account id, the way the sending tests name one.
fn account(value: &str) -> engine_api::AccountId {
    engine_api::AccountId::try_from(value).unwrap()
}

/// A one-paragraph document carrying `text`, the smallest document a send holds.
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

/// What the composer's Send button dispatches: a rich submit about `subject`.
fn submit(subject: &str) -> Intent {
    Intent::SubmitRichMail {
        from: account("work"),
        to: "you@test.local".to_owned(),
        cc: String::new(),
        bcc: String::new(),
        subject: subject.to_owned(),
        document: document("On its way"),
        blobs: Vec::new(),
        crypto: crate::crypto::CryptoFlags::default(),
    }
}

/// An in-memory app over one `work` account whose SMTP leg answers `script`.
fn app_over(
    script: Vec<ScriptedSend>,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> App<fakes::FakeProvider> {
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]).submitting(script);
    fakes::app(vec![fakes::account("work", provider)], surfaces)
}

/// The one queued row the sending snapshot holds; panics when the queue is not
/// holding exactly one send, which is the assertion.
fn the_one_queued_send(app: &App<fakes::FakeProvider>) -> crate::reference::QueuedRef {
    let snapshot = app.sending();
    assert_eq!(snapshot.rows.len(), 1, "the queue holds exactly one send");
    let row = &snapshot.rows[0];
    QueuedRef::from_parts(row.queued.account.as_str(), row.queued.op).unwrap()
}

#[tokio::test]
async fn a_delivered_submit_reaches_the_recipients_and_leaves_no_row() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = app_over(vec![ScriptedSend::Delivered], &surfaces);

    app.dispatch(submit("Quarterly review")).await.unwrap();

    assert_eq!(
        app.sending().rows.len(),
        0,
        "a delivered send has nothing left to show in the queue"
    );
    assert_eq!(
        fakes::sending_signals(&surfaces),
        0,
        "the queue never moved"
    );
}

#[tokio::test]
async fn a_submit_the_network_holds_stays_queued_then_goes_out_when_asked() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .submitting(vec![ScriptedSend::Unreachable, ScriptedSend::Delivered]);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.dispatch(submit("On a train")).await.unwrap();

    {
        let snapshot = app.sending();
        assert_eq!(snapshot.rows.len(), 1, "the message is kept, not lost");
        let row = &snapshot.rows[0];
        assert_eq!(row.queued.account.as_str(), "work");
        assert_eq!(row.subject, "On a train");
        assert_eq!(row.state, QueuedState::Queued);
        assert!(
            !row.arrived_at.is_empty(),
            "the row says when the send joined the queue"
        );
    }
    assert!(
        fakes::sending_signals(&surfaces) >= 1,
        "a send joining the queue is signalled"
    );

    // The user asks for it to go now: the backoff is cleared and the attempt
    // happens in this gesture, not at some later pass.
    let queued = the_one_queued_send(&app);
    app.dispatch(Intent::Outbox(OutboxIntent::SendNow { queued }))
        .await
        .unwrap();

    assert_eq!(app.sending().rows.len(), 0, "the send went out");
    assert_eq!(
        submits.lock().unwrap().len(),
        2,
        "the parked send was attempted again, not replaced"
    );
    assert!(
        submits.lock().unwrap()[0].subject == *"On a train"
            && submits.lock().unwrap()[1].subject == *"On a train",
        "both attempts carried the same message"
    );
}

#[tokio::test]
async fn a_send_the_server_refused_is_kept_until_the_user_acts() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .submitting(vec![ScriptedSend::Refused, ScriptedSend::Delivered]);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.dispatch(submit("Back off")).await.unwrap();

    let snapshot = app.sending();
    assert_eq!(
        snapshot.rows.len(),
        1,
        "a refused send is the user's message still"
    );
    assert_eq!(snapshot.rows[0].state, QueuedState::Failed);

    // Nothing retries it on its own: a refresh's drain makes no new attempt.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert_eq!(
        submits.lock().unwrap().len(),
        1,
        "a refused send waits for the user, not for a drain"
    );

    // Send again: it goes back in the queue and out through the leg.
    let queued = the_one_queued_send(&app);
    app.dispatch(Intent::Outbox(OutboxIntent::SendNow { queued }))
        .await
        .unwrap();
    assert_eq!(app.sending().rows.len(), 0);
    assert_eq!(submits.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn a_queued_send_can_be_withdrawn() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .submitting(vec![ScriptedSend::Unreachable, ScriptedSend::Delivered]);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.dispatch(submit("Never mind")).await.unwrap();
    let queued = the_one_queued_send(&app);

    app.dispatch(Intent::Outbox(OutboxIntent::Cancel { queued }))
        .await
        .unwrap();

    assert_eq!(app.sending().rows.len(), 0, "the withdrawn send is gone");
    // A later pass must not deliver what the user took back.
    app.dispatch(Intent::RefreshMail).await.unwrap();
    assert_eq!(
        submits.lock().unwrap().len(),
        1,
        "a withdrawn send is never attempted again"
    );
    assert_eq!(app.sending().rows.len(), 0);
}

#[tokio::test]
async fn an_unconfirmed_send_prompts_then_resolves_as_sent() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider =
        fakes::FakeProvider::with(vec![fakes::inbox()]).submitting(vec![ScriptedSend::Silent]);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.dispatch(submit("Did it go?")).await.unwrap();

    let snapshot = app.sending();
    assert_eq!(snapshot.rows.len(), 1);
    assert_eq!(
        snapshot.rows[0].state,
        QueuedState::Unconfirmed,
        "a send with no answer is a standing question, never retried"
    );

    // The user says it arrived: it settles and is never sent again.
    let queued = the_one_queued_send(&app);
    app.dispatch(Intent::Outbox(OutboxIntent::ConfirmSent { queued }))
        .await
        .unwrap();
    assert_eq!(app.sending().rows.len(), 0);
    assert_eq!(
        submits.lock().unwrap().len(),
        1,
        "confirming it was sent never sends it again"
    );
}

#[tokio::test]
async fn an_unconfirmed_send_answered_not_sent_goes_out_again() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .submitting(vec![ScriptedSend::Silent, ScriptedSend::Delivered]);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.dispatch(submit("Once more")).await.unwrap();
    let queued = the_one_queued_send(&app);

    app.dispatch(Intent::Outbox(OutboxIntent::ConfirmNotSent { queued }))
        .await
        .unwrap();

    assert_eq!(app.sending().rows.len(), 0, "the requeued send went out");
    assert_eq!(
        submits.lock().unwrap().len(),
        2,
        "the answer of not delivered is the one way a may-have-sent message is retried"
    );
}

#[tokio::test]
async fn the_sending_surface_publishes_each_transition_once() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()])
        .submitting(vec![ScriptedSend::Unreachable, ScriptedSend::Delivered]);
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    app.dispatch(submit("Each step")).await.unwrap();
    assert_eq!(
        fakes::sending_signals(&surfaces),
        1,
        "the send joining the queue is one transition"
    );

    // A show that finds nothing moved says nothing: the pull answers the rest.
    app.dispatch(Intent::Outbox(OutboxIntent::Show))
        .await
        .unwrap();
    assert_eq!(fakes::sending_signals(&surfaces), 1);

    let queued = the_one_queued_send(&app);
    app.dispatch(Intent::Outbox(OutboxIntent::SendNow { queued }))
        .await
        .unwrap();
    assert_eq!(
        fakes::sending_signals(&surfaces),
        2,
        "the send leaving the queue is the second transition, said once"
    );
}

#[tokio::test]
async fn a_submit_without_a_recipient_or_a_configured_account_queues_nothing() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let provider = fakes::FakeProvider::with(vec![fakes::inbox()]);
    let submits = provider.submits();
    let app = fakes::app(vec![fakes::account("work", provider)], &surfaces);

    // No recipient: never send a recipient-less message.
    let mut no_one = submit("To nobody");
    if let Intent::SubmitRichMail { to, .. } = &mut no_one {
        *to = String::new();
    }
    app.dispatch(no_one).await.unwrap();
    assert_eq!(submits.lock().unwrap().len(), 0, "nothing was sent");

    // An account the app no longer holds: nothing to send through.
    let mut ghost = submit("From nowhere");
    if let Intent::SubmitRichMail { from, .. } = &mut ghost {
        *from = account("ghost");
    }
    app.dispatch(ghost).await.unwrap();
    assert_eq!(submits.lock().unwrap().len(), 0, "nothing was sent");

    assert_eq!(app.sending().rows.len(), 0, "no op was queued either way");
    assert_eq!(fakes::sending_signals(&surfaces), 0);
}
