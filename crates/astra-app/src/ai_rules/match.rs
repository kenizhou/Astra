// SPDX-License-Identifier: GPL-3.0-only
//! Rule matching: one `JudgeRuleMatch` call per enabled rule over one
//! message's context, the verdict recorded to the results store (migration
//! 0003) as it lands. The prompt keeps its two halves apart: the condition
//! sentence rides the trusted question half as data, and only the mail
//! content rides the context half the host wraps with the hardening
//! wrapper, so a condition can never be smuggled beside the mail it is
//! judged against.

use astra_ai::{AiStore, CorrelationId, JudgeOutcome, JudgeUseCase, JudgeVerdict, NewAiResult};
use engine_api::{AccountId, ProviderKey};

use super::{AiJudge, Rule};
use crate::ai_results::{outcome_word, AiStage};

/// The version of the rule-match prompt: the question's phrasing and the
/// context's shape. Any change to what [`match_question`] or
/// [`judge_context`] sends bumps this constant and reruns the match evals
/// (the rulebook's AI rule; the eval harness lands later in A1). It rides
/// beside the builders as the generation and judge constants do, observed
/// through the eval runner, not text baked into the model's prompt.
pub const RULES_MATCH_VERSION: &str = "1";

/// How many body characters ride one match context: the newest thoughts a
/// condition needs and no more, bounding what untrusted mail can carry
/// into a prompt.
pub const MATCH_CONTEXT_BODY_CAP: usize = 4000;

/// The marker a truncated body carries, so the judge knows the message
/// continues past what it can see.
const TRUNCATION_MARKER: &str = "[truncated]";

/// One message as the match sees it: its identity halves (the account and
/// provider key together, the reference convention) and the text a
/// condition is judged against. Built by the incoming pass from the
/// engine's rows and body read; the fields are plain so the match tests
/// need no engine at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailContext {
    /// The account owning the message.
    pub account: AccountId,
    /// The message's provider key, unique only within `account`.
    pub key: ProviderKey,
    /// The sender's display name, empty when the header carried none.
    pub from_name: String,
    /// The sender's address as the header spelled it.
    pub from_address: String,
    /// The subject, empty when the message carries none.
    pub subject: String,
    /// The plain-text body, uncapped: [`judge_context`] applies the cap.
    pub body: String,
}

/// One rule's verdict on one message: the rule (its actions included) and
/// the outcome that produced the match, so an audit can tell a model's yes
/// from the path behind it.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// The rule that matched.
    pub rule: Rule,
    /// The judge outcome that stands.
    pub outcome: JudgeOutcome,
}

/// Matches `mail` against every enabled rule in `rules`, one judge call
/// each, recording every verdict to `results` under the pass's
/// `correlation` as it lands. Only a `Yes` verdict matches: the abstain
/// default resolves to no match, so nothing executes on the table's
/// answer. A row that cannot be records is logged and dropped, never
/// failed: the pass's contract is to survive its store.
pub async fn match_rules(
    mail: &MailContext,
    rules: &[Rule],
    correlation: &CorrelationId,
    judge: &dyn AiJudge,
    results: &AiStore,
) -> Vec<MatchResult> {
    let mut matched = Vec::new();
    for rule in rules.iter().filter(|rule| rule.enabled) {
        let question = match_question(&rule.condition);
        let context = judge_context(mail);
        let outcome = judge
            .judge(
                JudgeUseCase::JudgeRuleMatch,
                &question,
                &context,
                correlation,
            )
            .await;
        let row = NewAiResult {
            correlation: correlation.clone(),
            message_account: mail.account.as_str().to_owned(),
            message_key: mail.key.as_str().to_owned(),
            stage: AiStage::RuleMatch.as_str().to_owned(),
            rule_id: Some(rule.id.clone()),
            verdict: outcome_word(outcome).to_owned(),
            category: None,
        };
        if let Err(err) = results.record_ai_result(&row) {
            log::warn!("ai rules: the match result could not be recorded: {err}");
        }
        if outcome.verdict == JudgeVerdict::Yes {
            matched.push(MatchResult {
                rule: rule.clone(),
                outcome,
            });
        }
    }
    matched
}

/// Builds the trusted question half for one condition: the condition as
/// data, then the question phrased so `Yes` is the positive pole (the
/// match), the convention every safe default in the judge table relies
/// on. The condition never rides the context half, so the hardening wrap
/// the host draws around the context cannot be turned into a place where
/// a rule's words masquerade as mail, or mail as the rule.
#[must_use]
pub fn match_question(condition: &str) -> String {
    format!(
        "Rule condition: {condition}\n\
         Does the rule condition above match the message in the context?"
    )
}

/// Builds the untrusted context half: the sender, the subject, and the
/// body capped at [`MATCH_CONTEXT_BODY_CAP`] characters with the
/// truncation marker when the message continues past the cap. Only mail
/// content rides here; the host wraps the whole half before the model
/// sees it.
#[must_use]
pub fn judge_context(mail: &MailContext) -> String {
    let from = if mail.from_name.trim().is_empty() {
        mail.from_address.clone()
    } else {
        format!("{} <{}>", mail.from_name, mail.from_address)
    };
    format!(
        "From: {from}\nSubject: {}\n\n{}",
        mail.subject,
        capped(&mail.body)
    )
}

/// `body`'s head to the cap with the truncation marker when it ran over,
/// char boundaries respected; an under-cap body rides unchanged.
fn capped(body: &str) -> String {
    if body.chars().count() <= MATCH_CONTEXT_BODY_CAP {
        return body.to_owned();
    }
    let head: String = body.chars().take(MATCH_CONTEXT_BODY_CAP).collect();
    format!("{head}\n{TRUNCATION_MARKER}")
}
