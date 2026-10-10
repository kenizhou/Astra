// SPDX-License-Identifier: GPL-3.0-only
//! The binary judge: strict one-word verdicts for one-bit decisions
//! (the binary judge spec). A judge call sends one versioned prompt built
//! from the caller's question and the hardening-wrapped context, forces a
//! single-token answer, parses it with [`parse_judgement`], and folds the
//! one allowed retry and the safe default into a [`JudgeOutcome`].
//! Abstain is a first-class state: it is retried once, then loudly
//! resolved through the safe-default table, never silently collapsed.

use serde::{Deserialize, Serialize};

use crate::hardening::tag_untrusted;
use crate::tier::ModelTier;
use crate::use_case::LlmUseCase;

/// The version of the judge prompt templates. Any change to what
/// [`judge_prompt`] sends, including the strict instruction's wording,
/// bumps this constant and reruns the scenario evals (the rulebook's AI
/// rule; the eval harness lands later in A1).
pub const JUDGE_TEMPLATES_VERSION: &str = "1";

/// The abstain marker: the one answer word that names an honest
/// uncertainty. The spelling is pinned here and by the parser's tests;
/// the strict instruction the template sends spells it identically.
pub const ABSTAIN_MARKER: &str = "abstain";

/// A judge's answer: one bit, or an honest abstain. The wire words are the
/// lowercase answer words themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeVerdict {
    /// The positive pole.
    Yes,
    /// The negative pole.
    No,
    /// The model abstained or answered nothing parseable.
    Abstain,
}

impl JudgeVerdict {
    /// The lowercase answer word, the same word the ledger row's outcome
    /// carries after the `judge:` prefix.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
            Self::Abstain => ABSTAIN_MARKER,
        }
    }
}

impl std::fmt::Display for JudgeVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which path produced a judge outcome: the first try answered, the one
/// allowed retry answered, or neither did and the safe default applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgePath {
    /// The first completion parsed to a verdict.
    FirstTry,
    /// The first completion abstained; the retry parsed to a verdict.
    Retry,
    /// Both completions abstained (or the call never completed); the
    /// safe-default table answered.
    AbstainDefault,
}

impl JudgePath {
    /// The snake_case wire word.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FirstTry => "first_try",
            Self::Retry => "retry",
            Self::AbstainDefault => "abstain_default",
        }
    }
}

impl std::fmt::Display for JudgePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One judge call's result: the verdict that stands and the path that
/// produced it, so an audit can tell a model's `No` from the table's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JudgeOutcome {
    /// The verdict the caller acts on.
    pub verdict: JudgeVerdict,
    /// How the verdict was reached.
    pub path: JudgePath,
}

impl JudgeOutcome {
    /// The retry state machine (the binary judge spec's
    /// retry-once-then-default rule): a decisive first try stands alone, a
    /// decisive retry wins with [`JudgePath::Retry`], and a retry that
    /// still abstains resolves to `default` under
    /// [`JudgePath::AbstainDefault`]. The `retried` verdict is consulted
    /// only when `first` abstained; callers retry nothing else.
    #[must_use]
    pub fn resolve(first: JudgeVerdict, retried: JudgeVerdict, default: JudgeVerdict) -> Self {
        match (first, retried) {
            (verdict @ (JudgeVerdict::Yes | JudgeVerdict::No), _) => Self {
                verdict,
                path: JudgePath::FirstTry,
            },
            (JudgeVerdict::Abstain, verdict @ (JudgeVerdict::Yes | JudgeVerdict::No)) => Self {
                verdict,
                path: JudgePath::Retry,
            },
            (JudgeVerdict::Abstain, JudgeVerdict::Abstain) => Self {
                verdict: default,
                path: JudgePath::AbstainDefault,
            },
        }
    }
}

/// The judge family's use cases: the one-bit decisions the A1 through A4
/// slices judge. Each names the [`LlmUseCase`] its calls ledger under (the
/// registry grew the same-named variants additively) and inherits its tier
/// from there: the classification-sized judges on nano, the rule-match and
/// eval judges on economy, none on chat or draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeUseCase {
    /// Is this message cold outreach? Abstains to not cold: the mail stays
    /// put (A1).
    JudgeColdEmail,
    /// Does this rule's condition match the message? Abstains to no match:
    /// the rule does not run (A1).
    JudgeRuleMatch,
    /// May this content leave the machine for the model? Abstains to
    /// block: privacy errs shut (A1).
    JudgeDlp,
    /// Is this content free of prompt injection? Abstains to hostile:
    /// security errs closed (A1).
    JudgeInjection,
    /// Should this drafted reply be surfaced? Abstains to unsurfaced: the
    /// draft still exists in the folder (A2).
    JudgeDraftSurface,
    /// Is the summary faithful to the source? Abstains to a failed eval:
    /// evals err strict (A1's harness).
    JudgeSummaryFaithful,
    /// Did the scenario behave as scripted? Abstains to a failed scenario
    /// (A1's harness).
    JudgeScenario,
}

