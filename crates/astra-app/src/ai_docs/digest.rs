// SPDX-License-Identifier: GPL-3.0-only
//! The digest service: one versioned generation over the window's
//! messages and their stored categorisation (the inbox-zero
//! `aiSummarizeEmailForDigest` shape, one call over the whole window
//! where inbox-zero spends one per message). The prompt keeps its
//! halves apart the way every Astra AI prompt does: the instruction,
//! the rollup, and the JSON output policy ride the trusted half, and
//! every message rides its own untrusted wrap under its trusted
//! category line, so a mail body can never speak as an instruction.
//! Failures answer typed errors; whether a failure shows as an empty
//! document or a surfaced error is the caller's decision.

use astra_ai::{repair_json, CorrelationId, LlmUseCase};
use engine_api::AccountId;
use serde::Deserialize;

use super::{
    clamped_days, grouped_messages, rollup, rollup_lines, DocMessage, StoredCategories,
    DOCUMENT_HEAD_LIMIT,
};
use crate::ai_rules::AiComplete;

/// The version of the digest prompt. Any change to what
/// [`digest_prompt`] sends, instruction wording included, bumps this
/// constant and reruns the digest evals (the rulebook's AI rule; the
/// eval pins land with their slice). It rides beside the builder the
/// way the drafting and judge constants do, observed through the eval
/// runner, not text baked into the model's prompt.
pub const DIGEST_PROMPT_VERSION: &str = "1";

/// The completion's parsed shape: `{"digest": "..."}`, the object the
/// output policy pins. Unknown fields are tolerated so the shape grows
/// additively.
#[derive(Debug, Deserialize)]
struct Digested {
    /// The digest text.
    digest: String,
}

/// Failures of the digest generation: the seam's completion error and
/// an output no parse or repair could save. The service propagates
/// both; the caller decides the surface.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DigestError {
    /// The completion seam failed; the message is the host's.
    #[error("digest generation call failed: {0}")]
    Completion(String),
    /// The completion did not parse as the digest JSON shape, even
    /// after the repair path.
    #[error("digest generation output unparseable: {0}")]
    MalformedOutput(String),
}

/// Generates the digest of `account`'s window: the rollup over
/// `messages` and their `stored_categories`, the versioned prompt over
/// it, and ONE completion through the seam under the turn's
/// `correlation` (the digest use case, default tier), parsed strictly
/// with one repair through the shared recovery path. The days clamp
/// applies inside the prompt builder, so the window the prompt names is
/// the window that generated; the message window itself is the
/// caller's collection.
///
/// The answer is the digest text, the content the caller stores under
/// the document's period. An empty window makes no call and answers an
/// empty string; the empty-versus-error surface stays the caller's.
///
/// # Errors
/// [`DigestError::Completion`] when the seam fails;
/// [`DigestError::MalformedOutput`] when no parse or repair saves the
/// completion.
pub async fn generate_digest(
    account: &AccountId,
    days: u32,
    messages: &[DocMessage],
    stored_categories: &StoredCategories,
    correlation: &CorrelationId,
    complete: &dyn AiComplete,
) -> Result<String, DigestError> {
    if messages.is_empty() {
        log::debug!("ai docs: the digest window for {account} is empty; no call rides");
        return Ok(String::new());
    }
    let prompt = digest_prompt(days, messages, stored_categories);
    let completion = complete
        .complete(LlmUseCase::Digest, &prompt, correlation)
        .await
        .map_err(DigestError::Completion)?;
    parse_digest(&completion)
}

/// Builds the versioned digest prompt: the instruction, the rollup
/// (one count line per category bucket, the uncategorised bucket
/// included), and the JSON output policy ride the trusted half, then
/// the messages grouped by their stored category with the head
/// [`DOCUMENT_HEAD_LIMIT`](super::DOCUMENT_HEAD_LIMIT) of each newest
/// first, every message in its own untrusted wrap under its trusted
/// category line. The days clamp applies here, so the window the
/// prompt names never leaves the pinned bounds.
#[must_use]
pub fn digest_prompt(
    days: u32,
    messages: &[DocMessage],
    stored_categories: &StoredCategories,
) -> String {
    let days = clamped_days(days);
    let day_word = if days == 1 { "day" } else { "days" };
    let buckets = rollup(messages, stored_categories);
    format!(
        "You write the on-demand digest of one email account's recent mail.\n\
         The window covers the last {days} {day_word}.\n\
         \n\
         Summarise what arrived, grounded only in the messages below.\n\
         Lead with what needs the user's attention, then the rest.\n\
         State the content directly; no meta-commentary about the mail.\n\
         Do not invent messages, senders, or details.\n\
         Do not include account numbers, tokens, or tracking identifiers.\n\
         \n\
         Output policy:\n\
         - Return a JSON object only: no prose, no markdown.\n\
         - The output must match the schema exactly: {{\"digest\":\"...\"}}.\n\
         \n\
         The window's rollup by stored category:\n\
         {rollup}\n\
         \n\
         The messages follow, grouped by category, newest first, at most \
         {limit} shown per category:\n\
         \n\
         {messages}\n\
         Write the digest from the messages above.",
        rollup = rollup_lines(&buckets),
        limit = DOCUMENT_HEAD_LIMIT,
        messages = grouped_messages(&buckets, DOCUMENT_HEAD_LIMIT),
    )
}

/// Parses one completion as the digest: a strict parse first, and on
/// failure exactly one repair through the first balanced JSON object,
/// the shared recovery path. The serde error of whichever parse got
/// furthest is the failure's detail.
fn parse_digest(completion: &str) -> Result<String, DigestError> {
    serde_json::from_str(completion)
        .or_else(|direct| repair_json(completion).map_or(Err(direct), serde_json::from_str))
        .map(|digested: Digested| digested.digest)
        .map_err(|source| DigestError::MalformedOutput(source.to_string()))
}
