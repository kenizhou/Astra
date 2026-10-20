// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the signature rules: the per-account assignment the preferences
//! persist and the use each slot answers, the block a compose open seeds and
//! where it lands, and the swap a From change makes of a seeded signature the
//! person has not touched.

use std::path::PathBuf;

use astra_composer::{
    Block, InlineContent, Paragraph, Quote, QuoteAttribution, QuoteStyle, Signature, TextRun,
};
use engine_api::AccountId;

use crate::signatures::{seed_signature, swap_seeded, AccountSignatures};
use crate::{load_preferences, save_preferences, Preferences};

/// A fresh scratch directory of its own, so one test's writes cannot reach
/// another's.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("astra-signatures-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn account(id: &str) -> AccountId {
    AccountId::try_from(id).unwrap()
}

/// One signature literal carrying the given words in both spellings.
fn signature(words: &str) -> Signature {
    Signature {
        body_html: format!("<p>{words}</p>"),
        body_plain: words.to_owned(),
    }
}

/// One plain paragraph of unmarked words.
fn paragraph(words: &str) -> Block {
    Block::Paragraph(Paragraph {
        content: vec![InlineContent::Text(TextRun {
            text: words.to_owned(),
            bold: false,
            italic: false,
            underline: false,
            font_size: None,
            color: None,
            highlight: None,
            link: None,
        })],
    })
}

/// One quoted original, the block a reply opens beneath.
fn quote() -> Block {
    Block::Quote(Quote {
        style: QuoteStyle::Indented,
        attribution: QuoteAttribution {
            line: "Ada wrote:".to_owned(),
            headers: Vec::new(),
        },
        body_html: "<div>The numbers are in.</div>".to_owned(),
        body_plain: "The numbers are in.".to_owned(),
    })
}

#[test]
fn an_account_without_an_assignment_seeds_no_signature() {
    let mut prefs = Preferences::default();
    prefs.signatures.insert(
        account("half"),
        AccountSignatures {
            new_mail: Some(signature("Ada")),
            reply: None,
        },
    );

    assert_eq!(prefs.seeded_signature(&account("work"), false), None);
    assert_eq!(
        prefs.seeded_signature(&account("half"), true),
        None,
        "an empty slot seeds nothing, the account's other slot notwithstanding"
    );
}

#[test]
fn the_assignment_answers_by_use_new_mail_and_reply_independently() {
    let mut prefs = Preferences::default();
    prefs.signatures.insert(
        account("work"),
        AccountSignatures {
            new_mail: Some(signature("Ada Lovelace")),
            reply: Some(signature("A. Lovelace")),
        },
    );

    assert_eq!(
        prefs.seeded_signature(&account("work"), false),
        Some(signature("Ada Lovelace"))
    );
    assert_eq!(
        prefs.seeded_signature(&account("work"), true),
        Some(signature("A. Lovelace"))
    );
}

#[test]
fn recording_a_slot_keeps_the_row_while_any_slot_stands() {
    let mut prefs = Preferences::default();
    let work = account("work");

    prefs.set_seeded_signature(&work, false, Some(signature("Ada")));
    prefs.set_seeded_signature(&work, true, Some(signature("A.")));
    assert_eq!(prefs.signatures.len(), 1);

    prefs.set_seeded_signature(&work, true, None);
    assert_eq!(
        prefs.seeded_signature(&work, false),
        Some(signature("Ada")),
        "the standing new-mail slot keeps the row"
    );

    prefs.set_seeded_signature(&work, false, None);
    assert!(
        prefs.signatures.is_empty(),
        "the last empty slot takes the row with it"
    );
}

#[test]
fn the_default_send_account_falls_back_to_the_first_standing_account() {
    let mut prefs = Preferences::default();
    let accounts = [account("work"), account("home")];

    assert_eq!(
        prefs.send_account(&accounts),
        Some(account("work")),
        "unset, the first standing account answers"
    );

    prefs.set_send_account(Some(account("home")));
    assert_eq!(prefs.send_account(&accounts), Some(account("home")));

    let stale = [account("work")];
    assert_eq!(
        prefs.send_account(&stale),
        Some(account("work")),
        "a preference naming no standing account answers the first"
    );

    prefs.set_send_account(None);
    assert_eq!(prefs.send_account(&accounts), Some(account("work")));
    assert_eq!(prefs.send_account(&[]), None, "no account stands at all");
}

