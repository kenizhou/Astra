// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the reply-delivery-failure prompt's other half: answering it
//! through the intent, both ways, with and without a remembered choice, and
//! the subject the host composed riding the reply it mails.

use std::sync::{Arc, Mutex};

use astra_viewmodel::CalendarWriteState;
use engine_api::ScheduleMethod;

use crate::calendar_fakes::{account, calendar_app, draft_method, meeting, CalendarFake};
use crate::{CalendarIntent, Intent, MessageRef, RsvpAnswer, Surface};

/// Boots the app over `fake` with the invitation in the store and the
/// meeting's week in the calendar window, the shared prefix of every prompt
/// test.
async fn prompted_app(
    fake: CalendarFake,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> crate::App<CalendarFake> {
    let app = calendar_app(vec![account("work", fake)], surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: "2026-07-27T00:00:00Z".parse().unwrap(),
        days: 7,
        view: None,
    }))
    .await
    .unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    // The server that says it could not deliver: the prompt stands.
    app.dispatch(Intent::Calendar(CalendarIntent::RespondToInvitation {
        message: MessageRef::from_parts("work", "invite-1".to_owned()).unwrap(),
        response: RsvpAnswer::Accepted,
        comment: None,
        notify_organizer: true,
    }))
    .await
    .unwrap();
    assert_eq!(app.calendar().reply_prompts.len(), 1, "the question stands");
    app
}

/// The intent that answers the standing prompt.
fn answer(send: bool, remember: bool, reply_subject: Option<String>) -> Intent {
    Intent::Calendar(CalendarIntent::AnswerReplyPrompt {
        send,
        remember,
        reply_subject,
    })
}

#[tokio::test]
async fn the_prompt_answers_through_the_intent_both_ways() {
    // Sending: the reply is re-derived from the invitation and mailed, and
    // the question is gone.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .failing_reply_delivery()
        .holding(meeting("evt-1", "info@test.local"));
    let submits = Arc::clone(&fake.submits);
    let app = prompted_app(fake, &surfaces).await;

    app.dispatch(answer(true, false, None)).await.unwrap();

    let sent = submits.lock().unwrap().clone();
    assert_eq!(sent.len(), 1, "the reply was mailed");
    assert_eq!(draft_method(&sent[0]), Some(ScheduleMethod::Reply));
    assert_eq!(sent[0].subject, "Re: Sprint planning");
    assert!(
        app.calendar().reply_prompts.is_empty(),
        "the question cleared when it was answered"
    );

    // Dismissing: nothing is mailed, and the question is gone the same way.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .failing_reply_delivery()
        .holding(meeting("evt-1", "info@test.local"));
    let submits = Arc::clone(&fake.submits);
    let app = prompted_app(fake, &surfaces).await;

    app.dispatch(answer(false, false, None)).await.unwrap();

    assert!(
        submits.lock().unwrap().is_empty(),
        "declining to send sends nothing"
    );
    assert!(app.calendar().reply_prompts.is_empty());
}

#[tokio::test]
async fn a_remembered_send_mails_the_next_failure_without_asking() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .failing_reply_delivery()
        .holding(meeting("evt-1", "info@test.local"));
    let submits = Arc::clone(&fake.submits);
    let app = prompted_app(fake, &surfaces).await;

    app.dispatch(answer(true, true, None)).await.unwrap();
    assert_eq!(submits.lock().unwrap().len(), 1);

    // The same server fails the next answer too: no question this time, the
    // account already decided, and the reply goes out on its own.
    app.dispatch(Intent::Calendar(CalendarIntent::RespondToInvitation {
        message: MessageRef::from_parts("work", "invite-1".to_owned()).unwrap(),
        response: RsvpAnswer::Accepted,
        comment: None,
        notify_organizer: true,
    }))
    .await
    .unwrap();

    assert!(
        app.calendar().reply_prompts.is_empty(),
        "a remembered choice is not asked for again"
    );
    assert_eq!(
        submits.lock().unwrap().len(),
        2,
        "the account said always, so the reply was mailed"
    );
}

#[tokio::test]
async fn a_remembered_refusal_never_asks_and_never_mails() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .failing_reply_delivery()
        .holding(meeting("evt-1", "info@test.local"));
    let submits = Arc::clone(&fake.submits);
    let app = prompted_app(fake, &surfaces).await;

    app.dispatch(answer(false, true, None)).await.unwrap();

    app.dispatch(Intent::Calendar(CalendarIntent::RespondToInvitation {
        message: MessageRef::from_parts("work", "invite-1".to_owned()).unwrap(),
        response: RsvpAnswer::Accepted,
        comment: None,
        notify_organizer: true,
    }))
    .await
    .unwrap();

    assert!(
        app.calendar().reply_prompts.is_empty(),
        "the account said never email; offering it again would not be a question"
    );
    assert!(
        submits.lock().unwrap().is_empty(),
        "nothing was mailed on the account's standing refusal"
    );
}

#[tokio::test]
async fn the_subject_the_host_composed_rides_the_reply() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .failing_reply_delivery()
        .holding(meeting("evt-1", "info@test.local"));
    let submits = Arc::clone(&fake.submits);
    let app = prompted_app(fake, &surfaces).await;

    app.dispatch(answer(
        true,
        false,
        Some("Zusage: Sprint planning".to_owned()),
    ))
    .await
    .unwrap();

    let sent = submits.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].subject, "Zusage: Sprint planning",
        "the host's words replaced the derived subject, not prefixed them"
    );
    assert!(
        sent[0]
            .calendar
            .as_ref()
            .is_some_and(|calendar| calendar.ical.contains("METHOD:REPLY")),
        "the iTIP part is unchanged by the subject"
    );
}

#[tokio::test]
async fn an_imip_reply_that_cannot_be_sent_surfaces_a_failed_row() {
    // The client route, where the reply is the only telling: the answer
    // itself lands, the mail cannot go out, and the surface sees the failure
    // rather than believing an organiser who has heard nothing.
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let fake = CalendarFake::with_calendar_and_mail()
        .without_scheduling()
        .holding(meeting("evt-1", "info@test.local"));
    *fake.fail_submits.lock().unwrap() = 1;
    let rsvps = Arc::clone(&fake.rsvps);
    let app = calendar_app(vec![account("work", fake)], &surfaces);
    app.dispatch(Intent::RefreshMail).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::SetRange {
        start: "2026-07-27T00:00:00Z".parse().unwrap(),
        days: 7,
        view: None,
    }))
    .await
    .unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    app.dispatch(Intent::Calendar(CalendarIntent::RespondToInvitation {
        message: MessageRef::from_parts("work", "invite-1".to_owned()).unwrap(),
        response: RsvpAnswer::Accepted,
        comment: None,
        notify_organizer: true,
    }))
    .await
    .unwrap();

    assert_eq!(rsvps.lock().unwrap().len(), 1, "the answer itself landed");
    let states: Vec<_> = app
        .calendar()
        .write_states
        .iter()
        .map(|row| row.state)
        .collect();
    assert_eq!(states, [CalendarWriteState::FailedWithRetry]);
}
