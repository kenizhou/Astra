// SPDX-License-Identifier: GPL-3.0-only
//! The surfacing gate: the read that decides which pool rows become
//! smart-reply chips (the A2 spec's D2 and D3 data half). One pool row
//! surfaces only when its confidence clears the floor constant and the
//! binary judge answers Yes to the worth-surfacing question; every other
//! outcome hides the chip and loses nothing, the row stays pooled and
//! queryable. The dismissal marker is consulted first, before any judge
//! call is spent, so a dismissed message never costs another nano call.
//! Gate verdicts ride the host's ledger under the turn's correlation id
//! and nothing else: the ai_results table belongs to the incoming pass
//! and its durable seen tracking, and a gate row there would mark a
//! message judged that no pass judged.

use astra_ai::{AiStore, CorrelationId, JudgeUseCase, JudgeVerdict};
use engine_api::{AccountId, ProviderKey};

use crate::ai_rules::AiJudge;

/// The version of the surfacing gate's prompt halves: the question's
/// phrasing and the context's shape. Any change to what
/// [`surface_question`] or the drafted-reply context sends bumps this
/// constant and reruns the gate evals (the rulebook's AI rule; the eval
/// pins land with the live slice).
pub const GATE_JUDGE_VERSION: &str = "1";

/// The confidence a pooled draft must carry to reach the judge: the
/// closed set's medium score, so medium and high clear the floor and low
/// stays pooled.
pub const CHIP_CONFIDENCE_FLOOR: f64 = 0.6;

/// How many chips one message surfaces: the reading pane's pair.
pub const CHIP_LIMIT: usize = 2;

/// How many characters one chip's preview carries, the trim mark
/// excluded: the button's one-line budget.
pub const CHIP_PREVIEW_CAP: usize = 80;

/// One smart-reply chip: the whole drafted reply (the document the
/// composer opens as a reply when the chip is accepted) beside the
/// one-line preview the button renders. Serialisable so the host command
/// carries it across the wire to the reading pane.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Chip {
    /// The drafted reply, trimmed: the text the accept path prefills.
    pub reply: String,
    /// The reply's head as one line, capped at [`CHIP_PREVIEW_CAP`]
    /// characters with `...` when it was trimmed.
    pub preview: String,
}

/// Reads the message's chips from the pool: the dismissal marker first,
/// then the pool's rows newest first, each drafted row that clears the
/// confidence floor judged once through the seam under `correlation`
/// until [`CHIP_LIMIT`] chips stand. Skipped rows, blank replies, and
/// drafts below the floor never reach the judge; a judge that answers
/// anything but Yes (the abstain default included) hides its chip. An
/// unreadable store answers no chips: the gate fails quiet, and the pool
/// row remains queryable through the CLI whatever the surface does.
pub async fn chips_for(
    account: &AccountId,
    key: &ProviderKey,
    pool: &AiStore,
    judge: &dyn AiJudge,
    correlation: &CorrelationId,
) -> Vec<Chip> {
    let (account, key) = (account.as_str(), key.as_str());
    if dismissed(pool, account, key) {
        return Vec::new();
    }
    let rows = match pool.draft_pool_for_message(account, key) {
        Ok(rows) => rows,
        Err(err) => {
            log::warn!("ai replies: the pool read failed; the gate answers no chips: {err}");
            return Vec::new();
        }
    };
    let mut chips = Vec::new();
    for row in rows {
        if chips.len() == CHIP_LIMIT {
            break;
        }
        let Some(reply) = drafted_reply(row.reply.as_deref()) else {
            continue;
        };
        if !row
            .confidence
            .is_some_and(|confidence| confidence >= CHIP_CONFIDENCE_FLOOR)
        {
            continue;
        }
        let outcome = judge
            .judge(
                JudgeUseCase::JudgeDraftSurface,
                &surface_question(),
                &surface_context(reply),
                correlation,
            )
            .await;
        if outcome.verdict == JudgeVerdict::Yes {
            chips.push(Chip {
                preview: preview(reply),
                reply: reply.to_owned(),
            });
        }
    }
    chips
}

/// Builds the trusted question half, phrased so `Yes` is the positive
/// pole (worth surfacing), the convention the safe-default table leans
/// on: an abstaining judge answers unsurfaced, and the chip stays hidden.
#[must_use]
pub fn surface_question() -> String {
    "Is the drafted reply in the context a genuinely useful reply the user could send, not filler?"
        .to_owned()
}

/// Builds the untrusted context half: the drafted reply alone, labelled.
/// The reply is model-derived text over untrusted mail, so the host wraps
/// it like any context half before the judge sees it.
fn surface_context(reply: &str) -> String {
    format!("Drafted reply:\n{reply}")
}

/// The reply of one pooled row as a candidate: trimmed, and `None` when
/// the row carries none (a skipped row) or only blanks (filler by
/// definition, never worth a judge call).
fn drafted_reply(reply: Option<&str>) -> Option<&str> {
    reply.map(str::trim).filter(|reply| !reply.is_empty())
}

/// The reply's head as the button's one-line preview: whitespace runs
/// collapse to single spaces, then the first [`CHIP_PREVIEW_CAP`]
/// characters ride with `...` when the line ran over, character
/// boundaries respected.
fn preview(reply: &str) -> String {
    let collapsed = reply.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= CHIP_PREVIEW_CAP {
        return collapsed;
    }
    let head: String = collapsed.chars().take(CHIP_PREVIEW_CAP).collect();
    format!("{head}...")
}

/// Whether the message's chips stand dismissed. A read that fails answers
/// dismissed: the gate errs quiet rather than surfacing chips the reader
/// may have put away.
fn dismissed(pool: &AiStore, account: &str, key: &str) -> bool {
    match pool.chips_dismissed(account, key) {
        Ok(dismissed) => dismissed,
        Err(err) => {
            log::warn!("ai replies: the dismissal read failed; the gate answers no chips: {err}");
            true
        }
    }
}