#[test]
fn the_assignment_and_the_default_send_account_round_trip_through_the_file() {
    let dir = scratch("round-trip");
    let path = dir.join("preferences.toml");
    let mut prefs = Preferences::default();
    prefs.set_send_account(Some(account("home")));
    prefs.set_seeded_signature(&account("work"), false, Some(signature("Ada")));
    prefs.set_seeded_signature(&account("work"), true, Some(signature("A.")));

    save_preferences(&path, &prefs).unwrap();
    let back = load_preferences(&path);

    assert_eq!(
        back.send_account(&[account("work"), account("home")]),
        Some(account("home"))
    );
    assert_eq!(
        back.seeded_signature(&account("work"), true),
        Some(signature("A."))
    );
}

#[test]
fn seeding_places_the_block_at_the_top_of_a_new_mail() {
    let blocks = vec![paragraph("Hello"), paragraph("Bye")];

    assert_eq!(
        seed_signature(blocks, &signature("Ada")),
        vec![
            Block::Signature(signature("Ada")),
            paragraph("Hello"),
            paragraph("Bye"),
        ],
        "a new mail's signature stands above the words"
    );
}

#[test]
fn seeding_places_the_block_directly_under_the_quote_of_a_reply() {
    let blocks = vec![paragraph("Thanks"), quote(), paragraph("Ps")];

    assert_eq!(
        seed_signature(blocks, &signature("Ada")),
        vec![
            paragraph("Thanks"),
            quote(),
            Block::Signature(signature("Ada")),
            paragraph("Ps"),
        ],
        "the signature lands directly beneath the quoted original"
    );
}

#[test]
fn a_document_already_bearing_a_signature_seeds_nothing() {
    let blocks = vec![Block::Signature(signature("Ada"))];

    assert_eq!(
        seed_signature(blocks, &signature("Ada")),
        vec![Block::Signature(signature("Ada"))],
        "no second block, and the standing one stays as it is"
    );
}

#[test]
fn a_from_change_swaps_an_untouched_seeded_signature() {
    let blocks = vec![paragraph("Thanks"), Block::Signature(signature("Ada"))];

    assert_eq!(
        swap_seeded(blocks, &signature("Ada"), Some(&signature("Grace"))),
        vec![paragraph("Thanks"), Block::Signature(signature("Grace"))],
    );
}

#[test]
fn a_from_change_drops_an_untouched_signature_when_the_account_seeds_none() {
    let blocks = vec![Block::Signature(signature("Ada")), paragraph("Thanks")];

    assert_eq!(
        swap_seeded(blocks, &signature("Ada"), None),
        vec![paragraph("Thanks")],
    );
}

#[test]
fn an_edited_signature_stays_through_the_swap() {
    let edited = Signature {
        body_html: "<p>Ada (edited)</p>".to_owned(),
        body_plain: "Ada (edited)".to_owned(),
    };
    let blocks = vec![Block::Signature(edited.clone())];

    assert_eq!(
        swap_seeded(blocks, &signature("Ada"), Some(&signature("Grace"))),
        vec![Block::Signature(edited)],
        "the person's words are theirs, not the picker's"
    );
}

#[test]
fn removing_an_account_drops_its_assignment_and_a_stale_default() {
    let mut prefs = Preferences::default();
    prefs.set_send_account(Some(account("work")));
    prefs.set_seeded_signature(&account("work"), false, Some(signature("Ada")));
    prefs.set_seeded_signature(&account("home"), true, Some(signature("H.")));

    assert!(prefs.remove_account("work"));

    assert_eq!(prefs.seeded_signature(&account("work"), false), None);
    assert_eq!(prefs.default_send_account, None);
    assert_eq!(
        prefs.seeded_signature(&account("home"), true),
        Some(signature("H.")),
        "another account's assignment stands"
    );
}
