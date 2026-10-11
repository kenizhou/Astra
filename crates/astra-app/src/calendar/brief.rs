// SPDX-License-Identifier: GPL-3.0-only
//! The meeting brief service: one versioned generation over a civil day's
//! meetings, the D5 answer of the calendar slice. The prompt keeps its
//! halves apart the way every Astra AI prompt does: the instruction, the
//! day, and the JSON output policy ride the trusted half with each
//! meeting's timing, and every meeting's own content rides its own
//! untrusted wrap through the crate's span redactor, so no address or
//! phone-shaped number a participant line carries crosses to the model
//! and a calendar field can never speak as an instruction.
//!
//! The app-facing half (the store re-read, the invitation link, and the
//! view the host commands answer) lives in the brief drive module beside
//! this one, the same service-and-drive split the AI documents slice
//! made for its digest.

use std::fmt::Write as _;

use astra_ai::{redact, repair_json, tag_untrusted, CorrelationId, LlmUseCase};
use engine_api::{CalendarDate, UtcDateTime};
use serde::{Deserialize, Serialize};

use crate::ai_rules::AiComplete;

/// The version of the meeting brief prompt. Any change to what
/// [`meeting_brief_prompt`] sends, instruction wording included, bumps
/// this constant and reruns the meeting brief evals (the rulebook's AI
/// rule), the same contract the digest's own version rides.
pub const MEETING_BRIEF_PROMPT_VERSION: &str = "1";

/// How many of the account's newest messages the invitation link scans:
/// the bound that keeps one brief's scheduling reads finite. The scan is
/// newest first and cache-first on the raw source, so a mailbox whose
/// invitation is older than this bound links no subject; the bound is the
/// honest cost of not parsing the whole mailbox per brief.
pub const BRIEF_INVITATION_SCAN_LIMIT: usize = 50;

/// One meeting as the brief service sees it: the joined facts of one
/// occurrence (its instants) and its master (its content), with the
/// invitation context where a stored invitation names the meeting. The
/// drive builds these from the store's rows; the fields are plain so the
/// service tests need no engine at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BriefMeeting {
    /// The meeting's title, as the master event carries it.
    pub title: String,
    /// The occurrence's start, resolved to a UTC instant.
    pub start: UtcDateTime,
    /// The occurrence's end, resolved the same way.
    pub end: UtcDateTime,
    /// Whether the master event is all-day.
    pub all_day: bool,
    /// The organiser's line (`Name <address>`), when the event carries
    /// participants.
    pub organizer: Option<String>,
    /// The attendee lines, when the event carries participants.
    pub attendees: Vec<String>,
    /// The location, when the event carries one.
    pub location: Option<String>,
    /// The linked invitation message's subject, when a stored invitation
    /// names this meeting.
    pub invitation_subject: Option<String>,
}

/// The completion's parsed shape: `{"brief": "..."}`, the object the
/// output policy pins. Unknown fields are tolerated so the shape grows
/// additively.
#[derive(Debug, Deserialize)]
struct Briefed {
    /// The brief text.
    brief: String,
}

/// Failures of the brief generation: the seam's completion error and an
/// output no parse or repair could save. The service propagates both; the
/// caller decides the surface.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MeetingBriefError {
    /// The completion seam failed; the message is the host's.
    #[error("meeting brief generation call failed: {0}")]
    Completion(String),
    /// The completion did not parse as the brief JSON shape, even after
    /// the repair path.
    #[error("meeting brief generation output unparseable: {0}")]
    MalformedOutput(String),
}

/// Where one brief of one day stands. Crosses as a snake_case word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingBriefState {
    /// The day held meetings and one generation wrote the brief.
    Generated,
    /// The day held no meetings: no call was spent and no brief exists.
    NoMeetings,
}

/// One meeting row of the brief view: the same joined facts the service
/// briefs over, as the host renders the brief's per-meeting list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefMeetingRow {
    /// The meeting's title, as the master event carries it.
    pub title: String,
    /// The occurrence's start, as a whole-second UTC `...Z` instant.
    pub start: UtcDateTime,
    /// The occurrence's end, resolved the same way.
    pub end: UtcDateTime,
    /// Whether the master event is all-day.
    pub all_day: bool,
    /// The organiser's line, when the event carries participants.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organizer: Option<String>,
    /// The attendee lines, when the event carries participants.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attendees: Vec<String>,
    /// The location, when the event carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// The linked invitation message's subject, when a stored invitation
    /// names this meeting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitation_subject: Option<String>,
}

/// One brief of one account's civil day, as the host command answers it:
/// the day, the state, the brief text the one generation wrote, and the
/// per-meeting rows it briefs over. The no-meetings state crosses with no
/// brief and no rows at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingBriefView {
    /// The account whose day was briefed.
    pub account: engine_api::AccountId,
    /// The civil day the brief covers, in the view's display zone.
    pub day: CalendarDate,
    /// Where the brief stands.
    pub state: MeetingBriefState,
    /// The brief text; empty in the no-meetings state.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub brief: String,
    /// The meetings the brief covers, in start order; empty in the
    /// no-meetings state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meetings: Vec<BriefMeetingRow>,
}

