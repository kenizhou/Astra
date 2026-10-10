// SPDX-License-Identifier: GPL-3.0-only
//! Sender categorisation: one versioned structured completion per message
//! through the [`AiComplete`] seam, one of the five-way closed set back
//! (the Ai rules spec's D4). The prompt keeps its halves apart the way the
//! judge's does: the instruction and the output policy are trusted text,
//! and the mail context rides the untrusted wrap the AI foundation's
//! hardening helper draws, so a message can never speak as an instruction.

use astra_ai::{repair_json, tag_untrusted, AiStore, CorrelationId, LlmUseCase, NewAiResult};
use serde::Deserialize;

use super::matching::{judge_context, MailContext};
use super::AiComplete;
use crate::ai_results::AiStage;

/// The version of the categorisation prompt. Any change to what
/// [`categorise_prompt`] sends, instruction wording included, bumps this
/// constant and reruns the categorisation evals (the rulebook's AI rule;
/// the eval harness lands later in A1). It rides beside the builder the
/// way the rules and judge constants do, observed through the eval
/// runner, not text baked into the model's prompt.
pub const CATEGORISE_PROMPT_VERSION: &str = "1";

/// The stage's verdict word when a completion decided.
const VERDICT_OK: &str = "ok";
/// The stage's verdict word when no decision landed: the failed or
/// unparseable call, recorded honestly rather than guessed.
const VERDICT_SKIPPED: &str = "skipped";

/// One message's category, the five-way closed set the Ai rules spec pins.
/// Wire words are snake_case; an unknown word is rejected loudly at parse,
/// never silently dropped or defaulted: a completion that names a category
/// this build does not know fails its categorisation and the stage records
/// itself skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// A person's own mail, written to the recipient by hand.
    Primary,
    /// A recurring bulk mailing the recipient opted into or could have.
    Newsletter,
    /// A transaction record, an invoice, or a shipping notice.
    Receipt,
    /// An automated service notice an action or a schedule triggered.
    Notification,
    /// An unsolicited outreach from a sender with no prior relationship.
    Cold,
}

impl Category {
    /// The closed set, in wire order: the one list the categorisation
    /// prompt teaches and the pins cross-check.
    pub const ALL: [Self; 5] = [
        Self::Primary,
        Self::Newsletter,
        Self::Receipt,
        Self::Notification,
        Self::Cold,
    ];

    /// The snake_case wire word.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Newsletter => "newsletter",
            Self::Receipt => "receipt",
            Self::Notification => "notification",
            Self::Cold => "cold",
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The completion's parsed shape: `{"category": "..."}`, the object the
/// output policy pins. Unknown fields are tolerated so the shape grows
/// additively; an unknown category is not, being the closed set.
#[derive(Debug, Deserialize)]
struct Categorised {
    /// The assigned category.
    category: Category,
}

/// Failures of the categorisation stage: the seam's completion error and
/// an output no parse or repair could save, a category outside the closed
/// set among them (its word quoted by the detail).
#[derive(Debug, thiserror::Error)]
pub enum CategoriseError {
    /// The completion seam failed; the message is the host's.
    #[error("categorisation call failed: {0}")]
    Completion(String),
    /// The completion did not parse as the category JSON shape, even after
    /// the repair path.
    #[error("categorisation output unparseable: {0}")]
    MalformedOutput(String),
}

/// Categorises `mail`: builds the versioned prompt over the capped mail
/// context, sends it through the seam under the pass's `correlation` (the
/// categorise use case, default tier), and parses the completion strictly,
/// repairing once through `repair_json` on a bad parse. The stage records
/// its own 0003 row as it lands: the assigned category on a decision, and
/// a skipped row on every failure, so the pass survives a bad completion
/// without ever guessing a category.
///
/// # Errors
/// [`CategoriseError::Completion`] when the seam fails;
/// [`CategoriseError::MalformedOutput`] when no parse or repair saves the
/// completion, an unknown category word included.
pub async fn categorise(
    mail: &MailContext,
    correlation: &CorrelationId,
    complete: &dyn AiComplete,
    results: &AiStore,
) -> Result<Category, CategoriseError> {
    let prompt = categorise_prompt(&judge_context(mail));
    let outcome = match complete
        .complete(LlmUseCase::CategorizeSender, &prompt, correlation)
        .await
    {
        Ok(completion) => parse_category(&completion),
        Err(source) => Err(CategoriseError::Completion(source)),
    };
    let row = match &outcome {
        Ok(category) => NewAiResult {
            correlation: correlation.clone(),
            message_account: mail.account.as_str().to_owned(),
            message_key: mail.key.as_str().to_owned(),
            stage: AiStage::Categorise.as_str().to_owned(),
            rule_id: None,
            verdict: VERDICT_OK.to_owned(),
            category: Some(category.as_str().to_owned()),
        },
        Err(_) => NewAiResult {
            correlation: correlation.clone(),
            message_account: mail.account.as_str().to_owned(),
            message_key: mail.key.as_str().to_owned(),
            stage: AiStage::Categorise.as_str().to_owned(),
            rule_id: None,
            verdict: VERDICT_SKIPPED.to_owned(),
            category: None,
        },
    };
    if let Err(err) = results.record_ai_result(&row) {
        log::warn!("ai rules: the categorise result could not be recorded: {err}");
    }
    outcome
}

/// Builds the versioned categorisation prompt: the closed set with one
/// definition line each, the JSON output policy, and the mail context
/// wrapped by the hardening wrapper (untrusted content stays data, the
/// same contract the judge's context half carries).
#[must_use]
pub fn categorise_prompt(context: &str) -> String {
    let set = Category::ALL
        .iter()
        .map(Category::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "You sort one incoming message into exactly one category.\n\
         The closed set is {set}, one definition each:\n\
         primary: a person's own mail, written to the recipient by hand.\n\
         newsletter: a recurring bulk mailing the recipient opted into or could have.\n\
         receipt: a transaction record, an invoice, or a shipping notice.\n\
         notification: an automated service notice an action or a schedule triggered.\n\
         cold: an unsolicited outreach from a sender with no prior relationship, selling something.\n\
         \n\
         Output policy:\n\
         - Return a JSON object only: no prose, no markdown.\n\
         - The output must match the schema exactly: \
         {{\"category\":\"primary\"}}.\n\
         \n\
         Categorise the message in the context:\n\
         \n\
         {}",
        tag_untrusted("email", context),
    )
}

/// Parses one completion as the category: a strict parse first, and on
/// failure exactly one repair through the first balanced JSON object, the
/// shared recovery path. The serde error of whichever parse got furthest
/// is the failure's detail.
fn parse_category(completion: &str) -> Result<Category, CategoriseError> {
    serde_json::from_str(completion)
        .or_else(|direct| repair_json(completion).map_or(Err(direct), serde_json::from_str))
        .map(|categorised: Categorised| categorised.category)
        .map_err(|source| CategoriseError::MalformedOutput(source.to_string()))
}
