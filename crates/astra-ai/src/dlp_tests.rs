// SPDX-License-Identifier: GPL-3.0-only
//! DLP tests: the redactor's pinned patterns (emails, phone shapes, long
//! digit runs, edge adjacency, span merging, idempotence), the policy parse
//! (absent is allow, malformed is loud), and the gate's three outcomes.

use crate::dlp::{apply, redact, DlpError, DlpOutcome, DlpPolicy};

#[test]
fn email_addresses_are_masked_with_length_preserved() {
    let masked = redact("Write to ada@example.test soon.");
    assert_eq!(
        masked,
        format!("Write to {} soon.", "*".repeat(16)),
        "every character of the address is one mask"
    );
    assert_eq!(
        masked.chars().count(),
        "Write to ada@example.test soon.".chars().count(),
        "redaction preserves lengths"
    );
    assert_eq!(
        redact("also ADA@EXAMPLE.TEST here"),
        format!("also {} here", "*".repeat(16))
    );
    assert_eq!(
        redact("first_last@corp.example now"),
        format!("{} now", "*".repeat(23)),
        "underscore and multi-dot domains ride the same class"
    );
}

#[test]
fn address_shaped_words_without_a_domain_stay() {
    // No dot in the domain, no TLD: not an address, not masked.
    assert_eq!(redact("see bad@localhost ok"), "see bad@localhost ok");
    assert_eq!(redact("nope a@x.t here"), "nope a@x.t here");
}

#[test]
fn sentence_final_addresses_are_masked_and_the_period_stays() {
    // The sentence's closing period is not the address's: the mask ends at
    // the TLD and the sentence keeps its punctuation.
    assert_eq!(
        redact("Contact ada@example.test."),
        format!("Contact {}.", "*".repeat(16)),
        "a sentence-final address masks while the period stays",
    );
    assert_eq!(
        redact("Write ada@example.co.uk. Thanks."),
        format!("Write {}. Thanks.", "*".repeat(17)),
        "a multi-dot domain at sentence end masks the same way",
    );
}

#[test]
fn idn_domains_are_masked() {
    // A raw UTF-8 domain rides the domain class: the address masks whole,
    // one mask character per address character.
    let address = "ada@münchen-umlaut.de";
    let masked = redact("Mail ada@münchen-umlaut.de now");
    assert_eq!(
        masked,
        format!("Mail {} now", "*".repeat(address.chars().count())),
        "the whole address masks, umlaut included",
    );
    assert_eq!(
        masked.chars().count(),
        "Mail ada@münchen-umlaut.de now".chars().count(),
        "redaction preserves character lengths over multibyte spans",
    );
}

#[test]
fn phone_shaped_runs_are_masked_with_their_separators() {
    assert_eq!(
        redact("Call +1 (555) 123-4567 now."),
        format!("Call {} now.", "*".repeat(17)),
        "brackets, spaces, and dashes ride the mask"
    );
    assert_eq!(redact("or 020 7946 0958"), format!("or {}", "*".repeat(13)));
    assert_eq!(
        redact("alt 555.123.4567 here"),
        format!("alt {} here", "*".repeat(12)),
        "dotted groups read as phones too"
    );
    // A date rides the same shape as a phone: over-masking is the safe
    // direction, pinned.
    assert_eq!(
        redact("sent 2026-10-10 late"),
        format!("sent {} late", "*".repeat(10))
    );
}

#[test]
fn long_digit_runs_are_masked_and_short_ones_stay() {
    assert_eq!(
        redact("card 4111111111111111 end"),
        format!("card {} end", "*".repeat(16)),
    );
    assert_eq!(
        redact("ref 123456789012"),
        format!("ref {}", "*".repeat(12)),
        "twelve is in"
    );
    assert_eq!(
        redact("qty 12345678901 stays"),
        "qty 12345678901 stays",
        "eleven is out"
    );
    assert_eq!(redact("small 3 4 and 5 stay"), "small 3 4 and 5 stay");
    assert_eq!(
        redact("price 1,234.56 stays"),
        "price 1,234.56 stays",
        "commas do not join"
    );
}

#[test]
fn letter_flanked_digit_runs_stay() {
    assert_eq!(
        redact("ref abc123456789012def ok"),
        "ref abc123456789012def ok",
        "a run inside a word is part of the token, not a number"
    );
    assert_eq!(
        redact("id=123456789012;"),
        format!("id={};", "*".repeat(12)),
        "punctuation flanks do not protect"
    );
}

#[test]
fn overlapping_spans_merge_into_one_mask() {
    // The local part is itself a long digit run: one mask, the union.
    assert_eq!(
        redact("mail 1234567890123@corp.test ok"),
        format!("mail {} ok", "*".repeat(23)),
    );
}

#[test]
fn redaction_is_idempotent_over_masked_content() {
    let once = redact("ada@example.test paid 4111111111111111 on +1 (555) 123-4567");
    assert_eq!(
        redact(&once),
        once,
        "a masked span carries nothing left to mask"
    );
    assert!(!once.contains('@'));
    assert!(!once.contains('4'));
}

#[test]
fn the_policy_defaults_to_allow_and_parses_its_words() {
    assert_eq!(DlpPolicy::from_value(None), Ok(DlpPolicy::Allow));
    assert_eq!(DlpPolicy::from_value(Some("")), Ok(DlpPolicy::Allow));
    assert_eq!(DlpPolicy::from_value(Some("   ")), Ok(DlpPolicy::Allow));
    assert_eq!(DlpPolicy::from_value(Some("allow")), Ok(DlpPolicy::Allow));
    assert_eq!(
        DlpPolicy::from_value(Some(" REDACT ")),
        Ok(DlpPolicy::Redact)
    );
    assert_eq!(DlpPolicy::from_value(Some("Block")), Ok(DlpPolicy::Block));
}

#[test]
fn a_malformed_policy_is_rejected_loudly() {
    let err = DlpPolicy::from_value(Some("encrypt")).expect_err("an unknown word refuses");
    assert!(matches!(err, DlpError::Malformed { .. }), "{err}");
    let message = err.to_string();
    assert!(message.contains("ASTRA_DLP"), "{message}");
    assert!(message.contains("encrypt"), "{message}");
    assert!(message.contains("allow"), "{message}");
    assert!(message.contains("block"), "{message}");
}

#[test]
fn the_gate_passes_redacts_and_blocks() {
    assert_eq!(
        apply(DlpPolicy::Allow, "ada@example.test"),
        DlpOutcome::Allowed {
            content: "ada@example.test".to_owned(),
        },
    );
    assert_eq!(
        apply(DlpPolicy::Redact, "ada@example.test"),
        DlpOutcome::Allowed {
            content: "*".repeat(16),
        },
        "redact transforms then allows"
    );
    assert_eq!(
        apply(DlpPolicy::Redact, "plain words only"),
        DlpOutcome::Allowed {
            content: "plain words only".to_owned(),
        },
        "clean content rides a redacting policy unchanged"
    );
    assert_eq!(
        apply(DlpPolicy::Block, "anything at all"),
        DlpOutcome::Blocked
    );
}
