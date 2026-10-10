// SPDX-License-Identifier: GPL-3.0-only
//! The per-account style profile: one versioned structured completion
//! over the account's recent Sent mail that becomes the short style note
//! the drafting prompt carries as data (the A2 spec's D5). The caller
//! supplies the collected Sent samples (the Sent-folder read is the
//! arming's wiring, not the profile's); this module caps them, wraps
//! every sample as untrusted mail (the user's own mail is mail all the
//! same), infers the note through the [`AiComplete`] seam under the
//! turn's correlation, and saves it to the style profile store
//! (migration 0004's sibling table). The built_at daily cap keeps a
//! fresh profile from spending a call: only a profile older than a day,
//! an absent one, or a forced rebuild infers again.

use astra_ai::{repair_json, tag_untrusted, AiStore, CorrelationId, LlmUseCase, StyleProfileRow};
use engine_api::AccountId;
use serde::Deserialize;

use crate::ai_rules::AiComplete;

/// The version of the style inference prompt. Any change to what
/// [`style_prompt`] sends, instruction wording included, bumps this
/// constant and reruns the style evals (the rulebook's AI rule; the eval
/// pins land with the live slice).
pub const STYLE_PROMPT_VERSION: &str = "1";

/// How many recent Sent messages ride one inference: the evidence a
/// short note needs, newest first from the caller's collection.
pub const STYLE_SAMPLE_LIMIT: usize = 20;

/// How many body characters each Sent sample rides: the per-sample share
/// of the prompt's mail budget, the drafting collector's precedent.
pub const STYLE_SAMPLE_BODY_CAP: usize = 2000;

/// How old a stored profile may be before a read rebuilds it: the daily
/// cap the spec pins, demand-driven only (no background timer).
pub const STYLE_PROFILE_MAX_AGE_SECS: i64 = 86_400;

/// The marker a truncated sample carries, so the model knows the message
/// continues past what it can see (the collector's convention).
const TRUNCATION_MARKER: &str = "[truncated]";

/// The completion's parsed shape: `{"style_note": "..."}`, the object the
/// output policy pins. Unknown fields are tolerated so the shape grows
/// additively.
#[derive(Debug, Deserialize)]
struct Inferred {
    /// The account's short style description.
    style_note: String,
}

/// Reads the account's style profile, inferring it fresh when nothing is
/// stored, the stored build is older than
/// [`STYLE_PROFILE_MAX_AGE_SECS`], or `force` says so (the CLI's rebuild
/// verb). A profile built within the day answers from the store with no
/// seam call; an empty Sent collection never infers, keeping whatever
/// profile the store holds (no Sent mail, no profile, and an empty folder
/// never erases a built one). An infer that fails or will not parse falls
/// back to the stored note, a stale note beating none, and saves nothing.
pub async fn style_profile(
    account: &AccountId,
    sent: &[String],
    force: bool,
    complete: &dyn AiComplete,
    correlation: &CorrelationId,
    store: &AiStore,
) -> Option<String> {
    let stored = read_stored(store, account);
    if !should_rebuild(stored.as_ref(), force, now_secs()) {
        return stored.map(|row| row.note);
    }
    if sent.is_empty() {
        return stored.map(|row| row.note);
    }
    let prompt = style_prompt(sent);
    let inferred = match complete
        .complete(LlmUseCase::StyleInfer, &prompt, correlation)
        .await
    {
        Ok(completion) => parse_style(&completion),
        Err(err) => {
            log::warn!("ai replies: the style infer call failed; the stored profile stands: {err}");
            None
        }
    };
    match inferred {
        Some(note) => {
            if let Err(err) = store.save_style_profile(account.as_str(), &note) {
                log::warn!("ai replies: the style profile could not be saved: {err}");
            }
            Some(note)
        }
        None => stored.map(|row| row.note),
    }
}

/// Whether a read infers afresh: nothing stored, the daily cap past, or
/// the caller forcing. A stamp in the future (clock skew between the
/// build and the read) reads fresh, spending nothing.
pub(super) fn should_rebuild(stored: Option<&StyleProfileRow>, force: bool, now: i64) -> bool {
    if force {
        return true;
    }
    stored.is_none_or(|row| now - row.built_at > STYLE_PROFILE_MAX_AGE_SECS)
}

/// Builds the versioned inference prompt: the instruction (the note's
/// dimensions and the no-identifying-details rule) and the JSON output
/// policy ride the trusted half, and every Sent sample rides its own
/// untrusted wrap, oldest first, each body capped with the truncation
/// marker when it ran over.
#[must_use]
pub fn style_prompt(sent: &[String]) -> String {
    let mut prompt = String::from(
        "You describe how one email user writes, from samples of their own sent mail below.\n\
         The note you write will be read by another assistant drafting replies the user \
         reviews before sending.\n\
         \n\
         Describe only patterns that repeat across the samples:\n\
         - tone and formality\n\
         - greeting and sign-off habits\n\
         - typical length and structure\n\
         - any consistent formatting habits\n\
         Do not mention names, addresses, companies, or other identifying details.\n\
         Keep the note short: a few sentences at most.\n\
         \n\
         Output policy:\n\
         - Return a JSON object only: no prose, no markdown.\n\
         - The output must match the schema exactly: {\"style_note\":\"...\"}.\n\
         \n\
         Here are the user's sent messages, oldest first:\n",
    );
    for sample in kept_samples(sent) {
        prompt.push_str(&tag_untrusted("sent email", &capped(sample)));
        prompt.push('\n');
    }
    prompt.push_str("\nWrite the style note from the sent messages above.\n");
    prompt
}

/// The samples the prompt keeps: the newest [`STYLE_SAMPLE_LIMIT`]
/// entries of the newest-first list the caller collected, flipped to
/// oldest-first for rendering.
fn kept_samples(sent: &[String]) -> impl Iterator<Item = &String> {
    sent.iter().take(STYLE_SAMPLE_LIMIT).rev()
}

/// `sample`'s head to the cap with the truncation marker when it ran
/// over, char boundaries respected; an under-cap sample rides unchanged.
fn capped(sample: &str) -> String {
    if sample.chars().count() <= STYLE_SAMPLE_BODY_CAP {
        return sample.to_owned();
    }
    let head: String = sample.chars().take(STYLE_SAMPLE_BODY_CAP).collect();
    format!("{head}\n{TRUNCATION_MARKER}")
}

/// The stored profile; a read that fails answers none with a warning,
/// letting the infer path decide what to do rather than failing the read.
fn read_stored(store: &AiStore, account: &AccountId) -> Option<StyleProfileRow> {
    match store.style_profile(account.as_str()) {
        Ok(row) => row,
        Err(err) => {
            log::warn!("ai replies: the style profile read failed: {err}");
            None
        }
    }
}

/// The wall clock in unix seconds, the shape the store stamps `built_at`
/// in; `None` only if the clock reads before the epoch, which reads as no
/// stamp worth comparing and so rebuilds nothing.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs().cast_signed())
}

/// Parses one completion as the style note: a strict parse first, and on
/// failure exactly one repair through the first balanced JSON object, the
/// shared recovery path; `None` when neither parse lands.
fn parse_style(completion: &str) -> Option<String> {
    serde_json::from_str(completion)
        .or_else(|direct| repair_json(completion).map_or(Err(direct), serde_json::from_str))
        .map(|inferred: Inferred| inferred.style_note)
        .map_err(|err| {
            log::warn!(
                "ai replies: the style output was unparseable; the stored profile stands: {err}"
            );
            err
        })
        .ok()
}
