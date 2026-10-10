// SPDX-License-Identifier: GPL-3.0-only
//! Rule generation: one versioned prompt, one completion through the
//! [`AiComplete`](super::AiComplete) seam, and one strict parse that
//! repairs once through the AI foundation's `repair_json` when the model
//! answered with prose around the JSON (the inbox-zero
//! `aiPromptToRules` shape over the closed action set).

use astra_ai::{repair_json, CorrelationId, LlmUseCase, RuleId};
use serde::Deserialize;

use super::{Action, AiComplete, Rule, RulesError};

/// The version of the rules generation prompt. Any change to what
/// [`generation_prompt`] sends, instruction wording included, bumps this
/// constant and reruns the generation evals (the rulebook's AI rule; the
/// eval harness lands later in A1). It rides the prompt the way
/// `JUDGE_TEMPLATES_VERSION` rides the judge template: a constant beside
/// the builder, observed through the ledger's use case and the eval
/// runner, not text baked into the model's prompt.
pub const RULES_PROMPT_VERSION: &str = "1";

/// The completion's parsed shape: `{"rules": [...]}`, the object the
/// output policy pins. Unknown fields are tolerated so the shape grows
/// additively; unknown actions are not, being the closed set.
#[derive(Debug, Deserialize)]
struct GeneratedRules {
    /// The generated rules, in the model's order.
    rules: Vec<GeneratedRule>,
}

/// One generated rule before its identity is minted.
#[derive(Debug, Deserialize)]
struct GeneratedRule {
    /// The rule's short name.
    name: String,
    /// The natural language condition sentence.
    condition: String,
    /// The actions to take, every element of the closed set.
    actions: Vec<Action>,
}

/// Generates rules from `text`: builds the versioned prompt, sends it
/// through the seam under the turn's `correlation` (the host's ledger
/// rows ride the id), and parses the completion strictly, repairing once
/// through `repair_json` on a bad parse. Every generated rule arrives
/// enabled with a freshly minted id; storing is the caller's step.
///
/// The prompt text is user-authored instructions, not mail content, so it
/// rides unwrapped (inbox-zero marks the prompt file trusted); the stages
/// that carry message content wrap it with the hardening wrapper.
///
/// # Errors
/// [`RulesError::Completion`] when the seam fails;
/// [`RulesError::MalformedOutput`] when no parse or repair saves the
/// completion, an action outside the closed set included.
pub async fn prompt_to_rules(
    text: &str,
    correlation: &CorrelationId,
    client: &dyn AiComplete,
) -> Result<Vec<Rule>, RulesError> {
    let prompt = generation_prompt(text);
    let completion = client
        .complete(LlmUseCase::PromptToRules, &prompt, correlation)
        .await
        .map_err(RulesError::Completion)?;
    parse_generated(&completion).map(|generated| {
        generated
            .rules
            .into_iter()
            .map(|rule| Rule {
                id: RuleId::generate(),
                name: rule.name,
                condition: rule.condition,
                actions: rule.actions,
                enabled: true,
            })
            .collect()
    })
}

/// Builds the versioned generation prompt (inbox-zero's system instruction
/// trimmed to the closed action set): short names, one condition sentence,
/// the actions drawn only from the closed set, and the output policy that
/// demands the JSON object alone. The user text rides wrapped and intact.
#[must_use]
pub fn generation_prompt(text: &str) -> String {
    let closed = Action::ALL
        .iter()
        .map(|action| format!("\"{}\"", action.as_str()))
        .collect::<Vec<_>>()
        .join(" and ");
    format!(
        "You convert email management instructions into mail rules.\n\
         \n\
         Use short, concise rule names, preferably a single word: 'Marketing', \
         'Newsletters', 'Urgent', 'Receipts'. Avoid verbose names.\n\
         Each rule's condition is one plain sentence describing the messages it \
         matches. Keep conditions short and specific, and use a quoted snippet in \
         full when the instructions quote one.\n\
         Split instructions that ask for different things into separate rules.\n\
         The actions are a list drawn from the closed set {closed}. Never invent \
         an action outside the set.\n\
         \n\
         Output policy:\n\
         - Return a JSON object only: no prose, no markdown.\n\
         - The output must match the schema exactly: \
         {{\"rules\":[{{\"name\":\"...\",\"condition\":\"...\",\"actions\":[\"mark_read\"]}}]}}.\n\
         \n\
         Convert the following instructions into rules:\n\
         \n\
         <prompt>\n\
         {text}\n\
         </prompt>"
    )
}

/// Parses one completion as the generated rules: a strict parse first, and
/// on failure exactly one repair through the first balanced JSON object,
/// the shared recovery path. The serde error of whichever parse got
/// furthest is the failure's detail.
fn parse_generated(completion: &str) -> Result<GeneratedRules, RulesError> {
    serde_json::from_str(completion)
        .or_else(|direct| repair_json(completion).map_or(Err(direct), serde_json::from_str))
        .map_err(|source| RulesError::MalformedOutput(source.to_string()))
}
