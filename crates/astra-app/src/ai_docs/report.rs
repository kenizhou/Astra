// SPDX-License-Identifier: GPL-3.0-only
//! The report service: the window's deterministic statistics computed
//! in code first (the counts per category, the unread rate, the sent
//! count), then ONE structured generation through the seam producing
//! the three sections the analytics report pins (the inbox-zero report
//! shapes, executive summary, response patterns, recommendations,
//! folded into one call). The prompt keeps its halves apart: the
//! instruction, the statistics, and the JSON output policy ride the
//! trusted half, and every listing message rides its own untrusted
//! wrap under its trusted category line, so a mail body can never speak
//! as an instruction. Failures answer typed errors; whether a failure
//! shows as an empty document or a surfaced error is the caller's
//! decision.

use astra_ai::{repair_json, CorrelationId, LlmUseCase};
use engine_api::AccountId;
use serde::Deserialize;

use super::{
    clamped_days, grouped_messages, rollup, rollup_lines, DocMessage, RollupBucket,
    StoredCategories, DOCUMENT_HEAD_LIMIT,
};
use crate::ai_rules::AiComplete;

/// The version of the report prompt. Any change to what
/// [`report_prompt`] sends, instruction wording included, bumps this
/// constant and reruns the report evals (the rulebook's AI rule; the
/// eval pins land with their slice). It rides beside the builder the
/// way the drafting and judge constants do, observed through the eval
/// runner, not text baked into the model's prompt.
pub const REPORT_PROMPT_VERSION: &str = "1";

/// The completion's parsed shape: the three fields the output policy
/// pins, one per report section. Unknown fields are tolerated so the
/// shape grows additively.
#[derive(Debug, Deserialize)]
struct Reported {
    /// The executive summary section.
    executive_summary: String,
    /// The response patterns section.
    response_patterns: String,
    /// The recommendations section.
    recommendations: String,
}

/// Failures of the report generation: the seam's completion error and
/// an output no parse or repair could save. The service propagates
/// both; the caller decides the surface.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReportError {
    /// The completion seam failed; the message is the host's.
    #[error("report generation call failed: {0}")]
    Completion(String),
    /// The completion did not parse as the report JSON shape, even
    /// after the repair path.
    #[error("report generation output unparseable: {0}")]
    MalformedOutput(String),
}

/// Generates the analytics report over `account`'s window: the
/// statistics over `messages`, their `stored_categories`, and the
/// window's `sent_count` computed in code first, the versioned prompt
/// over them, and ONE structured completion through the seam under the
/// turn's `correlation` (the report use case, default tier), parsed
/// strictly with one repair through the shared recovery path and
/// assembled into the three-section document. The days clamp applies
/// inside the prompt builder, so the window the prompt names is the
/// window that generated; the message window itself is the caller's
/// collection.
///
/// The answer is the assembled report text, the content the caller
/// stores under the document's period. An empty window makes no call
/// and answers an empty string; the empty-versus-error surface stays
/// the caller's.
///
/// # Errors
/// [`ReportError::Completion`] when the seam fails;
/// [`ReportError::MalformedOutput`] when no parse or repair saves the
/// completion.
pub async fn generate_report(
    account: &AccountId,
    days: u32,
    messages: &[DocMessage],
    stored_categories: &StoredCategories,
    sent_count: u32,
    correlation: &CorrelationId,
    complete: &dyn AiComplete,
) -> Result<String, ReportError> {
    if messages.is_empty() {
        log::debug!("ai docs: the report window for {account} is empty; no call rides");
        return Ok(String::new());
    }
    let prompt = report_prompt(days, messages, stored_categories, sent_count);
    let completion = complete
        .complete(LlmUseCase::Report, &prompt, correlation)
        .await
        .map_err(ReportError::Completion)?;
    parse_reported(&completion).map(|reported| assembled(&reported))
}

/// Builds the versioned report prompt: the instruction, the statistics
/// block computed before any call, and the JSON output policy ride the
/// trusted half, then the messages grouped by their stored category
/// with the head [`DOCUMENT_HEAD_LIMIT`] of each newest first, every
/// message in its own untrusted wrap under its trusted category line.
/// The days clamp applies here, so the window the prompt names never
/// leaves the pinned bounds.
#[must_use]
pub fn report_prompt(
    days: u32,
    messages: &[DocMessage],
    stored_categories: &StoredCategories,
    sent_count: u32,
) -> String {
    let days = clamped_days(days);
    let day_word = if days == 1 { "day" } else { "days" };
    let buckets = rollup(messages, stored_categories);
    format!(
        "You write the analytics report of one email account's recent mail.\n\
         The window covers the last {days} {day_word}.\n\
         \n\
         The window's statistics, computed before this call:\n\
         {stats}\n\
         \n\
         Write the report's three sections grounded in the statistics and the \
         messages:\n\
         1. executive summary: who the user is by their mail and what the window shows.\n\
         2. response patterns: how the user's mail arrives and what they act on.\n\
         3. recommendations: specific, practical changes, easiest first.\n\
         Do not invent numbers; the statistics above are the truth.\n\
         \n\
         Output policy:\n\
         - Return a JSON object only: no prose, no markdown.\n\
         - The output must match the schema exactly: \
         {{\"executive_summary\":\"...\",\"response_patterns\":\"...\",\"recommendations\":\"...\"}}.\n\
         \n\
         The messages follow, grouped by category, newest first, at most \
         {limit} shown per category:\n\
         \n\
         {messages}\n\
         Write the report from the statistics and messages above.",
        stats = stats_lines(messages, &buckets, sent_count),
        limit = DOCUMENT_HEAD_LIMIT,
        messages = grouped_messages(&buckets, DOCUMENT_HEAD_LIMIT),
    )
}

/// The statistics block as the prompt renders it: the window's message
/// count, the unread rate floored to whole percent, the sent count,
/// and the counts by stored category, every number computed in code
/// before any call. The empty window never reaches the prompt; the
/// rate guards its divide anyway.
fn stats_lines(messages: &[DocMessage], buckets: &[RollupBucket<'_>], sent_count: u32) -> String {
    let total = messages.len();
    let unread = messages.iter().filter(|message| message.unread).count();
    let percent = (unread * 100).checked_div(total).unwrap_or(0);
    format!(
        "messages in the window: {total}\n\
         unread: {unread} of {total} ({percent}%)\n\
         sent by the user in the window: {sent}\n\
         counts by stored category:\n\
         {rollup}",
        sent = sent_count,
        rollup = rollup_lines(buckets),
    )
}

/// The parsed completion as the stored document renders it: the three
/// sections under their headings, the one text the panel and the CLI
/// read back.
fn assembled(reported: &Reported) -> String {
    format!(
        "Executive summary\n{}\n\nResponse patterns\n{}\n\nRecommendations\n{}",
        reported.executive_summary, reported.response_patterns, reported.recommendations,
    )
}

/// Parses one completion as the report: a strict parse first, and on
/// failure exactly one repair through the first balanced JSON object,
/// the shared recovery path. The serde error of whichever parse got
/// furthest is the failure's detail.
fn parse_reported(completion: &str) -> Result<Reported, ReportError> {
    serde_json::from_str(completion)
        .or_else(|direct| repair_json(completion).map_or(Err(direct), serde_json::from_str))
        .map_err(|source| ReportError::MalformedOutput(source.to_string()))
}
