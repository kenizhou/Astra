// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the rule read-back words: the full sentences over the supported
//! vocabulary (the interval, the day set on a weekly rule, the end in count
//! or date words) and the honest bare interval-and-frequency a rule outside
//! the vocabulary falls back to, never a sentence the rule does not make
//! true.

use std::num::{NonZeroI32, NonZeroU32};

use astra_viewmodel::{RuleEndRead, RuleRead};
use engine_api::{Frequency, NDay, Recurrence, RecurrenceBound, RecurrenceRule, Weekday};

use super::rule_read;

/// The words for one single-rule recurrence, the shape every sentence test
/// starts from.
fn worded(rule: RecurrenceRule) -> Option<RuleRead> {
    rule_read(&Recurrence::from_rule(rule))
}

/// The sentence alone, unwrapped with the test's own message.
fn sentence(rule: RecurrenceRule) -> String {
    worded(rule).expect("a single rule always words").sentence
}

#[test]
fn the_full_sentence_words_interval_days_and_end() {
    let mut rule = RecurrenceRule::new(Frequency::Weekly);
    rule.interval = NonZeroU32::new(3).unwrap();
    // Stored Wednesday before Monday: the read crosses the day set in the
    // canonical Monday-first order whatever order the rule names.
    rule.by_day = vec![
        NDay {
            day: Weekday::We,
            nth_of_period: None,
        },
        NDay {
            day: Weekday::Mo,
            nth_of_period: None,
        },
    ];
    rule.bound = RecurrenceBound::Count(NonZeroU32::new(10).unwrap());
    let read = worded(rule).expect("the vocabulary serves this rule");
    assert_eq!(read.frequency, Frequency::Weekly);
    assert_eq!(read.interval, 3);
    assert_eq!(read.days, vec![Weekday::Mo, Weekday::We]);
    assert_eq!(read.end, Some(RuleEndRead::AfterCount(10)));
    assert_eq!(
        read.sentence,
        "Repeats every third week on Mondays and Wednesdays, ten times."
    );
}

#[test]
fn interval_one_needs_no_ordinal_and_no_end_crosses() {
    let read = worded(RecurrenceRule::new(Frequency::Daily)).expect("the simplest rule words");
    assert_eq!(read.sentence, "Repeats every day.");
    assert!(
        read.days.is_empty(),
        "a rule that names no day set crosses none"
    );
    assert_eq!(read.end, None, "an unbounded rule crosses no end");
}

#[test]
fn the_end_crosses_in_date_words() {
    let mut rule = RecurrenceRule::new(Frequency::Weekly);
    rule.bound = RecurrenceBound::Until("2026-11-30T23:59:59".parse().unwrap());
    let read = worded(rule).expect("the editor's own until shape words");
    assert_eq!(read.sentence, "Repeats every week until 30 November 2026.");
    assert_eq!(
        read.end,
        Some(RuleEndRead::UntilDate("2026-11-30".parse().unwrap()))
    );
}

#[test]
fn a_until_clock_that_is_not_the_final_second_keeps_its_time() {
    // A foreign rule can bound its series mid-day; wording the date alone
    // would promise an evening occurrence the bound refuses.
    let mut rule = RecurrenceRule::new(Frequency::Weekly);
    rule.bound = RecurrenceBound::Until("2026-11-30T09:00:00".parse().unwrap());
    assert_eq!(
        sentence(rule),
        "Repeats every week until 30 November 2026 at 09:00."
    );
}

#[test]
fn a_until_clock_whose_seconds_are_not_zero_words_them() {
    // A bound inside a minute never reads as the whole minute: the seconds
    // print whenever they are not zero, so the words promise nothing the
    // rule does not serve.
    let mut rule = RecurrenceRule::new(Frequency::Weekly);
    rule.bound = RecurrenceBound::Until("2026-11-30T09:00:30".parse().unwrap());
    assert_eq!(
        sentence(rule),
        "Repeats every week until 30 November 2026 at 09:00:30."
    );
}

#[test]
fn a_until_clock_inside_the_final_minute_names_its_seconds() {
    // 23:59:30 cut the day's last half minute: "at 23:59" would read as the
    // end of the day, so the seconds stand beside the minute.
    let mut rule = RecurrenceRule::new(Frequency::Weekly);
    rule.bound = RecurrenceBound::Until("2026-11-30T23:59:30".parse().unwrap());
    assert_eq!(
        sentence(rule),
        "Repeats every week until 30 November 2026 at 23:59:30."
    );
}

#[test]
fn a_large_interval_falls_back_to_digits_and_so_does_a_large_count() {
    let mut rule = RecurrenceRule::new(Frequency::Weekly);
    rule.interval = NonZeroU32::new(13).unwrap();
    rule.bound = RecurrenceBound::Count(NonZeroU32::new(25).unwrap());
    assert_eq!(sentence(rule), "Repeats every 13 weeks, 25 times.");
}

