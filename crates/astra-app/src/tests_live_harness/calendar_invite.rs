// SPDX-License-Identifier: GPL-3.0-only
//! The invitation machine's live proof, against the running harness: the
//! organiser's seeded event makes Stalwart's scheduling deposit the
//! attendee's calendar copy and mail the invitation, the
//! attendee-submitted invitation mail classifies through the reading
//! block, and the answer rides the server scheduling route Stalwart
//! serves.
//!
//! **The route, pinned by observation.** Stalwart advertises RFC 6638
//! auto-scheduling, so the product's invite machine takes the server
//! route: storing the answer IS sending it, the receipt reports nothing
//! about the reply's delivery (`NotReported`, silence, never a failure),
//! and no reply-delivery-failure prompt is raised. The proof pins that
//! route the only way it can be pinned: the capability fact the machine
//! read, the answer standing on the attendee's calendar, the empty prompt
//! list, and the organiser's own copy of the meeting carrying the
//! accepted participation after the server delivered the reply.
//!
//! The cleanup withdraws both calendar copies through the product's
//! delete intent and sweeps both mailboxes of every subject carrying the
//! run token (the run's own mail and the server's scheduling mails
//! alike), bounded-waiting until none survives because the server mints
//! its mails behind the deletes, so the harness keeps the shape the mail
//! suites pin. The iTIP deposits in the server's scheduling inboxes no
//! product surface reads are the one residue a run cannot withdraw.

use astra_viewmodel::{InvitationKind, Participation, RowKey};
use engine_api::{Draft, DraftCalendar, EmailAddress, MailEdit, Provider, ScheduleMethod};

use crate::protocol::RsvpAnswer;
use crate::{App, CalendarIntent, Intent, MessageRef};

use super::calendar_proof::{
    dav, event_of_account, range, request_ics, rows_of, run_token, suite_apps, until_account_rows,
};
use super::proof;
use super::{harness, inbox_key, row_by_subject, FolderRef};

/// Deletes every message whose subject carries `token` from `account`
/// through the engine's own outbox-mediated delete: the sweep that
/// withdraws a run's invitation mails (its own and the server's), so the
/// mail suites' pinned shapes survive the calendar proofs.
async fn sweep_subjects(
    app: &App<Box<dyn Provider>>,
    account: &engine_api::AccountId,
    token: &str,
) {
    let handle = app
        .account_handle(account)
        .await
        .expect("the account the sweep deletes through");
    let provider = handle.providers.first().expect("a provider to delete on");
    let targets: Vec<_> = app
        .engine
        .messages(account)
        .await
        .expect("the account's stored messages")
        .into_iter()
        .filter(|message| {
            message
                .envelope
                .subject
                .as_deref()
                .is_some_and(|subject| subject.contains(token))
        })
        .map(|message| message.id.key().clone())
        .collect();
    for target in targets {
        app.engine
            .edit_mail(
                provider,
                account,
                &proof::cleanup_idempotency(),
                &MailEdit::Delete { target },
            )
            .await
            .expect("the sweep delete");
    }
}

/// Whether `account` still holds any mail whose subject carries `token`:
/// the sweep's residue check, read through the engine's own store.
async fn token_mail_left(
    app: &App<Box<dyn Provider>>,
    account: &engine_api::AccountId,
    token: &str,
) -> bool {
    app.engine
        .messages(account)
        .await
        .unwrap_or_default()
        .into_iter()
        .any(|message| {
            message
                .envelope
                .subject
                .as_deref()
                .is_some_and(|text| text.contains(token))
        })
}

