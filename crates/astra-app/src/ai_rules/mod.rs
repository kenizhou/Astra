// SPDX-License-Identifier: GPL-3.0-only
//! The AI rules model: the closed action set, the rule record, the
//! completion seam the host implements, and the bridge to the rules store
//! (migration 0002). Generation lives in the `generate` child module; the
//! matching and execution stages arrive with the incoming pass.

// `match` is a keyword, so the module over match.rs is `matching` (the file
// name stays the one the plan pins).
mod execute;
mod generate;
#[path = "match.rs"]
mod matching;

#[cfg(test)]
#[path = "execute_tests.rs"]
mod execute_tests;

#[cfg(test)]
#[path = "generate_tests.rs"]
mod generate_tests;

#[cfg(test)]
#[path = "match_tests.rs"]
mod matching_tests;

use astra_ai::{CorrelationId, JudgeOutcome, JudgeUseCase, LlmUseCase, NewRule, RuleId, RuleRow};
use async_trait::async_trait;

pub use generate::{generation_prompt, prompt_to_rules, RULES_PROMPT_VERSION};
pub use matching::{
    judge_context, match_question, match_rules, MailContext, MatchResult, MATCH_CONTEXT_BODY_CAP,
    RULES_MATCH_VERSION,
};

/// One action a matched rule takes on a message. The set is closed:
/// `mark_read` and `set_flagged` (the deterministic, reversible pair A1
/// ships; the wider action set arrives with the folders and draft-reply
/// slices, recorded as it lands). Wire words are snake_case and unknown
/// strings are rejected loudly at parse, never silently dropped or
/// defaulted: a rule that names an action this build does not know fails
/// its load, and a generation that names one fails whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Mark the matched message read.
    MarkRead,
    /// Flag (star) the matched message.
    SetFlagged,
}

impl Action {
    /// The closed set, in wire order: the one list the generation prompt
    /// teaches and the pins cross-check.
    pub const ALL: [Self; 2] = [Self::MarkRead, Self::SetFlagged];

    /// The snake_case wire word.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MarkRead => "mark_read",
            Self::SetFlagged => "set_flagged",
        }
    }
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One rule: a short name, the natural language condition sentence the
/// versioned generation prompt produces, the closed action list, and the
/// enabled flag. Generated before storage, stored and loaded through the
/// store row bridge below; serialisable so the CLI's `--json` can carry
/// it across the process boundary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rule {
    /// The rule's identity, minted at generation or creation.
    pub id: RuleId,
    /// The rule's short name.
    pub name: String,
    /// The natural language condition sentence.
    pub condition: String,
    /// The actions to take on a match; every element of the closed set.
    pub actions: Vec<Action>,
    /// Whether the incoming pass considers the rule.
    pub enabled: bool,
}

impl Rule {
    /// The store insert for this rule: the row the `rules` table takes,
    /// with the actions serialised to their JSON wire words.
    #[must_use]
    pub fn as_new_rule(&self) -> NewRule {
        NewRule {
            id: self.id.clone(),
            name: self.name.clone(),
            condition: self.condition.clone(),
            actions: serde_json::to_string(&self.actions).expect("the closed set serialises"),
            enabled: self.enabled,
        }
    }
}

impl TryFrom<RuleRow> for Rule {
    type Error = RulesError;

    /// Parses a stored row into a rule. The actions payload must be a JSON
    /// array of the closed set's wire words; anything else, malformed JSON
    /// or an unknown action name, is rejected loudly rather than skipped.
    fn try_from(row: RuleRow) -> Result<Self, Self::Error> {
        let actions: Vec<Action> =
            serde_json::from_str(&row.actions).map_err(|source| RulesError::UnknownAction {
                id: row.id.clone(),
                detail: source.to_string(),
            })?;
        Ok(Self {
            id: row.id,
            name: row.name,
            condition: row.condition,
            actions,
            enabled: row.enabled,
        })
    }
}

/// The completion seam: one background-model completion behind the AI
/// generation paths. The desktop host implements it over its `LlmClient`
/// (tier chain, fallback, and the ledger row per attempt); tests and the
/// eval runner inject fixture completions through the same trait. The
/// error is the host's failure rendered as its message: this crate cannot
/// name the host's error type without depending on the host, and the
/// caller only reports it.
#[async_trait]
pub trait AiComplete: Send + Sync {
    /// One completion for `use_case`, ledgered by the implementing host
    /// under `correlation`. The returned string is the model's raw text;
    /// the structured caller parses it itself.
    ///
    /// # Errors
    /// The host's failure as its message.
    async fn complete(
        &self,
        use_case: LlmUseCase,
        prompt: &str,
        correlation: &CorrelationId,
    ) -> Result<String, String>;
}

/// The judge seam: one binary judge call behind the AI matching paths,
/// the judge family's mirror of [`AiComplete`]. The desktop host implements
/// it over its `LlmClient::judge` (the versioned single-token prompt with
/// the hardening wrap, the retry, the safe default, and the ledger row per
/// attempt); tests inject fixture verdicts through the same trait.
///
/// Errors never cross the seam: the host folds every failure into the
/// judge's safe default before it answers, so the caller always receives a
/// decided [`JudgeOutcome`] and never a failure to propagate. The question
/// and context arrive as the two halves the host's prompt template keeps
/// apart: the question is trusted instruction text (a rule's condition
/// sentence rides here, as data), the context is untrusted mail content
/// the host wraps before the model sees it.
#[async_trait]
pub trait AiJudge: Send + Sync {
    /// One judged one-bit answer for `use_case`, ledgered by the
    /// implementing host under `correlation`.
    async fn judge(
        &self,
        use_case: JudgeUseCase,
        question: &str,
        context: &str,
        correlation: &CorrelationId,
    ) -> JudgeOutcome;
}

/// Failures of the rules model: the seam's completion error, a generation
/// output no parse or repair could save, and a stored actions payload
/// outside the closed set.
#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    /// The completion seam failed; the message is the host's.
    #[error("rules generation call failed: {0}")]
    Completion(String),
    /// The completion did not parse as the rules JSON shape, even after
    /// the repair path. The detail names the parse failure; an action
    /// outside the closed set lands here for generated rules too, its
    /// word quoted by the detail.
    #[error("rules generation output unparseable: {0}")]
    MalformedOutput(String),
    /// A stored rule's actions payload is not a JSON array of the closed
    /// set: malformed JSON or an unknown action name, rejected loudly at
    /// load rather than skipped.
    #[error("rule {id} carries actions outside the closed set: {detail}")]
    UnknownAction {
        /// The rule whose payload was rejected.
        id: RuleId,
        /// Why: the parse failure, quoting the offending payload.
        detail: String,
    },
}
