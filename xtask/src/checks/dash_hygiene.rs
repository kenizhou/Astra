// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: no dash used as punctuation. Em and en dashes are banned in
//! every tracked text file; a spaced hyphen is additionally banned in
//! Markdown prose.

const EM_DASH: char = '\u{2014}';
const EN_DASH: char = '\u{2013}';

pub fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for f in super::walk_files(root, &["rs", "ts", "tsx", "md", "toml", "yml", "yaml", "json", "css", "sh"]) {
        let r = super::rel(root, &f);
        if r == "LICENSE" || r.starts_with("docs/zh/") || super::is_zh_mirror(&f) {
            continue;
        }
        let is_markdown = r.ends_with(".md");
        let mut in_fence = false;
        for (idx, line) in super::read_lossy(&f).lines().enumerate() {
            if is_markdown && line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if line.contains(EM_DASH) || line.contains(EN_DASH) {
                bad.push(format!("{r}:{} contains a dash character", idx + 1));
            }
            if is_markdown && !in_fence && spaced_hyphen_as_punctuation(line) {
                bad.push(format!("{r}:{} uses a spaced hyphen as punctuation", idx + 1));
            }
        }
    }
    if bad.is_empty() { Ok(()) } else { Err(bad) }
}

/// True when the line uses a spaced hyphen as punctuation. A leading list
/// marker (`- ` or `* ` after indentation) is not punctuation and is
/// stripped before scanning.
fn spaced_hyphen_as_punctuation(line: &str) -> bool {
    let trimmed = line.trim_start();
    let body = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .unwrap_or(trimmed);
    body.contains(" - ")
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;

    #[test]
    fn passes_on_clean_prose() {
        let root = fixture(&[("a.md", "The colour of the licence dialog.\n")]);
        assert!(super::check(&root).is_ok());
    }

    #[test]
    fn fails_on_em_dash() {
        let root = fixture(&[("a.md", "one\u{2014}two\n")]);
        assert!(super::check(&root).is_err());
    }

    #[test]
    fn fails_on_spaced_hyphen_in_markdown_only() {
        let ok_code = fixture(&[("a.rs", "let s = \"x - y\";\n")]);
        let bad_doc = fixture(&[("a.md", "hot - dog\n")]);
        assert!(super::check(&ok_code).is_ok());
        assert!(super::check(&bad_doc).is_err());
    }

    #[test]
    fn list_markers_are_not_punctuation() {
        let root = fixture(&[("a.md", "- item\n  - nested item\n* star item\n")]);
        assert!(super::check(&root).is_ok());
        let trailing = fixture(&[("a.md", "- item with - a mid dash\n")]);
        assert!(super::check(&trailing).is_err());
    }

    #[test]
    fn fenced_code_is_exempt_from_the_prose_rule() {
        let root = fixture(&[("a.md", "prose\n\n```rust\nlet s = \"x - y\";\n```\n\nmore prose\n")]);
        assert!(super::check(&root).is_ok());
    }
}
