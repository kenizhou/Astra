// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the event detail read: the participants roll-call (the
//! organiser first, every word the seats carry, the cap folding a long tail
//! into a count) and the full repeating rule with its read-back sentence,
//! both over the fakes through the targeted master read the snapshot rebuild
//! rides.

use std::sync::{Arc, Mutex};

use astra_viewmodel::{Participation, RuleEndRead};

use crate::calendar_fakes::{
    account, calendar_app, crowded_meeting, every_third_week_series, exotic_series,
    roll_call_meeting, utc, zoned_event, CalendarFake,
};
use crate::{reference::EventRef, CalendarIntent, Intent, Surface};

/// The fortnight the diary fixtures live in, so the refresh syncs the events
/// into the store before the detail read asks for them.
fn week() -> Intent {
    Intent::Calendar(CalendarIntent::SetRange {
        start: utc("2026-10-18T00:00:00Z"),
        days: 14,
        view: None,
    })
}

/// Boots the app over `fake` and lands the window plus a refresh, the prefix
/// every detail test reads.
async fn synced(
    fake: CalendarFake,
    surfaces: &Arc<Mutex<Vec<Surface>>>,
) -> crate::App<CalendarFake> {
    let app = calendar_app(vec![account("work", fake)], surfaces);
    app.dispatch(week()).await.unwrap();
    app.dispatch(Intent::Calendar(CalendarIntent::Refresh))
        .await
        .unwrap();
    app
}

/// The detail of the event keyed `key`, the read every test asserts on.
async fn detail_of(
    app: &crate::App<CalendarFake>,
    key: &str,
) -> Option<astra_viewmodel::EventDetail> {
    app.calendar_event_detail(&EventRef::from_parts("work", key.to_owned()).unwrap())
        .await
}

#[tokio::test]
async fn the_detail_read_carries_the_roll_call_with_the_organiser_first() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = synced(
        CalendarFake::with_calendar_and_mail().holding(roll_call_meeting("evt-r")),
        &surfaces,
    )
    .await;

    let detail = detail_of(&app, "evt-r").await.expect("the meeting read");
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
            ("Boss", Some("organiser"), Participation::Accepted),
            ("Kim", None, Participation::NeedsAction),
            ("Ada", Some("chair"), Participation::Tentative),
            ("Sam", Some("optional"), Participation::Declined),
            (
                "digest@test.local",
                Some("informational"),
                Participation::NeedsAction
            ),
        ],
        "the organiser stands first whatever order the event stores its lines in, \
         the plain attendee carries no role word, and a line with no name shows \
         its address"
    );
    assert_eq!(
        detail.folded_participants, 0,
        "a roll-call that fits crosses no folded tail"
    );
    // The line with neither a name nor an address renders nothing and counts
    // nowhere: there is no person to show for it.
    assert_eq!(detail.participants.len(), 5);
}

#[tokio::test]
async fn the_roll_call_folds_a_long_tail_into_a_count() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = synced(
        CalendarFake::with_calendar_and_mail().holding(crowded_meeting("evt-crowd", 12)),
        &surfaces,
    )
    .await;

    let detail = detail_of(&app, "evt-crowd")
        .await
        .expect("the meeting read");
    assert_eq!(
        detail.participants.len(),
        8,
        "the roll-call caps at eight lines, the size a docked pane reads at a glance"
    );
    assert_eq!(
        detail.participants[0].name_or_address, "Boss",
        "the fold keeps the organiser's line"
    );
    assert_eq!(
        detail.folded_participants, 5,
        "thirteen participants minus the eight shown"
    );
}

#[tokio::test]
async fn the_detail_read_carries_the_full_rule_with_its_sentence() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = synced(
        CalendarFake::with_calendar_and_mail().holding(every_third_week_series("evt-rule")),
        &surfaces,
    )
    .await;

    let detail = detail_of(&app, "evt-rule").await.expect("the series read");
    let rule = detail.rule.expect("a repeating series crosses its rule");
    assert_eq!(rule.frequency, engine_api::Frequency::Weekly);
    assert_eq!(rule.interval, 3);
    assert_eq!(
        rule.days,
        vec![engine_api::Weekday::Mo, engine_api::Weekday::We]
    );
    assert_eq!(rule.end, Some(RuleEndRead::AfterCount(10)));
    assert_eq!(
        rule.sentence,
        "Repeats every third week on Mondays and Wednesdays, ten times."
    );
}

#[tokio::test]
async fn an_exotic_rule_crosses_the_honest_bare_shape() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = synced(
        CalendarFake::with_calendar_and_mail().holding(exotic_series("evt-exotic")),
        &surfaces,
    )
    .await;

    let detail = detail_of(&app, "evt-exotic")
        .await
        .expect("the series read");
    let rule = detail.rule.expect("the cadence still crosses");
    assert_eq!(rule.sentence, "Repeats every month.");
    assert!(
        rule.days.is_empty(),
        "a day set the words refuse crosses none"
    );
    assert_eq!(rule.end, None, "no end is promised the rule does not word");
}

#[tokio::test]
async fn a_one_off_crosses_no_rule_and_no_roll_call() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = synced(
        CalendarFake::with_calendar_and_mail().holding(zoned_event(
            "evt-own",
            "Dentist",
            "2026-10-21T09:00:00",
        )),
        &surfaces,
    )
    .await;

    let detail = detail_of(&app, "evt-own")
        .await
        .expect("the appointment read");
    assert_eq!(detail.rule, None, "a one-off carries no rule to word");
    assert!(detail.participants.is_empty());
    assert_eq!(detail.folded_participants, 0);
}

#[tokio::test]
async fn an_event_the_store_does_not_hold_answers_none() {
    let surfaces = Arc::new(Mutex::new(Vec::new()));
    let app = synced(CalendarFake::with_calendar_and_mail(), &surfaces).await;

    assert!(detail_of(&app, "evt-absent").await.is_none());
}
