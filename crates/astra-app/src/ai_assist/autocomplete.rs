// SPDX-License-Identifier: GPL-3.0-only
//! The compose autocomplete suggestion: one versioned completion through
//! the [`AiComplete`] seam that answers the short continuation the
//! editor offers at the cursor. The prompt carries the editor's text
//! around the cursor in two labelled halves, each in its own untrusted
//! wrap (the text may quote mail, so it rides as data, never as
//! instruction); the trailing half is optional and its block is omitted
//! when the cursor sits at the end of the text. The answer is the
//! model's text trimmed, no JSON on the latency path; a failed call
//! answers an empty string, never a crash, and the editor's chip never
//! inserts without the user's gesture, which is the caller's discipline.

use astra_ai::{tag_untrusted, CorrelationId, LlmUseCase};

use crate::ai_rules::AiComplete;

/// The version of the autocomplete prompt. Any change to what
/// [`autocomplete_prompt`] sends, instruction wording included, bumps
/// this constant and reruns the autocomplete evals (the rulebook's AI
/// rule; the eval pins land with their slice). It rides beside the
/// builder the way the drafting and judge constants do, observed through
/// the eval runner, not text baked into the model's prompt.
pub const AUTOCOMPLETE_VERSION: &str = "1";

/// Answers the continuation for the text around the cursor: builds the
/// versioned prompt over `before` and the optional `after`, sends it
/// through the seam under the turn's `correlation` (the autocomplete use
/// case, chat tier for latency), and answers the completion trimmed. A
/// failed call answers an empty string with a log, so the editor's chip
/// simply never shows; the caller decides when to ask.
pub async fn autocomplete(
    before: &str,
    after: &str,
    correlation: &CorrelationId,
    complete: &dyn AiComplete,
) -> String {
    let prompt = autocomplete_prompt(before, after);
    match complete
        .complete(LlmUseCase::Autocomplete, &prompt, correlation)
        .await
    {
        Ok(completion) => completion.trim().to_owned(),
        Err(err) => {
            log::warn!("ai assist: the autocomplete call failed; no suggestion rides: {err}");
            String::new()
        }
    }
}

/// Builds the versioned autocomplete prompt: the instruction (one short
/// continuation, its length cap named, no rewriting of the text before
/// the cursor) rides the trusted half, and the editor's two halves each
/// ride their own untrusted wrap under a trusted label, the trailing
/// half's block omitted entirely when the cursor has no text after it.
#[must_use]
pub fn autocomplete_prompt(before: &str, after: &str) -> String {
    let mut prompt = String::from(
        "You continue an email the user is writing, from the cursor position between the text \
         blocks below.\n\
         \n\
         Write only the continuation: the next few words the user would most likely type at the \
         cursor, no more than about ten words.\n\
         Do not repeat or rewrite the text that already sits before the cursor.\n\
         Match the language and the tone of the existing text.\n\
         When text after the cursor is present, write a continuation that joins naturally onto \
         it.\n\
         \n\
         The text before the cursor:\n",
    );
    prompt.push_str(&tag_untrusted("editor text before the cursor", before));
    if !after.trim().is_empty() {
        prompt.push_str("\n\nThe text after the cursor:\n");
        prompt.push_str(&tag_untrusted("editor text after the cursor", after));
    }
    prompt.push_str("\n\nReturn only the continuation words, nothing else.\n");
    prompt
}
