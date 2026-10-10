// SPDX-License-Identifier: GPL-3.0-only
//! The cold stage: one `JudgeColdEmail` call per message through the
//! [`AiJudge`] seam, the verdict recorded to the results store (migration
//! 0003) as it lands. The call rides the judge family's cheapest tier
//! (nano, classification-sized, from the use case the seam ledgers under)
//! and the same capped context the match stage sends, so one message costs
//! the same context twice and nothing more.

use astra_ai::{AiStore, CorrelationId, JudgeOutcome, JudgeUseCase, NewAiResult};

use super::matching::{judge_context, MailContext};
use super::AiJudge;
use crate::ai_results::{outcome_word, AiStage};

/// The version of the cold judge's prompt halves: the question's phrasing
/// and the context's shape. Any change to what [`cold_question`] or the
/// shared [`judge_context`] sends bumps this constant and reruns the cold
/// evals (the rulebook's AI rule; the eval harness lands later in A1).
pub const COLD_JUDGE_VERSION: &str = "1";

/// Judges whether `mail` is cold outreach: one binary judge call under the
/// pass's `correlation`, the outcome recorded to `results` as it lands.
/// Only the verdict is returned for the caller to read; A1 executes
/// nothing on it, the CLI's cold read and any later surface consume the
/// stored row.
pub async fn judge_cold(
    mail: &MailContext,
    correlation: &CorrelationId,
    judge: &dyn AiJudge,
    results: &AiStore,
) -> JudgeOutcome {
    let outcome = judge
        .judge(
            JudgeUseCase::JudgeColdEmail,
            &cold_question(),
            &judge_context(mail),
            correlation,
        )
        .await;
    let row = NewAiResult {
        correlation: correlation.clone(),
        message_account: mail.account.as_str().to_owned(),
        message_key: mail.key.as_str().to_owned(),
        stage: AiStage::Cold.as_str().to_owned(),
        rule_id: None,
        verdict: outcome_word(outcome).to_owned(),
        category: None,
    };
    if let Err(err) = results.record_ai_result(&row) {
        log::warn!("ai rules: the cold result could not be recorded: {err}");
    }
    outcome
}

/// Builds the trusted question half, phrased so `Yes` is the cold pole
/// (the verdict the safe-default table leans against: an abstaining judge
/// answers not cold, and the mail stays put). Only instruction text rides
/// here; the mail rides the context half the host wraps.
#[must_use]
pub fn cold_question() -> String {
    "Does the message in the context look like unsolicited cold outreach \
     from a sender the recipient has no prior relationship with?"
        .to_owned()
}