#[test]
fn the_yearly_monthly_and_hourly_periods_word_their_own_names() {
    assert_eq!(
        sentence(RecurrenceRule::new(Frequency::Yearly)),
        "Repeats every year."
    );
    let mut monthly = RecurrenceRule::new(Frequency::Monthly);
    monthly.interval = NonZeroU32::new(2).unwrap();
    assert_eq!(sentence(monthly), "Repeats every second month.");
    let mut hourly = RecurrenceRule::new(Frequency::Hourly);
    hourly.interval = NonZeroU32::new(2).unwrap();
    assert_eq!(sentence(hourly), "Repeats every second hour.");
}

#[test]
fn an_exotic_rule_falls_back_to_the_bare_interval_and_frequency() {
    // A by-month-day pattern sits outside the vocabulary: the sentence keeps
    // the cadence, drops the day set and the end, and never invents wording
    // for a part it does not serve.
    let mut rule = RecurrenceRule::new(Frequency::Monthly);
    rule.by_month_day = vec![15];
    rule.bound = RecurrenceBound::Count(NonZeroU32::new(8).unwrap());
    let read = worded(rule).expect("a bare sentence still words");
    assert_eq!(read.sentence, "Repeats every month.");
    assert!(read.days.is_empty());
    assert_eq!(read.end, None);
}

#[test]
fn an_nth_weekday_is_outside_the_vocabulary() {
    let mut rule = RecurrenceRule::new(Frequency::Monthly);
    rule.by_day = vec![NDay {
        day: Weekday::Th,
        nth_of_period: Some(NonZeroI32::new(-1).unwrap()),
    }];
    assert_eq!(sentence(rule), "Repeats every month.");
}

#[test]
fn a_day_set_on_a_long_frequency_stays_bare() {
    // "On Tuesdays" reads truly on a weekly rule; on a monthly one the same
    // words would say one Tuesday a month, which is not what a plain BYDAY
    // on a monthly rule means, so the day set drops instead.
    let mut rule = RecurrenceRule::new(Frequency::Monthly);
    rule.by_day = vec![NDay {
        day: Weekday::Tu,
        nth_of_period: None,
    }];
    let read = worded(rule).expect("the cadence still words");
    assert_eq!(read.sentence, "Repeats every month.");
    assert!(read.days.is_empty());
}

#[test]
fn a_recurrence_of_two_rules_words_only_the_first_cadence() {
    let weekly = RecurrenceRule::new(Frequency::Weekly);
    let daily = RecurrenceRule::new(Frequency::Daily);
    let recurrence = Recurrence {
        rules: vec![weekly, daily],
        ..Recurrence::default()
    };
    let read = rule_read(&recurrence).expect("the union still words its first cadence");
    assert_eq!(read.sentence, "Repeats every week.");
    assert!(read.days.is_empty());
    assert_eq!(read.end, None);
}

#[test]
fn an_exclusion_carrying_recurrence_stays_bare() {
    let mut recurrence = Recurrence::from_rule(RecurrenceRule::new(Frequency::Weekly));
    recurrence
        .excluded_rules
        .push(RecurrenceRule::new(Frequency::Daily));
    let read = rule_read(&recurrence).expect("the base cadence still words");
    assert_eq!(read.sentence, "Repeats every week.");
    assert!(read.days.is_empty());
    assert_eq!(read.end, None);
}

#[test]
fn a_recurrence_with_no_rule_at_all_reads_as_none() {
    assert_eq!(rule_read(&Recurrence::default()), None);
}

#[test]
fn a_daily_rule_filtered_to_named_days_hedges_to_the_bare_word() {
    // The engine's expansion drops every day that is not Monday or
    // Wednesday, so the bare cadence head ("repeats every day") would be a
    // sentence the grid beneath it contradicts: the fallback hedges.
    let mut rule = RecurrenceRule::new(Frequency::Daily);
    rule.by_day = vec![
        NDay {
            day: Weekday::Mo,
            nth_of_period: None,
        },
        NDay {
            day: Weekday::We,
            nth_of_period: None,
        },
    ];
    let read = worded(rule).expect("the rule words");
    assert_eq!(read.sentence, "Repeats.");
    assert!(read.days.is_empty(), "no day set crosses a hedged rule");
}

#[test]
fn a_monthly_rule_filtered_to_january_hedges_to_the_bare_word() {
    // Eleven months of the year carry no occurrence at all, so "repeats
    // every month" would be false.
    let mut rule = RecurrenceRule::new(Frequency::Monthly);
    rule.by_month = vec!["1".to_owned()];
    let read = worded(rule).expect("the rule words");
    assert_eq!(read.sentence, "Repeats.");
}
