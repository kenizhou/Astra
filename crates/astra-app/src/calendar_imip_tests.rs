// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the iMIP leg's pure half: content-line folding at the 75-octet
//! limit a strict parser is entitled to enforce.

use super::line;

/// Unfolds folded output the way RFC 5545 §3.1 says a parser does: a line
/// break followed by a space is removed, joining the pieces. The single
/// newline that ends the last physical line stays, as it does on the wire.
fn unfolded(out: &str) -> String {
    out.replace("\r\n ", "")
}

#[test]
fn a_short_line_crosses_untouched() {
    let mut out = String::new();
    line(&mut out, "UID:meeting-1@test.local");
    assert_eq!(out, "UID:meeting-1@test.local\r\n");
}

#[test]
fn a_long_line_folds_at_the_limit_and_continues_with_one_space() {
    let content = format!("UID:{}@test.local", "a".repeat(80));
    let mut out = String::new();
    line(&mut out, &content);
    let lines: Vec<&str> = out.trim_end_matches("\r\n").split("\r\n").collect();
    assert!(lines.len() > 1, "the line folded: {out:?}");
    assert_eq!(
        lines[0].len(),
        75,
        "the first physical line spends exactly the budget"
    );
    for continuation in &lines[1..] {
        assert!(
            continuation.starts_with(' ')
                && continuation.len() <= 75
                && continuation.as_bytes()[1].is_ascii_graphic(),
            "every continuation opens with one space and stays inside the \
             limit: {continuation:?}"
        );
    }
    assert_eq!(
        unfolded(&out),
        format!("{content}\r\n"),
        "folding is lossless"
    );
}

#[test]
fn folding_never_cuts_a_multibyte_character_in_half() {
    let content = format!("SUMMARY:{}", "ü".repeat(60));
    let mut out = String::new();
    line(&mut out, &content);
    // Reassembling the pieces must give back a well-formed string: the fold
    // point backs up to a character boundary rather than splitting a scalar.
    assert_eq!(unfolded(&out), format!("{content}\r\n"));
    assert!(out.ends_with("\r\n"));
}
