// SPDX-License-Identifier: GPL-3.0-only
//! The cached summarise action: one versioned prompt over the message's
//! context through the [`AiComplete`] seam, its answer served from the
//! summarise cache (migration 0005) whenever the summarised body is still
//! the body in front of the reader. The prompt keeps its halves apart the
//! way every Astra AI prompt does: the instruction and the output policy
//! ride the trusted half, and the message rides its own untrusted wrap,
//! so a mail body can never speak as an instruction. A cache hit returns
//! without a seam call; a miss runs the Summarise use case once and puts
//! the answer back; a failed or unparseable call caches nothing, so the
//! reader's retry spends the call the failure wasted.

use std::hash::{DefaultHasher, Hasher};

use astra_ai::{repair_json, tag_untrusted, AiStore, CorrelationId, LlmUseCase};
use serde::Deserialize;

use crate::ai_rules::{judge_context, AiComplete, MailContext};

/// The version of the summarisation prompt. Any change to what
/// [`summarise_prompt`] sends, instruction wording included, bumps this
/// constant and reruns the summarise evals (the rulebook's AI rule; the
/// eval pins land with the live slice). It rides beside the builder the
/// way the drafting and judge constants do, observed through the eval
/// runner, not text baked into the model's prompt.
pub const SUMMARISE_PROMPT_VERSION: &str = "1";

/// The completion's parsed shape: `{"summary": "..."}`, the object the
/// output policy pins (the digest summary's `content` shape, renamed for
/// the wire word the reading pane carries). Unknown fields are tolerated
/// so the shape grows additively.
#[derive(Debug, Deserialize)]
struct Summarised {
    /// The message's summary text.
    summary: String,
}

/// Failures of the summarise action: the seam's completion error and an
/// output no parse or repair could save.
#[derive(Debug, thiserror::Error)]
pub enum SummariseError {
    /// The completion seam failed; the message is the host's.
    #[error("summarise call failed: {0}")]
    Completion(String),
    /// The completion did not parse as the summary JSON shape, even after
    /// the repair path.
    #[error("summarise output unparseable: {0}")]
    MalformedOutput(String),
}

/// Summarises `mail`: the cached answer when the store holds a row whose
/// body hash matches, else one completion through the seam under the
/// turn's `correlation` (the Summarise use case, default tier), parsed
/// strictly with one repair through the shared recovery path, then cached
/// under the hash the body hashed to. The cache is an optimisation, never
/// a gate: a summary that cannot be cached still answers.
///
/// # Errors
/// [`SummariseError::Completion`] when the seam fails;
/// [`SummariseError::MalformedOutput`] when no parse or repair saves the
/// completion. Neither caches anything.
pub async fn summarise(
    mail: &MailContext,
    correlation: &CorrelationId,
    complete: &dyn AiComplete,
    cache: &AiStore,
) -> Result<String, SummariseError> {
    let hash = body_hash(&mail.body);
    if let Some(hit) = cached(mail, cache, &hash) {
        return Ok(hit);
    }
    let prompt = summarise_prompt(mail);
    let completion = complete
        .complete(LlmUseCase::Summarise, &prompt, correlation)
        .await
        .map_err(SummariseError::Completion)?;
    let summary = parse_summary(&completion)?;
    if let Err(err) = cache.cache_summary(mail.account.as_str(), mail.key.as_str(), &hash, &summary)
    {
        log::warn!("ai replies: the summary could not be cached: {err}");
    }
    Ok(summary)
}

/// The body's cache hash: sixteen hex digits of the standard library's
/// fixed-key hasher over the body's bytes (the drafts digest's precedent).
/// The failure direction is a false miss: a compiler's hasher change or a
/// refetched body re-summarises, never a stale hit.
#[must_use]
pub fn body_hash(body: &str) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(body.as_bytes());
    format!("{:016x}", hasher.finish())
}

/// Builds the versioned summarisation prompt: the instruction (the digest
/// summary's discipline, adapted to one message) and the JSON output
/// policy ride the trusted half, and the message rides the shared
/// one-message context (sender, subject, capped body) inside its own
/// untrusted wrap.
#[must_use]
pub fn summarise_prompt(mail: &MailContext) -> String {
    format!(
        "You summarise one email for its reader.\n\
         \n\
         Lead with the most important point, then the details that matter.\n\
         State the content directly; do not describe the message or use meta-commentary \
         such as highlights or discusses.\n\
         Do not mention the sender or start with phrases like this is a message from.\n\
         Separate several points with newlines; do not use bullet characters.\n\
         Exclude opaque technical identifiers such as account ids or tracking tokens.\n\
         Keep it concise: no longer than the message needs.\n\
         \n\
         Output policy:\n\
         - Return a JSON object only: no prose, no markdown.\n\
         - The output must match the schema exactly: {{\"summary\":\"...\"}}.\n\
         \n\
         Summarise the message in the context:\n\
         \n\
         {}",
        tag_untrusted("email", &judge_context(mail)),
    )
}

/// The cached answer, when the store holds one for the body in front of
/// the reader; a failed read answers none so the action summarises fresh
/// rather than failing on its optimisation.
fn cached(mail: &MailContext, cache: &AiStore, hash: &str) -> Option<String> {
    match cache.cached_summary(mail.account.as_str(), mail.key.as_str(), hash) {
        Ok(hit) => hit,
        Err(err) => {
            log::warn!("ai replies: the cache read failed; the summary runs fresh: {err}");
            None
        }
    }
}

/// Parses one completion as the summary: a strict parse first, and on
/// failure exactly one repair through the first balanced JSON object, the
/// shared recovery path. The serde error of whichever parse got furthest
/// is the failure's detail.
fn parse_summary(completion: &str) -> Result<String, SummariseError> {
    serde_json::from_str(completion)
        .or_else(|direct| repair_json(completion).map_or(Err(direct), serde_json::from_str))
        .map(|summarised: Summarised| summarised.summary)
        .map_err(|source| SummariseError::MalformedOutput(source.to_string()))
}
