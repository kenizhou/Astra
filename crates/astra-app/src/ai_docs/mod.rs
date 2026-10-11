// SPDX-License-Identifier: GPL-3.0-only
//! The AI documents model: the A4 services over the A1 seams. This
//! module owns the on-demand digest of an account's recent mail, the
//! analytics report over the same window, and the split inbox's
//! derivation from the stored categorisation; the host commands, the
//! reading surface's panel, and the CLI verbs arrive in their own
//! slices and consume these services through the same
//! [`AiComplete`](crate::ai_rules::AiComplete) seam the rules, replies,
//! and assist models ride. Both documents build their inputs from the
//! window's messages and the stored categorisation rows the incoming
//! pass recorded; the split adds no model call at all, being pure
//! derivation over the same rows.

mod digest;
mod drive;
mod report;
mod split;

#[cfg(test)]
#[path = "digest_tests.rs"]
mod digest_tests;

#[cfg(test)]
#[path = "report_tests.rs"]
mod report_tests;

#[cfg(test)]
#[path = "split_tests.rs"]
mod split_tests;

pub use digest::{digest_prompt, generate_digest, DigestError, DIGEST_PROMPT_VERSION};
pub use drive::{
    window_bounds, DigestView, DocumentError, DocumentView, SplitEntry,
    DOCUMENT_WINDOW_DEFAULT_DAYS, DOCUMENT_WINDOW_READ_LIMIT,
};
pub use report::{generate_report, report_prompt, ReportError, REPORT_PROMPT_VERSION};
pub use split::{side_for, split_map, SplitSide, SPLIT_MAPPING, SPLIT_MAPPING_VERSION};

use std::collections::BTreeMap;
use std::fmt::Write as _;

use astra_ai::tag_untrusted;
use engine_api::ProviderKey;

use crate::ai_rules::Category;

/// One message as the document services see it: the identity and text
/// one generation window collected, newest first from the caller, with
/// the unread flag the report's statistics count. The caller builds
/// these from the engine's window reads and body fetches; the fields
/// are plain so the service tests need no engine at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocMessage {
    /// The message's provider key, unique within its account.
    pub key: ProviderKey,
    /// The sender's display name, empty when the header carried none.
    pub from_name: String,
    /// The sender's address as the header spelled it.
    pub from_address: String,
    /// The subject, empty when the message carries none.
    pub subject: String,
    /// The plain-text body, uncapped: the prompt builder applies the
    /// cap.
    pub body: String,
    /// Whether the window read carried the message unread.
    pub unread: bool,
}

/// The stored categorisation the rollups and the split ride over: one
/// decided categorise row per message, keyed by the message's provider
/// key, the stage's category word exactly as the row holds it. A word
/// outside the closed set stays data: the rollup names it under its own
/// word and the split excludes it.
pub type StoredCategories = BTreeMap<ProviderKey, String>;

/// The generation window's ceiling in days: every digest and report
/// window clamps to one day at the floor and the spec's thirty-one at
/// the ceiling, whatever the caller asked.
pub const DOCUMENT_WINDOW_MAX_DAYS: u32 = 31;

/// How many body characters each message rides the prompts: the
/// per-message share of the prompt's mail budget, the knowledge and
/// drafting collectors' cap.
pub const DOCUMENT_BODY_CAP: usize = 2000;

/// How many messages each category's listing carries, the newest first:
/// the spec's head-five pin, the bound that keeps one fat category from
/// spending the whole prompt.
pub const DOCUMENT_HEAD_LIMIT: usize = 5;

/// The bucket word for a message with no stored category: the rollup's
/// honest name for the uncategorised.
const UNCATEGORISED_WORD: &str = "uncategorised";

/// The marker a truncated body carries, so the model knows the message
/// continues past what it can see (the collectors' convention).
const TRUNCATION_MARKER: &str = "[truncated]";