/// The whole invitation flow live: deposit, classification, the answer,
/// the route, the organiser hearing it, and the cleanup.
#[tokio::test]
async fn the_invitation_answers_over_the_route_stalwart_serves() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let summary = format!("Harness proof meeting {token}");
    let uid = format!("rsvp-{token}@astra.local");
    let subject = format!("Harness invitation {token}");
    let request = request_ics(
        &uid,
        &summary,
        "DTSTART:20261104T150000Z",
        "DTEND:20261104T160000Z",
        "astra2@example.test",
        "astra1@example.test",
    );
    let (mail, calendar, first, second) = suite_apps(&host, &port, &user, &password).await;
    mail.dispatch(Intent::RefreshMail).await.unwrap();

    // The organiser's copy: seeded straight onto astra2's calendar, which
    // is what arms Stalwart's scheduling for the named attendee.
    assert_eq!(
        dav("PUT", proof::RECIPIENT, &uid, &password, &request),
        201,
        "the organiser's event is seeded"
    );
    // The invitation mail: submitted through the engine's own SMTP leg, a
    // REQUEST body part carried as iMIP, from the organiser to astra1.
    let handle = mail
        .account_handle(&second)
        .await
        .expect("the organiser's mail account");
    let provider = handle.providers.first().expect("the submission provider");
    let draft = Draft::new(
        crate::drafts::mint_message_id().expect("a fresh message id"),
        EmailAddress::new(proof::RECIPIENT),
        vec![EmailAddress::new(&user)],
        subject.clone(),
        "The invitation this proof answers.",
    )
    .with_calendar(DraftCalendar::new(ScheduleMethod::Request, request));
    mail.engine
        .submit_mail(provider, &second, &draft)
        .await
        .expect("the invitation mail submits");

    // The deposit: astra1's own calendar gains the server's copy of the
    // meeting, read back through the product's sync and snapshot. The wait
    // is scoped to the attendee: the organiser's own copy renders the same
    // title the moment it is seeded.
    calendar.dispatch(range()).await.unwrap();
    until_account_rows(&calendar, &first, &summary).await;

    // The reading block classifies: the attendee-submitted mail carries
    // the invitation card the two-condition gate owes.
    mail.dispatch(Intent::RefreshMail).await.unwrap();
    let inbox = FolderRef::from_parts(first.as_str(), inbox_key(&mail, &first))
        .expect("the attendee's INBOX reference");
    mail.dispatch(Intent::SelectFolder { folder: inbox })
        .await
        .unwrap();
    let snapshot = mail.mailbox_list();
    let row = row_by_subject(&snapshot, &subject);
    let RowKey::Message { key, .. } = row.key.clone() else {
        panic!("a flat list row is keyed by its message");
    };
    mail.dispatch(Intent::OpenMessage {
        message: MessageRef {
            account: first.clone(),
            key,
        },
    })
    .await
    .unwrap();
    let reading = mail.reading().expect("the open publishes a reading view");
    let invitation = reading
        .invitation
        .as_ref()
        .expect("the card rides the body");
    assert_eq!(invitation.kind, InvitationKind::Rsvp);
    assert_eq!(invitation.response, Participation::NeedsAction);
    assert_eq!(invitation.summary, summary);
    assert_eq!(invitation.organizer, "astra2@example.test");

    // The answer, through the intent: on the server route Stalwart serves,
    // the capability fact the machine read before it wrote.
    let caps = calendar
        .account_handle(&first)
        .await
        .expect("the attendee's calendar account")
        .providers
        .first()
        .expect("the calendar provider")
        .connection_info()
        .capabilities;
    assert!(
        caps.calendar_scheduling(),
        "Stalwart advertises RFC 6638 auto-scheduling, so the server route is the one this harness serves"
    );
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::RespondToInvitation {
            message: MessageRef {
                account: first.clone(),
                key: reading.message.key.clone(),
            },
            response: RsvpAnswer::Accepted,
            comment: None,
            notify_organizer: true,
        }))
        .await
        .unwrap();
    let snapshot = calendar.calendar();
    assert!(
        snapshot.write_states.is_empty(),
        "the answer settled: no write row stands"
    );
    assert!(
        snapshot.reply_prompts.is_empty(),
        "the server route reported no delivery failure, so no question was raised"
    );
    // Scoped to the attendee: the organiser holds her own copy of the same
    // meeting, and the snapshot renders both accounts' calendars.
    let attendee_rows: Vec<_> = rows_of(&snapshot, &summary)
        .into_iter()
        .filter(|row| row.account == first)
        .collect();
    assert_eq!(
        attendee_rows.len(),
        1,
        "the answered meeting still renders on the attendee's calendar"
    );
    assert_eq!(
        attendee_rows[0].participation,
        Participation::Accepted,
        "the answer stands on the attendee's calendar"
    );

    // The organiser heard: astra2's own copy of the meeting carries
    // astra1's acceptance, read through the engine's targeted master read
    // (the same read the brief drive rides).
    let organiser_key = event_of_account(&calendar, &second, &summary).key;
    let deadline = std::time::Instant::now() + super::calendar_proof::DELIVERY_TIMEOUT;
    loop {
        calendar
            .dispatch(Intent::Calendar(CalendarIntent::Refresh))
            .await
            .unwrap();
        let accepted = calendar
            .engine
            .events_by_keys(&second, std::slice::from_ref(&organiser_key))
            .await
            .unwrap_or_default()
            .into_iter()
            .flat_map(|event| event.participants)
            .any(|participant| {
                participant.email.as_deref() == Some(user.as_str())
                    && participant.participation_status.as_str() == "accepted"
            });
        if accepted {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the organiser's copy never learned the acceptance within the delivery timeout"
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    // The cleanup: both calendar copies through the product's delete, the
    // organiser's first because her copy is what arms the server's
    // scheduling (deleting the attendee's copy while the organiser's still
    // stands lets the scheduling inbox re-deposit it), then both mailboxes
    // swept of every subject carrying the run token, the deletes landed
    // and observed so a rerun starts from the harness's shape.
    for account in [&second, &first] {
        calendar
            .dispatch(Intent::Calendar(CalendarIntent::DeleteEvent {
                event: event_of_account(&calendar, account, &summary),
                occurrence: None,
            }))
            .await
            .unwrap();
    }
    // The bounded mail sweep: the server's scheduling mints mails behind the
    // deletes (the invitation itself, the organiser's copies, the
    // cancellation), so both mailboxes are swept and refreshed until no
    // subject carrying the run token survives, within the delivery timeout.
    // One fixed pass would race the server and leave residue that breaks
    // the mail suites' whole-INBOX pins.
    let deadline = std::time::Instant::now() + super::calendar_proof::DELIVERY_TIMEOUT;
    loop {
        mail.dispatch(Intent::RefreshMail).await.unwrap();
        sweep_subjects(&mail, &first, &token).await;
        sweep_subjects(&mail, &second, &token).await;
        mail.dispatch(Intent::RefreshMail).await.unwrap();
        let residue = token_mail_left(&mail, &first, &token).await
            || token_mail_left(&mail, &second, &token).await;
        if !residue {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "mail carrying the run token kept arriving within the delivery timeout"
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    // The bounded withdrawal wait: the server's scheduling runs behind the
    // deletes, so the calendars are polled empty rather than asserted empty
    // after one refresh.
    let deadline = std::time::Instant::now() + super::calendar_proof::DELIVERY_TIMEOUT;
    loop {
        calendar
            .dispatch(Intent::Calendar(CalendarIntent::Refresh))
            .await
            .unwrap();
        let snapshot = calendar.calendar();
        if rows_of(&snapshot, &summary).is_empty() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the meeting was not withdrawn from every calendar within the delivery timeout"
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    for account in [&first, &second] {
        assert!(
            !token_mail_left(&mail, account, &token).await,
            "no mail carrying the run token survives"
        );
    }
}
