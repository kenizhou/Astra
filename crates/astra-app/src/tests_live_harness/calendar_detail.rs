// SPDX-License-Identifier: GPL-3.0-only
//! The gated proof of the event detail read against the live harness: one
//! seeded event carrying a full guest list (an organiser who also holds the
//! chair seat, attendees across the role and response words, and more lines
//! than the roll-call's cap) and a weekly rule with a day set and a count
//! bound, both crossed by the product's own sync and read back through the
//! detail read the docked pane rides. Every participant address sits on a
//! domain the harness does not host, so seeding arms none of the server's
//! scheduling: the event this run reads is the event it seeded, nothing
//! deposits anywhere, and the withdrawal matches only this run's artifacts.

use astra_viewmodel::{Participation, RuleEndRead};
use engine_api::{Frequency, Weekday};

use crate::{CalendarIntent, Intent};

use super::calendar_proof::{
    dav, event_ics, event_of_account, range, rows_of, run_token, suite_apps,
};
use super::harness;

#[tokio::test]
async fn the_detail_read_renders_the_roll_call_and_the_rule_sentence_live() {
    let Some((host, port, user, password)) = harness() else {
        eprintln!("skipping live harness test: ASTRA_LIVE_HARNESS is not 1");
        return;
    };
    let token = run_token();
    let title = format!("Harness roll call {token}");
    let uid = format!("rollcall-{token}@astra.local");
    let organiser = format!("chief-{token}@astra.local");
    // Ten participants: the organiser (merged with her own chair attendee
    // line, the self-organised meeting) plus nine attendees across the role
    // and response words, two more lines than the roll-call's cap of eight.
    let extra: Vec<String> = [
        format!("ORGANIZER;CN=Chief Planner:mailto:{organiser}"),
        format!("ATTENDEE;CN=Chief Planner;ROLE=CHAIR;PARTSTAT=ACCEPTED:mailto:{organiser}"),
        format!("ATTENDEE;CN=Kim Plain;RSVP=TRUE:mailto:kim-{token}@astra.local"),
        format!("ATTENDEE;CN=Ada Chair;ROLE=CHAIR;PARTSTAT=TENTATIVE:mailto:ada-{token}@astra.local"),
        format!(
            "ATTENDEE;CN=Sam Optional;ROLE=OPT-PARTICIPANT;PARTSTAT=DECLINED:mailto:sam-{token}@astra.local"
        ),
        format!(
            "ATTENDEE;CN=Dee Info;ROLE=NON-PARTICIPANT;PARTSTAT=ACCEPTED:mailto:dee-{token}@astra.local"
        ),
        format!("ATTENDEE;CN=Eve Accepted;PARTSTAT=ACCEPTED:mailto:eve-{token}@astra.local"),
        format!("ATTENDEE;CN=Fay Tentative;PARTSTAT=TENTATIVE:mailto:fay-{token}@astra.local"),
        format!("ATTENDEE;CN=Gil Declined;PARTSTAT=DECLINED:mailto:gil-{token}@astra.local"),
        format!("ATTENDEE;CN=Hal Held:mailto:hal-{token}@astra.local"),
        format!("ATTENDEE;CN=Ida Folded;PARTSTAT=ACCEPTED:mailto:ida-{token}@astra.local"),
        "RRULE:FREQ=WEEKLY;BYDAY=TU,TH;COUNT=6".to_owned(),
    ]
    .into();
    let extra: Vec<&str> = extra.iter().map(String::as_str).collect();
    assert_eq!(
        dav(
            "PUT",
            &user,
            &uid,
            &password,
            &event_ics(
                &uid,
                &title,
                "DTSTART:20261103T090000Z",
                "DTEND:20261103T093000Z",
                &extra,
            ),
        ),
        201,
        "the seeding PUT stores the event"
    );

    let (_mail, calendar, first, _second) = suite_apps(&host, &port, &user, &password).await;
    calendar.dispatch(range()).await.unwrap();
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();

    // The sync leg first: the rule expands to its six counted occurrences
    // inside the proof window, so the series the detail words is the one the
    // grid draws.
    let snapshot = calendar.calendar();
    let rows = rows_of(&snapshot, &title);
    assert_eq!(rows.len(), 6, "the weekly rule renders six occurrences");
    assert!(
        rows.iter().all(|row| row.recurring),
        "every row of the series carries the recurrence marker"
    );

    // The detail read: the roll-call and the full rule, both from the master
    // the product's own targeted read names.
    let detail = calendar
        .calendar_event_detail(&event_of_account(&calendar, &first, &title))
        .await
        .expect("the seeded event's detail");
    assert_eq!(
        detail
            .participants
            .iter()
            .map(|row| (
                row.name_or_address.as_str(),
                row.role.as_deref(),
                row.response
            ))
            .collect::<Vec<_>>(),
        vec![
            ("Chief Planner", Some("organiser"), Participation::Accepted),
            ("Kim Plain", None, Participation::NeedsAction),
            ("Ada Chair", Some("chair"), Participation::Tentative),
            ("Sam Optional", Some("optional"), Participation::Declined),
            ("Dee Info", Some("informational"), Participation::Accepted),
            ("Eve Accepted", None, Participation::Accepted),
            ("Fay Tentative", None, Participation::Tentative),
            ("Gil Declined", None, Participation::Declined),
        ],
        "the organiser stands first with her merged response, the role and \
         response words ride their seats, and the plain attendee carries no \
         role word"
    );
    assert_eq!(
        detail.folded_participants, 2,
        "ten participants minus the eight the roll-call shows"
    );

    let rule = detail
        .rule
        .expect("a rule-carrying series crosses its rule");
    assert_eq!(rule.frequency, Frequency::Weekly);
    assert_eq!(rule.interval, 1);
    assert_eq!(rule.days, vec![Weekday::Tu, Weekday::Th]);
    assert_eq!(rule.end, Some(RuleEndRead::AfterCount(6)));
    assert_eq!(
        rule.sentence, "Repeats every week on Tuesdays and Thursdays, six times.",
        "the sentence words the rule in full over the live wire"
    );

    // The withdrawal: the seed leaves the window again.
    assert_eq!(
        dav("DELETE", &user, &uid, &password, ""),
        204,
        "the seeding cleanup withdraws the event"
    );
    calendar
        .dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    assert!(
        rows_of(&calendar.calendar(), &title).is_empty(),
        "the withdrawn event renders nowhere after one refresh"
    );
}
