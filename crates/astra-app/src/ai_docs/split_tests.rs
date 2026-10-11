// SPDX-License-Identifier: GPL-3.0-only
//! Split mapping tests: the pinned mapping table as data (primary rides
//! Actioned, the four bulk words ride Fyi), every closed-set category
//! word carrying its pinned side, an unknown word excluded so its
//! message appears in All only, the key-ordered answer, and the wire
//! words.

use engine_api::ProviderKey;

use super::split::{side_for, split_map, SplitSide, SPLIT_MAPPING, SPLIT_MAPPING_VERSION};
use super::StoredCategories;
use crate::ai_rules::Category;

/// The stored categorisation the split rides over, straight from the
/// caller's rows: message keys beside their stored category words.
fn stored(pairs: &[(&str, &str)]) -> StoredCategories {
    pairs
        .iter()
        .map(|(key, category)| (ProviderKey::new(*key).unwrap(), (*category).to_owned()))
        .collect()
}

#[test]
fn the_mapping_pin_is_the_pinned_table() {
    assert_eq!(SPLIT_MAPPING_VERSION, "1");
    assert_eq!(
        SPLIT_MAPPING,
        [
            ("primary", SplitSide::Actioned),
            ("newsletter", SplitSide::Fyi),
            ("notification", SplitSide::Fyi),
            ("receipt", SplitSide::Fyi),
            ("cold", SplitSide::Fyi),
        ],
        "the mapping is data: primary rides Actioned, the four bulk words ride Fyi",
    );
}

#[test]
fn every_closed_set_word_carries_its_pinned_side() {
    for category in Category::ALL {
        let side = side_for(category.as_str())
            .unwrap_or_else(|| panic!("the closed set's word {category} has no pinned side"));
        let expected = if category == Category::Primary {
            SplitSide::Actioned
        } else {
            SplitSide::Fyi
        };
        assert_eq!(side, expected, "the word {category} keeps its pin");
    }
}

#[test]
fn an_unknown_category_word_is_excluded() {
    assert_eq!(
        side_for("rumour"),
        None,
        "a word outside the table maps to nothing",
    );
    let stored_categories = stored(&[("m1", "primary"), ("m2", "rumour"), ("m3", "cold")]);
    assert_eq!(
        split_map(&stored_categories),
        vec![
            (ProviderKey::new("m1").unwrap(), SplitSide::Actioned),
            (ProviderKey::new("m3").unwrap(), SplitSide::Fyi),
        ],
        "the unknown word's message appears in All only",
    );
}

#[test]
fn split_map_answers_in_key_order() {
    let stored_categories = stored(&[("m9", "cold"), ("m2", "primary"), ("m7", "newsletter")]);
    let split = split_map(&stored_categories);
    let keys: Vec<&str> = split.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        keys,
        vec!["m2", "m7", "m9"],
        "the map answers in key order, whatever the insert order",
    );
}

#[test]
fn split_side_wire_words_are_snake_case() {
    assert_eq!(SplitSide::Actioned.as_str(), "actioned");
    assert_eq!(SplitSide::Fyi.as_str(), "fyi");
    assert_eq!(
        serde_json::to_string(&SplitSide::Actioned).unwrap(),
        "\"actioned\"",
    );
    assert_eq!(serde_json::to_string(&SplitSide::Fyi).unwrap(), "\"fyi\"");
}
