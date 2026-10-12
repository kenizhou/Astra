// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the repeat-rule editability round trip: the rules the editor
//! can describe in full against the ones it must never rewrite, under the
//! week start the setting names. Split from the edit tests beside it for the
//! 500-line cap, the established pattern.

use engine_api::{Frequency, RecurrenceBound, RecurrenceEdit, RecurrenceRule, Weekday};

use super::{build_patch, rule_is_editable, CalendarEditError};
use crate::calendar_fakes::{local, utc, weekly_event};
use crate::protocol::{EventEdit, RecurrenceChange, RepeatEnd, RepeatRule};

/// A fixed stamp, so the patch assertions are deterministic.
fn stamp() -> engine_api::UtcDateTime {
    utc("2026-10-19T10:00:00Z")
}

#[test]
fn a_stored_rule_too_rich_for_the_editor_is_never_replaced() {
    let mut event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    let mut rich = RecurrenceRule::new(Frequency::Monthly);
    rich.by_month_day = vec![15];
    event.recurrence = Some(engine_api::Recurrence::from_rule(rich));
    // A `Set` writes the editor's partial picture back over a rule whose
    // parts the editor never saw: refused.
    let replacement = RepeatRule {
        frequency: Frequency::Weekly,
        interval: 1,
        weekdays: Vec::new(),
        end: RepeatEnd::Never,
    };
    let edit = EventEdit {
        recurrence: Some(RecurrenceChange::Set(replacement)),
        ..EventEdit::default()
    };
    assert_eq!(
        build_patch(&event, &edit, stamp(), Weekday::Mo).unwrap_err(),
        CalendarEditError::RuleTooRich
    );
    // A `Clear` is total rather than partial, so it is allowed even on a rich
    // rule: nothing it drops is something the editor misdescribes.
    let edit = EventEdit {
        recurrence: Some(RecurrenceChange::Clear),
        ..EventEdit::default()
    };
    assert!(build_patch(&event, &edit, stamp(), Weekday::Mo).is_ok());
    // A rule the editor can describe in full is replaceable, and no rule at
    // all (a one-off gaining one) is too.
    assert!(rule_is_editable(None, Weekday::Mo));
    assert!(rule_is_editable(
        weekly_event("evt-x", "Standup", "2026-10-19T09:00:00")
            .recurrence
            .as_ref(),
        Weekday::Mo
    ));
}

#[test]
fn a_wkst_the_editor_would_move_degrades_rather_than_silently_rebuilding() {
    // The round trip is the guard itself: the projection carries no week
    // start, so a rule that states one the editor would not write back comes
    // back different, does not match, and stays read-only. Under the default
    // week start that is a rule anchored to Sunday.
    let mut event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    event.recurrence.as_mut().unwrap().rules[0].first_day_of_week = Weekday::Su;
    assert!(
        !rule_is_editable(event.recurrence.as_ref(), Weekday::Mo),
        "editing it would rewrite the rule with the default week start, \
         changing which days a weekly rule produces"
    );

    // The same refusal on the write path: a `Set` over it never builds.
    let edit = EventEdit {
        recurrence: Some(RecurrenceChange::Set(RepeatRule {
            frequency: Frequency::Weekly,
            interval: 1,
            weekdays: Vec::new(),
            end: RepeatEnd::Never,
        })),
        ..EventEdit::default()
    };
    assert_eq!(
        build_patch(&event, &edit, stamp(), Weekday::Mo).unwrap_err(),
        CalendarEditError::RuleTooRich
    );
}

#[test]
fn a_wkst_the_setting_names_stays_editable() {
    // The same rule, under the setting that names its anchor: the editor
    // would write it back identically, so it is editable, and a save does
    // build. A rule that names its weekdays never carries a week start, so
    // one anchored differently stays read-only whatever the setting says.
    let mut event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    event.recurrence.as_mut().unwrap().rules[0].first_day_of_week = Weekday::Su;
    assert!(rule_is_editable(event.recurrence.as_ref(), Weekday::Su));

    let edit = EventEdit {
        recurrence: Some(RecurrenceChange::Set(RepeatRule {
            frequency: Frequency::Weekly,
            interval: 1,
            weekdays: Vec::new(),
            end: RepeatEnd::Never,
        })),
        ..EventEdit::default()
    };
    let (_, patch) = build_patch(&event, &edit, stamp(), Weekday::Su).unwrap();
    let built = match patch.recurrence_edit().expect("the Set built a rule") {
        RecurrenceEdit::Set(built) => built.as_ref(),
        RecurrenceEdit::Clear => panic!("a Set never clears"),
    };
    assert_eq!(built.rule.first_day_of_week, Weekday::Su);

    let mut named = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    named.recurrence.as_mut().unwrap().rules[0].by_day = vec![engine_api::NDay {
        day: Weekday::Tu,
        nth_of_period: None,
    }];
    named.recurrence.as_mut().unwrap().rules[0].first_day_of_week = Weekday::Su;
    assert!(
        !rule_is_editable(named.recurrence.as_ref(), Weekday::Su),
        "a rule the editor rebuilds with the engine's default start, because \
         it names its weekdays, refuses when the stored rule states another"
    );
}

#[test]
fn an_exrule_carrying_set_degrades_to_read_only() {
    // A rule whose instances are subtracted is a set an editor built from one
    // frequency cannot state, whatever the two rules look like alone.
    let mut event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    event
        .recurrence
        .as_mut()
        .unwrap()
        .excluded_rules
        .push(RecurrenceRule::new(Frequency::Daily));
    assert!(!rule_is_editable(event.recurrence.as_ref(), Weekday::Mo));
}

#[test]
fn a_bound_the_editor_cannot_state_exactly_degrades_to_read_only() {
    // The editor's date control states "the last day that recurs"; a stored
    // `UNTIL` at mid-day is a finer statement than that, and rewriting it to
    // the day's final second would silently add every occurrence that
    // afternoon. The round trip refuses instead: the projection loses the
    // clock, the rebuild puts the final second back, and they differ.
    let mut event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    event.recurrence.as_mut().unwrap().rules[0].bound =
        RecurrenceBound::Until(local("2026-11-03T12:00:00"));
    assert!(!rule_is_editable(event.recurrence.as_ref(), Weekday::Mo));

    // The one bound the editor does state exactly: the final second of the
    // named day, the same rule the create path writes.
    event.recurrence.as_mut().unwrap().rules[0].bound =
        RecurrenceBound::Until(local("2026-11-03T23:59:59"));
    assert!(rule_is_editable(event.recurrence.as_ref(), Weekday::Mo));
}

#[test]
fn a_repeat_measured_in_hours_degrades_to_read_only() {
    let mut event = weekly_event("evt-1", "Standup", "2026-10-19T09:00:00");
    event.recurrence.as_mut().unwrap().rules[0].frequency = Frequency::Hourly;
    assert!(
        !rule_is_editable(event.recurrence.as_ref(), Weekday::Mo),
        "a repeat in hours is not a calendar repeat any editor offers"
    );
}
