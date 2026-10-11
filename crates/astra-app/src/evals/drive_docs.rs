// SPDX-License-Identifier: GPL-3.0-only
//! The AI documents scenario drives: the A4 features' arms of the
//! seam-parameter drive. The digest runs its generation over the
//! completion seam and pins, beside the parsed digest, the prompt the
//! service built: the rollup's bucket order and the head limit's shown
//! and omitted subjects. The report pins the structured three-section
//! parse and its assembly into the stored document's text. The split is
//! pure derivation with no seam spend at all: its map compared over the
//! fixture's category words, a word outside the mapping excluded.

use astra_ai::CorrelationId;
use engine_api::{AccountId, ProviderKey};
use serde_json::json;

use crate::ai_docs::{
    digest_prompt, generate_digest, generate_report, split_map, DigestError, DocMessage,
    ReportError, StoredCategories,
};
use crate::ai_rules::AiComplete;
use crate::{generate_meeting_brief, meeting_brief_prompt, BriefMeeting, MeetingBriefError};

use super::drive::{expect, str_field, strings_field};
use super::Scenario;

/// The digest scenario: the fixture's window (messages newest first,
/// the caller's collection order) and stored categorisation through the
/// generation seam over the canned completion, the parsed digest
/// compared as the answer. The prompt the service builds is pinned
/// first, deterministically through the same builder: the rollup's
/// bucket lines in their order (the closed set's order, the
/// uncategorised bucket last), the window's day line, and the head
/// limit's subjects, the shown riding and the older omitted.
pub(super) async fn digest_scenario(
    scenario: &Scenario,
    complete: &dyn AiComplete,
    correlation: &CorrelationId,
) -> Result<(), String> {
    let account = account_field(&scenario.inputs)?;
    let days = number_field(&scenario.inputs, "days")?;
    let messages = doc_messages_field(&scenario.inputs)?;
    let stored = stored_field(&scenario.inputs)?;
    let prompt = digest_prompt(days, &messages, &stored);
    let mut at = 0usize;
    for needle in strings_field(&scenario.expected, "rollup_in_order")? {
        let Some(found) = prompt[at..].find(&needle) else {
            return Err(format!("the rollup dropped {needle:?}:\n{prompt}"));
        };
        at += found + needle.len();
    }
    for needle in strings_field(&scenario.expected, "contains")? {
        if !prompt.contains(&needle) {
            return Err(format!("the prompt dropped {needle:?}:\n{prompt}"));
        }
    }
    for needle in strings_field(&scenario.expected, "omits")? {
        if prompt.contains(&needle) {
            return Err(format!(
                "the prompt rode {needle:?} past the head limit:\n{prompt}"
            ));
        }
    }
    let actual =
        match generate_digest(&account, days, &messages, &stored, correlation, complete).await {
            Ok(digest) => serde_json::Value::String(digest),
            Err(err) => json!({ "error": digest_error_word(&err) }),
        };
    let expected = scenario
        .expected
        .get("digest")
        .cloned()
        .ok_or_else(|| "carries no digest string".to_owned())?;
    expect(&expected, &actual)
}

/// The report scenario: the fixture's window, stored categorisation,
/// and sent count through the generation seam over the canned
/// three-field completion, the parsed sections compared as the assembled
/// document text the caller stores, headings and order included.
pub(super) async fn report_scenario(
    scenario: &Scenario,
    complete: &dyn AiComplete,
    correlation: &CorrelationId,
) -> Result<(), String> {
    let account = account_field(&scenario.inputs)?;
    let days = number_field(&scenario.inputs, "days")?;
    let messages = doc_messages_field(&scenario.inputs)?;
    let stored = stored_field(&scenario.inputs)?;
    let sent_count = number_field(&scenario.inputs, "sent_count")?;
    let actual = match generate_report(
        &account,
        days,
        &messages,
        &stored,
        sent_count,
        correlation,
        complete,
    )
    .await
    {
        Ok(report) => serde_json::Value::String(report),
        Err(err) => json!({ "error": report_error_word(&err) }),
    };
    let expected = scenario
        .expected
        .get("report")
        .cloned()
        .ok_or_else(|| "carries no report string".to_owned())?;
    expect(&expected, &actual)
}

/// The split scenario: the fixture's stored categorisation through the
/// pure mapping, the map compared as its key and side pairs in key
/// order, the words the mapping excludes simply absent.
pub(super) fn split_scenario(scenario: &Scenario) -> Result<(), String> {
    let stored = stored_field(&scenario.inputs)?;
    let map: Vec<serde_json::Value> = split_map(&stored)
        .iter()
        .map(|(key, side)| json!([key.as_str(), side.as_str()]))
        .collect();
    expect(&scenario.expected, &json!({ "map": map }))
}