/// The days one generation window covers, clamped to the floor of one
/// and the ceiling [`DOCUMENT_WINDOW_MAX_DAYS`] pins: the one clamp
/// every digest and report window reads through, whatever the caller
/// asked for.
fn clamped_days(days: u32) -> u32 {
    days.clamp(1, DOCUMENT_WINDOW_MAX_DAYS)
}

/// One rollup bucket: the category word the bucket answers to and the
/// window's messages it holds, newest first exactly as the caller
/// collected them.
struct RollupBucket<'a> {
    /// The bucket's category word: one of the closed set, a foreign
    /// word the store held, or the uncategorised bucket's own.
    word: String,
    /// The bucket's messages, newest first.
    messages: Vec<&'a DocMessage>,
}

/// The rollup over the window's messages: each message lands in the
/// bucket its stored category names, looked up by the message's
/// provider key, and a message with no stored category lands in the
/// uncategorised bucket. Buckets answer in the closed set's order, any
/// foreign word sorted behind it, uncategorised last.
fn rollup<'a>(
    messages: &'a [DocMessage],
    stored_categories: &StoredCategories,
) -> Vec<RollupBucket<'a>> {
    let mut by_word: BTreeMap<&str, Vec<&DocMessage>> = BTreeMap::new();
    for message in messages {
        let word = stored_categories
            .get(&message.key)
            .map_or(UNCATEGORISED_WORD, String::as_str);
        by_word.entry(word).or_default().push(message);
    }
    let uncategorised = by_word.remove(UNCATEGORISED_WORD);
    let mut buckets = Vec::with_capacity(by_word.len() + 1);
    for category in Category::ALL {
        if let Some(messages) = by_word.remove(category.as_str()) {
            buckets.push(RollupBucket {
                word: category.as_str().to_owned(),
                messages,
            });
        }
    }
    for (word, messages) in by_word {
        buckets.push(RollupBucket {
            word: word.to_owned(),
            messages,
        });
    }
    if let Some(messages) = uncategorised {
        buckets.push(RollupBucket {
            word: UNCATEGORISED_WORD.to_owned(),
            messages,
        });
    }
    buckets
}

/// The rollup as the prompts' statistics lines render it: one
/// `word: count` line per bucket.
fn rollup_lines(buckets: &[RollupBucket<'_>]) -> String {
    buckets
        .iter()
        .map(|bucket| format!("{}: {}", bucket.word, bucket.messages.len()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The buckets' messages as both prompts render their listings: per
/// bucket a trusted category line naming the true count and the head
/// count, then the head `head_limit` messages newest first, each in its
/// own untrusted wrap with the sender, the subject, and the capped
/// body, so a mail body can never speak as an instruction.
fn grouped_messages(buckets: &[RollupBucket<'_>], head_limit: usize) -> String {
    let mut out = String::new();
    for bucket in buckets {
        let shown = bucket.messages.len().min(head_limit);
        let _ = writeln!(
            out,
            "Category {} ({} message(s), the newest {} shown):",
            bucket.word,
            bucket.messages.len(),
            shown,
        );
        for message in &bucket.messages[..shown] {
            let _ = write!(
                out,
                "\n{}\n",
                tag_untrusted("email", &rendered_message(message)),
            );
        }
    }
    out
}

/// One message as the prompts render it: the sender, the subject, and
/// the body capped at [`DOCUMENT_BODY_CAP`] characters with the
/// truncation marker when the message continues past the cap.
fn rendered_message(message: &DocMessage) -> String {
    let from = if message.from_name.trim().is_empty() {
        message.from_address.clone()
    } else {
        format!("{} <{}>", message.from_name, message.from_address)
    };
    format!(
        "From: {from}\nSubject: {}\n\n{}",
        message.subject,
        capped(&message.body),
    )
}

/// `body`'s head to the cap with the truncation marker when it ran
/// over, char boundaries respected; an under-cap body rides unchanged.
fn capped(body: &str) -> String {
    if body.chars().count() <= DOCUMENT_BODY_CAP {
        return body.to_owned();
    }
    let head: String = body.chars().take(DOCUMENT_BODY_CAP).collect();
    format!("{head}\n{TRUNCATION_MARKER}")
}