impl JudgeUseCase {
    /// The snake_case wire word, shared with the [`LlmUseCase`] variant it
    /// ledgers under.
    pub fn as_str(&self) -> &'static str {
        self.llm_use_case().as_str()
    }

    /// The registry use case the judge's ledger rows carry.
    pub fn llm_use_case(&self) -> LlmUseCase {
        match self {
            Self::JudgeColdEmail => LlmUseCase::JudgeColdEmail,
            Self::JudgeRuleMatch => LlmUseCase::JudgeRuleMatch,
            Self::JudgeDlp => LlmUseCase::JudgeDlp,
            Self::JudgeInjection => LlmUseCase::JudgeInjection,
            Self::JudgeDraftSurface => LlmUseCase::JudgeDraftSurface,
            Self::JudgeSummaryFaithful => LlmUseCase::JudgeSummaryFaithful,
            Self::JudgeScenario => LlmUseCase::JudgeScenario,
        }
    }

    /// The tier the judge's calls resolve their chains through.
    pub fn tier(&self) -> ModelTier {
        self.llm_use_case().tier()
    }
}

impl std::fmt::Display for JudgeUseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The action a consumer takes when its judge abstains, in the consumer's
/// own vocabulary (the binary judge spec's table): the domain meaning the
/// bare verdict cannot carry. Judge questions are phrased so `Yes` is the
/// positive pole (proceed, match, safe, pass); every default therefore
/// resolves to the `No` pole, and the variant names what that No does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeDefault {
    /// Do nothing: the negative pole is itself the safe action.
    No,
    /// Block the content from leaving: privacy errs shut.
    Block,
    /// Treat the content as hostile and wrap harder: security errs closed.
    Hostile,
    /// Fail the eval or the scenario: evals err strict.
    Fail,
}

impl JudgeDefault {
    /// The verdict pole the default resolves to when the judge must answer
    /// binary: always [`JudgeVerdict::No`] under the positive-pole
    /// question convention.
    #[must_use]
    pub fn verdict(self) -> JudgeVerdict {
        match self {
            Self::No | Self::Block | Self::Hostile | Self::Fail => JudgeVerdict::No,
        }
    }

    /// The snake_case wire word.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::No => "no",
            Self::Block => "block",
            Self::Hostile => "hostile",
            Self::Fail => "fail",
        }
    }
}

impl std::fmt::Display for JudgeDefault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The safe-default table, one row per judge use case, exactly as the
/// binary judge spec's table reads. A judge use case without a row here
/// fails [`safe_default`]'s exhaustive match at compilation and the pin
/// tests at runtime; a new use case cannot ship without a default.
pub const SAFE_DEFAULTS: [(JudgeUseCase, JudgeDefault); 7] = [
    (JudgeUseCase::JudgeColdEmail, JudgeDefault::No),
    (JudgeUseCase::JudgeRuleMatch, JudgeDefault::No),
    (JudgeUseCase::JudgeDlp, JudgeDefault::Block),
    (JudgeUseCase::JudgeInjection, JudgeDefault::Hostile),
    (JudgeUseCase::JudgeDraftSurface, JudgeDefault::No),
    (JudgeUseCase::JudgeSummaryFaithful, JudgeDefault::Fail),
    (JudgeUseCase::JudgeScenario, JudgeDefault::Fail),
];

/// The safe default for a judge use case, from [`SAFE_DEFAULTS`]. The
/// match is exhaustive over [`JudgeUseCase`]: adding a variant without a
/// default fails compilation, which is the point. One arm per use case,
/// in table order, even where arms share a body: merged patterns would
/// hide which use cases the table pins individually.
#[must_use]
#[allow(clippy::match_same_arms)]
pub fn safe_default(use_case: JudgeUseCase) -> JudgeDefault {
    match use_case {
        JudgeUseCase::JudgeColdEmail => JudgeDefault::No,
        JudgeUseCase::JudgeRuleMatch => JudgeDefault::No,
        JudgeUseCase::JudgeDlp => JudgeDefault::Block,
        JudgeUseCase::JudgeInjection => JudgeDefault::Hostile,
        JudgeUseCase::JudgeDraftSurface => JudgeDefault::No,
        JudgeUseCase::JudgeSummaryFaithful => JudgeDefault::Fail,
        JudgeUseCase::JudgeScenario => JudgeDefault::Fail,
    }
}

/// True for the characters trimmed from a completion before the strict
/// comparison: whitespace and the punctuation a model decorates a single
/// word with. Only the edges are trimmed; inner punctuation keeps a
/// multi-word answer out, as it must be.
fn is_trimmed_away(character: char) -> bool {
    character.is_whitespace() || ".,!?;:'\"`*".contains(character)
}

/// Parses one completion as a judge verdict, strictly: `yes`, `no`, or the
/// [`ABSTAIN_MARKER`] word, case-insensitive after trimming edge
/// whitespace and punctuation; anything else, including prose, emphasis,
/// and empty text, is [`JudgeVerdict::Abstain`]. There is no JSON
/// envelope and no repair loop: the single-token form is the whole point.
#[must_use]
pub fn parse_judgement(raw: &str) -> JudgeVerdict {
    let trimmed = raw.trim_matches(is_trimmed_away);
    if trimmed.eq_ignore_ascii_case("yes") {
        return JudgeVerdict::Yes;
    }
    if trimmed.eq_ignore_ascii_case("no") {
        return JudgeVerdict::No;
    }
    JudgeVerdict::Abstain
}

/// Builds the versioned single-token judge prompt: the caller's question,
/// the context wrapped by the hardening wrapper (untrusted content stays
/// data), and the strict instruction whose wording the template version
/// pins. Questions are phrased so `Yes` is the positive pole; the
/// safe-default table relies on it.
#[must_use]
pub fn judge_prompt(question: &str, context: &str) -> String {
    format!(
        "{question}\n\n{}\n\nAnswer with exactly one word: yes, no, or {ABSTAIN_MARKER}.",
        tag_untrusted("judge context", context)
    )
}
