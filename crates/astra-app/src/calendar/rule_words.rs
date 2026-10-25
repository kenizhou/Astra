// SPDX-License-Identifier: GPL-3.0-only
//! The repeating rule's read-back words: the app-side human formatter that
//! turns the engine's recurrence shapes into the sentence the event detail
//! shows, the house pattern the survey names (the engine carries the rule,
//! the app words it). Pure over the engine's own shapes, present tense, and
//! honest by construction: a rule the vocabulary cannot serve in full words
//! only its interval and frequency, never a sentence the rule does not make
//! true.

use astra_viewmodel::{RuleEndRead, RuleRead};
use engine_api::{
    CalendarDate, Frequency, NDay, Recurrence, RecurrenceBound, RecurrenceRule, Weekday,
};

/// Words one stored recurrence as the detail read crosses it: the rule's own
/// shape beside the sentence. `None` for a recurrence that carries no rule at
/// all (the master start alone), which is not a shape to word.
///
/// A recurrence the vocabulary cannot serve in full (a rule carrying parts
/// beyond frequency, interval, a plain weekly day set, and the bound; a union
/// or an exclusion) crosses its first rule's cadence alone: the day set and
/// the end drop rather than being worded wrongly, and the sentence says no
/// more than the interval and frequency.
pub(crate) fn rule_read(recurrence: &Recurrence) -> Option<RuleRead> {
    let rule = recurrence.rules.first()?;
    let full = words_in_full(recurrence, rule);
    Some(RuleRead {
        frequency: rule.frequency,
        interval: rule.interval.get(),
        days: full.then(|| day_set(rule)).unwrap_or_default(),
        end: full.then(|| end_read(rule)).flatten(),
        sentence: sentence(rule, full),
    })
}

/// Whether the whole recurrence sits inside the wording vocabulary: exactly
/// one rule, nothing subtracted, and that rule carrying nothing beyond the
/// parts the words serve. A recurrence of several rules words its first
/// cadence only, because a disjunction of rules is a sentence this
/// vocabulary cannot make truthfully. The week start never degrades the
/// wording: the sentence claims no anchor week, so whichever day the rule's
/// weeks open on, the words stay true.
fn words_in_full(recurrence: &Recurrence, rule: &RecurrenceRule) -> bool {
    recurrence.rules.len() == 1
        && recurrence.excluded_rules.is_empty()
        && matches!(
            rule.frequency,
            Frequency::Yearly | Frequency::Monthly | Frequency::Weekly | Frequency::Daily
        )
        && rule.rscale.is_none()
        && rule.skip == engine_api::RecurrenceSkip::Omit
        && rule.by_month_day.is_empty()
        && rule.by_month.is_empty()
        && rule.by_year_day.is_empty()
        && rule.by_week_no.is_empty()
        && rule.by_hour.is_empty()
        && rule.by_minute.is_empty()
        && rule.by_second.is_empty()
        && rule.by_set_position.is_empty()
        && rule
            .by_day
            .iter()
            .all(|day| day.nth_of_period.is_none() && rule.frequency == Frequency::Weekly)
}

/// The canonical Monday-first order the day set crosses in.
const DAY_ORDER: [Weekday; 7] = [
    Weekday::Mo,
    Weekday::Tu,
    Weekday::We,
    Weekday::Th,
    Weekday::Fr,
    Weekday::Sa,
    Weekday::Su,
];

/// The rule's day set in the canonical Monday-first order, whatever order the
/// stored rule names its days in.
fn day_set(rule: &RecurrenceRule) -> Vec<Weekday> {
    DAY_ORDER
        .into_iter()
        .filter(|day| {
            rule.by_day
                .iter()
                .any(|named: &NDay| named.day == *day && named.nth_of_period.is_none())
        })
        .collect()
}

/// The rule's end as the read crosses it, in the vocabulary's own terms: a
/// count, or the date the bound's wall clock sits on.
fn end_read(rule: &RecurrenceRule) -> Option<RuleEndRead> {
    match &rule.bound {
        RecurrenceBound::Unbounded => None,
        RecurrenceBound::Count(count) => Some(RuleEndRead::AfterCount(count.get())),
        RecurrenceBound::Until(until) => Some(RuleEndRead::UntilDate(
            CalendarDate::new(until.year(), until.month(), until.day()).ok()?,
        )),
    }
}

