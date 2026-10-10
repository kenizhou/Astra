// SPDX-License-Identifier: GPL-3.0-only
//! The judge call on the background client: the spend guard first, then
//! the versioned single-token prompt through the shared completion
//! machinery, the strict parse, the one allowed retry on Abstain, and the
//! safe-default fold. A judge never errors and never gates authorisation:
//! its uncertainty is the abstain default, handed back as data.

use astra_ai::{
    judge_prompt, parse_judgement, safe_default, CorrelationId, JudgeOutcome, JudgePath,
    JudgeUseCase, JudgeVerdict, LlmCallRow,
};
use log::warn;

use super::LlmClient;

/// The weekly nano spend budget's default, in micros of currency: a
/// ten-dollar week of nano-tier spend, generous against any real judge
/// volume while still bounding a runaway loop.
const DEFAULT_NANO_BUDGET_MICROS: u64 = 10_000_000;

/// The environment variable the weekly budget comes from, read once at
/// client construction.
const NANO_BUDGET_VAR: &str = "ASTRA_LLM_NANO_BUDGET_MICROS";

/// Reads the weekly nano budget from `lookup`: an absent or blank value is
/// the generous default; a malformed value warns and stays on the default
/// (the guard is a net, not a configuration gate the host refuses to boot
/// over). Exposed to the parent for construction only.
pub(super) fn nano_budget_from(lookup: &dyn Fn(&str) -> Option<String>) -> u64 {
    let Some(raw) = lookup(NANO_BUDGET_VAR) else {
        return DEFAULT_NANO_BUDGET_MICROS;
    };
    if let Ok(budget) = raw.trim().parse::<u64>() {
        budget
    } else {
        warn!("[llm] {NANO_BUDGET_VAR} is not a number of micros; using the default");
        DEFAULT_NANO_BUDGET_MICROS
    }
}

/// The ledger outcome word for one judged completion: the strict parse of
/// what the model said, so a row's outcome is the verdict the judge acted
/// on (`judge:yes`, `judge:no`, `judge:abstain`).
fn judged_outcome(text: &str) -> String {
    format!("judge:{}", parse_judgement(text))
}

/// The safe-default outcome for a use case: the table's verdict pole under
/// the abstain-default path.
fn abstain_default(use_case: JudgeUseCase) -> JudgeOutcome {
    JudgeOutcome {
        verdict: safe_default(use_case).verdict(),
        path: JudgePath::AbstainDefault,
    }
}

impl LlmClient {
    /// One binary judge call: the versioned single-token prompt over the
    /// question and the hardening-wrapped context, one completion, the
    /// strict parser, and the safe-default machinery of the binary judge
    /// spec. The verdict of each attempt lands in its ledger row's outcome
    /// field, so the judge population stays auditable.
    ///
    /// The spend guard runs first: a weekly nano aggregate already at the
    /// `ASTRA_LLM_NANO_BUDGET_MICROS` budget answers the safe default with
    /// no network call and one `budget-skipped` row. An abstaining first
    /// try retries once and the retry resends the same versioned prompt at
    /// the same resolved chain head: an abstain never advances the fallback
    /// chain, while a retryable network failure inside either completion
    /// advances it exactly as [`LlmClient::complete`] does (that is the
    /// retry distinction). A call that fails outright (unconfigured,
    /// auth, quota, unreachable) answers the safe default under
    /// [`JudgePath::AbstainDefault`] with the failed attempts' rows
    /// already landed.
    pub async fn judge(
        &self,
        use_case: JudgeUseCase,
        question: &str,
        context: &str,
        correlation: &CorrelationId,
    ) -> JudgeOutcome {
        if let Some(skipped) = self.budget_guard_skips(use_case, correlation) {
            return skipped;
        }
        let llm_use_case = use_case.llm_use_case();
        let prompt = judge_prompt(question, context);
        let first = self
            .complete_with_outcome(llm_use_case, &prompt, correlation, &judged_outcome)
            .await;
        let first_verdict = match first {
            Ok(text) => parse_judgement(&text),
            Err(e) => {
                warn!("[llm] judge first try failed for {use_case}; the safe default applies: {e}");
                return abstain_default(use_case);
            }
        };
        if first_verdict != JudgeVerdict::Abstain {
            return JudgeOutcome {
                verdict: first_verdict,
                path: JudgePath::FirstTry,
            };
        }
        let retried = self
            .complete_with_outcome(llm_use_case, &prompt, correlation, &judged_outcome)
            .await;
        let retried_verdict = match retried {
            Ok(text) => parse_judgement(&text),
            Err(e) => {
                warn!("[llm] judge retry failed for {use_case}; the safe default applies: {e}");
                return abstain_default(use_case);
            }
        };
        JudgeOutcome::resolve(
            first_verdict,
            retried_verdict,
            safe_default(use_case).verdict(),
        )
    }

    /// The spend guard's answer when the weekly nano aggregate has reached
    /// the budget: the safe default, no network call, one `budget-skipped`
    /// row. A ledger read failure allows the call with a warn (a broken
    /// ledger cannot be spent against, and its rows cannot land either).
    fn budget_guard_skips(
        &self,
        use_case: JudgeUseCase,
        correlation: &CorrelationId,
    ) -> Option<JudgeOutcome> {
        let spent = match self.store.weekly_nano_spend_micros() {
            Ok(spent) => spent,
            Err(e) => {
                warn!("[llm] the spend guard read failed for {use_case}; allowing the call: {e}");
                return None;
            }
        };
        if spent < self.nano_budget_micros {
            return None;
        }
        warn!(
            "[llm] weekly nano spend of {spent} micros reached the {} micros budget; skipping \
             the {use_case} judge call",
            self.nano_budget_micros
        );
        self.record_skipped(use_case, correlation, "budget-skipped");
        Some(abstain_default(use_case))
    }

    /// Lands the no-call row for a judge call the guard answered: the use
    /// case and tier truthful, no model, no tokens, no cost, no latency. A
    /// ledger failure is logged, never dropped silently, and never fails
    /// the call it records.
    fn record_skipped(&self, use_case: JudgeUseCase, correlation: &CorrelationId, outcome: &str) {
        let row = LlmCallRow {
            correlation: correlation.clone(),
            use_case: use_case.llm_use_case(),
            tier: use_case.tier(),
            model: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_micros: None,
            latency_ms: 0,
            outcome: outcome.to_string(),
        };
        if let Err(e) = self.store.record_llm_call(&row) {
            warn!("[llm] ledger record failed: {e}");
        }
    }
}

#[cfg(test)]
#[path = "judge_call_tests.rs"]
mod tests;
