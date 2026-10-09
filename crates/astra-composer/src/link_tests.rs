// SPDX-License-Identifier: GPL-3.0-only
//! Links: which targets survive the scheme allowlist, and what reaches the
//! outgoing HTML and plain text.

use crate::test_support::html_doc;
use crate::{
    has_link_scheme, render, Block, ComposerDocument, InlineContent, LinkUrl, Paragraph, TextRun,
};

#[test]
fn accepts_web_and_mail_links() {
    for good in [
        "https://example.com",
        "http://example.com/a?b=c&d=e#f",
        "HTTPS://Example.com",
        "mailto:someone@example.com",
        "  https://example.com  ",
    ] {
        assert!(LinkUrl::new(good).is_some(), "expected {good:?} to be kept");
    }
}

#[test]
fn refuses_every_other_scheme_and_malformed_value() {
    for bad in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "data:text/html,<script>x</script>",
        "file:///etc/passwd",
        "myapp://open",
        "example.com",
        "/relative",
        "https://",
        "mailto:",
        "https://exa mple.com",
        "https://example.com/\nBcc: x",
        "",
    ] {
        assert!(
            LinkUrl::new(bad).is_none(),
            "expected {bad:?} to be refused"
        );
    }
}

#[test]
fn refuses_an_oversized_link() {
    let long = format!("https://example.com/{}", "a".repeat(2048));
    assert!(LinkUrl::new(&long).is_none());
}

#[test]
fn the_scheme_check_is_case_insensitive_and_byte_safe() {
    assert!(has_link_scheme("MAILTO:a@b.example"));
    assert!(!has_link_scheme("ht tp://x"));
    assert!(!has_link_scheme("é:x"));
    assert!(!has_link_scheme("no scheme"));
}

fn run(text: &str, bold: bool, link: Option<&str>) -> InlineContent {
    InlineContent::Text(TextRun {
        text: text.to_owned(),
        bold,
        italic: false,
        underline: false,
        font_size: None,
        color: None,
        highlight: None,
        link: link.map(|url| LinkUrl::new(url).expect("valid link")),
    })
}

fn document(content: Vec<InlineContent>) -> ComposerDocument {
    ComposerDocument {
        blocks: vec![Block::Paragraph(Paragraph { content })],
        attachments: Vec::new(),
    }
}

#[test]
fn a_linked_run_becomes_an_anchor_and_names_its_target_in_the_plain_text() {
    let output = render(&document(vec![
        run("See ", false, None),
        run("the agenda", false, Some("https://example.com/a?b=1&c=2")),
        run(".", false, None),
    ]))
    .expect("renders");
    assert_eq!(
        output.html,
        html_doc("<p>See <a href=\"https://example.com/a?b=1&amp;c=2\">the agenda</a>.</p>")
    );
    assert_eq!(
        output.plain_text,
        "See the agenda <https://example.com/a?b=1&c=2>."
    );
}

#[test]
fn runs_sharing_a_target_are_one_link() {
    // A link whose words carry different marks is still one link, in both parts.
    let output = render(&document(vec![
        run("the ", false, Some("https://example.com")),
        run("new", true, Some("https://example.com")),
        run(" plan", false, Some("https://example.com")),
    ]))
    .expect("renders");
    assert_eq!(
        output.html,
        html_doc("<p><a href=\"https://example.com\">the <strong>new</strong> plan</a></p>")
    );
    assert_eq!(output.plain_text, "the new plan <https://example.com>");
}

#[test]
fn adjacent_links_to_different_targets_stay_apart() {
    let output = render(&document(vec![
        run("one", false, Some("https://one.example")),
        run("two", false, Some("https://two.example")),
    ]))
    .expect("renders");
    assert_eq!(
        output.html,
        html_doc(
            "<p><a href=\"https://one.example\">one</a><a href=\"https://two.example\">two</a></p>"
        )
    );
}

#[test]
fn an_address_shown_as_itself_is_written_once_in_the_plain_text() {
    let output = render(&document(vec![
        run("https://example.com", false, Some("https://example.com")),
        run(" or ", false, None),
        run("a@example.com", false, Some("mailto:a@example.com")),
    ]))
    .expect("renders");
    assert_eq!(output.plain_text, "https://example.com or a@example.com");
}

#[test]
fn a_hostile_target_in_the_document_sends_the_words_without_a_link() {
    // The editor is not the only thing that can write a document, so the target is checked as it
    // is read: a `javascript:` link, or one trying to break out of the attribute, arrives as text.
    let json = r#"{"blocks":[{"Paragraph":{"content":[
        {"Text":{"text":"click","bold":false,"italic":false,"underline":false,"link":"javascript:alert(1)"}},
        {"Text":{"text":"me","bold":false,"italic":false,"underline":false,"link":"https://x\" onclick=\"y"}}
    ]}}]}"#;
    let document: ComposerDocument = serde_json::from_str(json).expect("parses");
    let output = render(&document).expect("renders");
    assert_eq!(output.html, html_doc("<p>clickme</p>"));
    assert_eq!(output.plain_text, "clickme");
}

#[test]
fn a_target_holding_quotes_is_escaped_in_the_attribute() {
    let output = render(&document(vec![run(
        "x",
        false,
        Some("https://example.com/'q'"),
    )]))
    .expect("renders");
    assert_eq!(
        output.html,
        html_doc("<p><a href=\"https://example.com/&#39;q&#39;\">x</a></p>")
    );
}