/// The sentence: the cadence always, the day set and the end only where the
/// words serve them.
fn sentence(rule: &RecurrenceRule, full: bool) -> String {
    // A digit interval takes the plural period ("every 13 weeks"); the
    // ordinal words carry their own singular ("every third week").
    let period = period_word(rule, rule.interval.get() > 12);
    let head = match rule.interval.get() {
        1 => format!("Repeats every {period}"),
        _ => format!("Repeats every {} {period}", interval_words(rule)),
    };
    if !full {
        return format!("{head}.");
    }
    let days = day_set(rule);
    let mut words = if days.is_empty() {
        head
    } else {
        let names = days
            .iter()
            .map(|day| format!("{}s", weekday_word(*day)))
            .collect::<Vec<_>>()
            .join(", ");
        let joined = match names.rfind(", ") {
            Some(split) => format!("{} and {}", &names[..split], &names[split + 2..]),
            None => names,
        };
        format!("{head} on {joined}")
    };
    match &rule.bound {
        RecurrenceBound::Unbounded => {}
        RecurrenceBound::Count(count) => {
            words = format!(
                "{words}, {}",
                if count.get() == 1 {
                    "one time".to_owned()
                } else {
                    format!("{} times", count_words(count.get()))
                }
            );
        }
        RecurrenceBound::Until(until) => {
            words = format!("{words} until {}", until_words(until));
        }
    }
    format!("{words}.")
}

/// The interval above one as words: the ordinal word through twelfth, digits
/// above (a thirteenth-and-up interval reads faster as a number than as a
/// compound ordinal).
fn interval_words(rule: &RecurrenceRule) -> String {
    match rule.interval.get() {
        2..=12 => ordinal_word(rule.interval.get()).to_owned(),
        above => above.to_string(),
    }
}

/// The ordinal word for 2 through 12.
fn ordinal_word(value: u32) -> &'static str {
    match value {
        2 => "second",
        3 => "third",
        4 => "fourth",
        5 => "fifth",
        6 => "sixth",
        7 => "seventh",
        8 => "eighth",
        9 => "ninth",
        10 => "tenth",
        11 => "eleventh",
        _ => "twelfth",
    }
}

/// The cardinal word for a count through 12, digits above.
fn count_words(value: u32) -> String {
    match value {
        1 => "one".to_owned(),
        2 => "two".to_owned(),
        3 => "three".to_owned(),
        4 => "four".to_owned(),
        5 => "five".to_owned(),
        6 => "six".to_owned(),
        7 => "seven".to_owned(),
        8 => "eight".to_owned(),
        9 => "nine".to_owned(),
        10 => "ten".to_owned(),
        11 => "eleven".to_owned(),
        12 => "twelve".to_owned(),
        above => above.to_string(),
    }
}

/// The frequency as the period it names: a daily rule says "day", an hourly
/// one "hour", whatever the words can serve, pluralised when the interval
/// word beside it is digits.
fn period_word(rule: &RecurrenceRule, plural: bool) -> &'static str {
    match (rule.frequency, plural) {
        (Frequency::Yearly, false) => "year",
        (Frequency::Yearly, true) => "years",
        (Frequency::Monthly, false) => "month",
        (Frequency::Monthly, true) => "months",
        (Frequency::Weekly, false) => "week",
        (Frequency::Weekly, true) => "weeks",
        (Frequency::Daily, false) => "day",
        (Frequency::Daily, true) => "days",
        (Frequency::Hourly, false) => "hour",
        (Frequency::Hourly, true) => "hours",
        (Frequency::Minutely, false) => "minute",
        (Frequency::Minutely, true) => "minutes",
        (Frequency::Secondly, false) => "second",
        (Frequency::Secondly, true) => "seconds",
    }
}

/// The weekday as the sentence names it, capitalised.
fn weekday_word(day: Weekday) -> &'static str {
    match day {
        Weekday::Mo => "Monday",
        Weekday::Tu => "Tuesday",
        Weekday::We => "Wednesday",
        Weekday::Th => "Thursday",
        Weekday::Fr => "Friday",
        Weekday::Sa => "Saturday",
        Weekday::Su => "Sunday",
    }
}

/// The month as the sentence names it, capitalised and in full.
fn month_word(month: u8) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        _ => "December",
    }
}

/// The bound's wall clock as the sentence names it: the date in day month
/// year order, and the clock beside it whenever the bound does not sit on
/// the final second of its own day, because a mid-day bound really does cut
/// that day's later occurrences and the date alone would promise them.
fn until_words(until: &engine_api::LocalDateTime) -> String {
    let date = format!(
        "{} {} {}",
        until.day(),
        month_word(until.month()),
        until.year()
    );
    if until.hour() == 23 && until.minute() == 59 && until.second() == 59 {
        return date;
    }
    format!("{date} at {:02}:{:02}", until.hour(), until.minute())
}

#[cfg(test)]
#[path = "../calendar_rule_words_tests.rs"]
mod tests;
