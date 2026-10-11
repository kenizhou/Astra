// SPDX-License-Identifier: GPL-3.0-only
//! The split inbox's derivation: no model call at all, just the pinned
//! mapping from the stored categorisation's category words to the two
//! sides of the split (the A4 design's D3, mail-calendar's feature
//! contract). The mapping is DATA, one table of word-to-side pairs, so
//! the decision lives in exactly one place and its version
//! ([`SPLIT_MAPPING_VERSION`]) pins it: a mapping change is an eval
//! rerun, the prompt versions' own rule. A word the table does not name
//! is excluded, so its message appears in All only, and a message with
//! no stored category never enters the map at all.

use engine_api::ProviderKey;
use serde::{Deserialize, Serialize};

use super::StoredCategories;

/// The version of the split mapping. Any change to [`SPLIT_MAPPING`], a
/// reassignment or a new word included, bumps this constant and reruns
/// the split evals (the rulebook's AI rule; the eval pins land with
/// their slice). It rides beside the table the way the prompt versions
/// ride beside their builders.
pub const SPLIT_MAPPING_VERSION: &str = "1";

/// One side of the split inbox. Wire words are snake_case; the message
/// list's three-way toggle filters locally against them. A message
/// mapped to neither side, and a message with no stored category at
/// all, appear in All only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitSide {
    /// Mail that needs the user's own reply or decision: the stored
    /// category primary.
    Actioned,
    /// Mail to read at leisure: newsletter, notification, receipt, and
    /// cold.
    Fyi,
}

impl SplitSide {
    /// The snake_case wire word.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Actioned => "actioned",
            Self::Fyi => "fyi",
        }
    }
}

impl std::fmt::Display for SplitSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The pinned mapping from a stored category word to its split side:
/// primary rides Actioned; newsletter, notification, receipt, and cold
/// ride Fyi; every other word is excluded (All only). The table is the
/// decision itself, versioned by [`SPLIT_MAPPING_VERSION`] and pinned
/// by the tests.
pub const SPLIT_MAPPING: [(&str, SplitSide); 5] = [
    ("primary", SplitSide::Actioned),
    ("newsletter", SplitSide::Fyi),
    ("notification", SplitSide::Fyi),
    ("receipt", SplitSide::Fyi),
    ("cold", SplitSide::Fyi),
];

/// The side one stored category word maps to: the table's answer for
/// the word, or `None` when the word is excluded, so its message
/// appears in All only.
#[must_use]
pub fn side_for(word: &str) -> Option<SplitSide> {
    SPLIT_MAPPING
        .iter()
        .find(|(mapped, _)| *mapped == word)
        .map(|(_, side)| *side)
}

/// The account's split map over its stored categorisation: every
/// mapped message as its key beside its side, in key order so the
/// answer is deterministic whatever order the caller collected the rows
/// in. A stored word outside the table is excluded here; a message with
/// no stored category never entered the map the caller built.
#[must_use]
pub fn split_map(stored_categories: &StoredCategories) -> Vec<(ProviderKey, SplitSide)> {
    stored_categories
        .iter()
        .filter_map(|(key, word)| side_for(word).map(|side| (key.clone(), side)))
        .collect()
}