/// The meeting brief scenario: the fixture's day and meetings through the
/// generation seam over the canned completion, the parsed brief compared
/// beside the state word, with the prompt the service builds pinned
/// first, deterministically through the same builder: the day line, the
/// meetings' own facts, and the addresses the redactor must have masked.
/// An empty day carries no completion at all: the strict offline script
/// panics if the no-meetings path spends a call.
pub(super) async fn meeting_brief_scenario(
    scenario: &Scenario,
    complete: &dyn AiComplete,
    correlation: &CorrelationId,
) -> Result<(), String> {
    let account = account_field(&scenario.inputs)?;
    let day = day_field(&scenario.inputs)?;
    let meetings = brief_meetings_field(&scenario.inputs)?;
    let prompt = meeting_brief_prompt(day, &meetings);
    for needle in optional_strings(&scenario.expected, "contains")? {
        if !prompt.contains(&needle) {
            return Err(format!("the prompt dropped {needle:?}:\n{prompt}"));
        }
    }
    for needle in optional_strings(&scenario.expected, "omits")? {
        if prompt.contains(&needle) {
            return Err(format!(
                "the prompt rode {needle:?} past the redactor:\n{prompt}"
            ));
        }
    }
    let actual = if meetings.is_empty() {
        let brief = generate_meeting_brief(&account, day, &meetings, correlation, complete)
            .await
            .map_err(|err| err.to_string())?;
        if !brief.is_empty() {
            return Err("the empty day answered a brief".to_owned());
        }
        json!({ "state": "no_meetings" })
    } else {
        match generate_meeting_brief(&account, day, &meetings, correlation, complete).await {
            Ok(brief) => json!({ "state": "generated", "brief": brief }),
            Err(err) => json!({ "error": brief_error_word(&err) }),
        }
    };
    let expected = scenario
        .expected
        .get("outcome")
        .cloned()
        .ok_or_else(|| "carries no outcome object".to_owned())?;
    expect(&expected, &actual)
}

/// The window's account, from the `account` field.
fn account_field(inputs: &serde_json::Value) -> Result<AccountId, String> {
    AccountId::try_from(str_field(inputs, "account")?.as_str()).map_err(|err| err.to_string())
}

/// The failure word for a digest outcome the scenario expects to fail:
/// the seam's completion error or the unparseable output.
fn digest_error_word(error: &DigestError) -> &'static str {
    match error {
        DigestError::Completion(_) => "completion",
        DigestError::MalformedOutput(_) => "malformed",
    }
}

/// The failure word for a report outcome the scenario expects to fail:
/// the seam's completion error or the unparseable output.
fn report_error_word(error: &ReportError) -> &'static str {
    match error {
        ReportError::Completion(_) => "completion",
        ReportError::MalformedOutput(_) => "malformed",
    }
}

/// A required whole-number field, the days and sent count both.
fn number_field(inputs: &serde_json::Value, name: &str) -> Result<u32, String> {
    inputs
        .get(name)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("carries no {name} number"))
}

/// The window's messages, newest first exactly as the caller collected
/// them: the mail objects of the `messages` array, in order.
fn doc_messages_field(inputs: &serde_json::Value) -> Result<Vec<DocMessage>, String> {
    let array = inputs
        .get("messages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "carries no messages array".to_owned())?;
    array
        .iter()
        .map(|one| {
            Ok(DocMessage {
                key: ProviderKey::new(str_field(one, "key")?).map_err(|err| err.to_string())?,
                from_name: str_field(one, "from_name")?,
                from_address: str_field(one, "from_address")?,
                subject: str_field(one, "subject")?,
                body: str_field(one, "body")?,
                unread: one
                    .get("unread")
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| "the message carries no unread boolean".to_owned())?,
            })
        })
        .collect()
}

/// The stored categorisation, from the `stored` object's key and
/// category word pairs.
fn stored_field(inputs: &serde_json::Value) -> Result<StoredCategories, String> {
    let object = inputs
        .get("stored")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "carries no stored object".to_owned())?;
    object
        .iter()
        .map(|(key, word)| {
            let word = word
                .as_str()
                .ok_or_else(|| "the stored words must be strings".to_owned())?;
            Ok((
                ProviderKey::new(key).map_err(|err| err.to_string())?,
                word.to_owned(),
            ))
        })
        .collect()
}

/// The brief's civil day, from the `day` field's date word.
fn day_field(inputs: &serde_json::Value) -> Result<engine_api::CalendarDate, String> {
    str_field(inputs, "day")?
        .parse::<engine_api::CalendarDate>()
        .map_err(|err| err.to_string())
}

/// An optional array-of-strings field: absent answers empty, a present
/// member that is not a string is still a malformed fixture.
fn optional_strings(value: &serde_json::Value, name: &str) -> Result<Vec<String>, String> {
    match value.get(name) {
        Some(_) => strings_field(value, name),
        None => Ok(Vec::new()),
    }
}

/// The day's meetings, in the fixture's own order: the objects of the
/// `meetings` array, every optional field absent rather than null.
fn brief_meetings_field(inputs: &serde_json::Value) -> Result<Vec<BriefMeeting>, String> {
    let array = inputs
        .get("meetings")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "carries no meetings array".to_owned())?;
    let instant =
        |one: &serde_json::Value, name: &str| -> Result<engine_api::UtcDateTime, String> {
            one.get(name)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("the meeting carries no {name} instant"))?
                .parse::<engine_api::UtcDateTime>()
                .map_err(|err| err.to_string())
        };
    array
        .iter()
        .map(|one| {
            Ok(BriefMeeting {
                title: str_field(one, "title")?,
                start: instant(one, "start")?,
                end: instant(one, "end")?,
                all_day: one
                    .get("all_day")
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| "the meeting carries no all_day boolean".to_owned())?,
                organizer: one
                    .get("organizer")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                attendees: match one.get("attendees") {
                    Some(_) => strings_field(one, "attendees")?,
                    None => Vec::new(),
                },
                location: one
                    .get("location")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                invitation_subject: one
                    .get("invitation_subject")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// The failure word for a meeting brief outcome the scenario expects to
/// fail: the seam's completion error or the unparseable output.
fn brief_error_word(error: &MeetingBriefError) -> &'static str {
    match error {
        MeetingBriefError::Completion(_) => "completion",
        MeetingBriefError::MalformedOutput(_) => "malformed",
    }
}