/// Failures of the brief pull: the honest no-bundle refusal, the
/// occurrence read or day resolution that cannot run, and the service's
/// own typed generation failures. Every failure answers with no content:
/// the caller never renders half a brief.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BriefDocumentError {
    /// The app runs no AI bundle, so no generation can be spent.
    #[error("no ai bundle is running")]
    NoBundle,
    /// The day or its occurrences could not be resolved or read, so the
    /// collection refuses rather than brief a day it could not see.
    #[error("the brief day could not be read: {0}")]
    Occurrence(String),
    /// The brief service failed; the message is its own.
    #[error(transparent)]
    Brief(#[from] MeetingBriefError),
}

/// Generates the brief of `account`'s `day` over `meetings`: the
/// versioned prompt over the day's rows, and ONE completion through the
/// seam under the turn's `correlation` (the meeting brief use case, the
/// registry's tier), parsed strictly with one repair through the shared
/// recovery path.
///
/// The answer is the brief text. An empty day makes no call and answers
/// an empty string; the empty-versus-error surface stays the caller's.
///
/// # Errors
/// [`MeetingBriefError::Completion`] when the seam fails;
/// [`MeetingBriefError::MalformedOutput`] when no parse or repair saves
/// the completion.
pub async fn generate_meeting_brief(
    account: &engine_api::AccountId,
    day: CalendarDate,
    meetings: &[BriefMeeting],
    correlation: &CorrelationId,
    complete: &dyn AiComplete,
) -> Result<String, MeetingBriefError> {
    if meetings.is_empty() {
        log::debug!("calendar: the brief day for {account} holds no meetings; no call rides");
        return Ok(String::new());
    }
    let prompt = meeting_brief_prompt(day, meetings);
    let completion = complete
        .complete(LlmUseCase::MeetingBrief, &prompt, correlation)
        .await
        .map_err(MeetingBriefError::Completion)?;
    parse_brief(&completion)
}

/// Builds the versioned meeting brief prompt: the instruction, the day,
/// and the JSON output policy ride the trusted half, then each meeting in
/// start order with its timing on a trusted line beside its own untrusted
/// wrap, the wrap's content passed through the crate's span redactor so
/// no address or phone-shaped number a participant line carries crosses
/// to the model.
#[must_use]
pub fn meeting_brief_prompt(day: CalendarDate, meetings: &[BriefMeeting]) -> String {
    format!(
        "You write the meeting brief for one calendar day.\n\
         The day is {day}.\n\
         \n\
         Summarise the day's meetings, grounded only in the meetings below.\n\
         Lead with what needs preparation or a reply, then the rest.\n\
         State the content directly; no meta-commentary about the calendar.\n\
         Do not invent meetings, attendees, or details.\n\
         Do not include account numbers, tokens, or tracking identifiers.\n\
         \n\
         Output policy:\n\
         - Return a JSON object only: no prose, no markdown.\n\
         - The output must match the schema exactly: {{\"brief\":\"...\"}}.\n\
         \n\
         The day's meetings follow, in start order:\n\
         {meetings}\
         Write the brief from the meetings above.",
        meetings = meetings
            .iter()
            .map(rendered_row)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// One meeting as the prompt's listing renders it: the timing on the
/// trusted line (an instant's digit runs must not ride the redactor),
/// then the meeting's own content in its own redacted untrusted wrap.
fn rendered_row(meeting: &BriefMeeting) -> String {
    let span = if meeting.all_day {
        format!("From {} to {} (all day):\n", meeting.start, meeting.end)
    } else {
        format!("From {} to {}:\n", meeting.start, meeting.end)
    };
    format!(
        "{span}{}\n",
        tag_untrusted("meeting", &redact(&rendered_meeting(meeting))),
    )
}

/// One meeting's content lines, every field the row carries: the title,
/// the organiser, the attendees, the location, and the linked invitation's
/// subject, each absent field crossing no line.
fn rendered_meeting(meeting: &BriefMeeting) -> String {
    let mut body = format!("Title: {}\n", meeting.title);
    if let Some(organizer) = &meeting.organizer {
        let _ = writeln!(body, "Organiser: {organizer}");
    }
    if !meeting.attendees.is_empty() {
        let _ = writeln!(body, "Attendees: {}", meeting.attendees.join(", "));
    }
    if let Some(location) = &meeting.location {
        let _ = writeln!(body, "Location: {location}");
    }
    if let Some(subject) = &meeting.invitation_subject {
        let _ = writeln!(body, "Invitation subject: {subject}");
    }
    body
}

/// Parses one completion as the brief: a strict parse first, and on
/// failure exactly one repair through the first balanced JSON object, the
/// shared recovery path. The serde error of whichever parse got furthest
/// is the failure's detail.
fn parse_brief(completion: &str) -> Result<String, MeetingBriefError> {
    serde_json::from_str(completion)
        .or_else(|direct| repair_json(completion).map_or(Err(direct), serde_json::from_str))
        .map(|briefed: Briefed| briefed.brief)
        .map_err(|source| MeetingBriefError::MalformedOutput(source.to_string()))
}

#[cfg(test)]
#[path = "../calendar_brief_tests.rs"]
mod tests;
