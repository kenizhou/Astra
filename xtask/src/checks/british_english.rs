// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: prose uses British spelling. Code identifiers are exempt by
//! the word boundary scan (casing inside identifiers does not match).

/// American spellings banned in prose. Extend as they are found; keep the
/// list alphabetical. `dialog`, `artifact`, and `catalog` stay American by
/// the mail-calendar rulebook.
pub(crate) const AMERICANISMS: &[&str] = &[
    "behavior",
    "behavioral",
    "canceled",
    "canceling",
    "color",
    "colored",
    "colors",
    "favorite",
    "favorites",
    "fulfill",
    "fulfillment",
    "labeled",
    "labeling",
    "maximize",
    "minimized",
    "organize",
    "organized",
];

pub(crate) fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for f in super::walk_files(root, &["md"]) {
        let r = super::rel(root, &f);
        if r.starts_with("docs/zh/") || super::is_zh_mirror(&f) {
            continue;
        }
        let mut in_fence = false;
        let mut prose = String::new();
        for line in super::read_lossy(&f).lines() {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if !in_fence {
                prose.push_str(line);
                prose.push('\n');
            }
        }
        for word in find_americanisms(&prose) {
            bad.push(format!("{r}: American spelling \"{word}\""));
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad)
    }
}

/// Case-insensitive whole-word scan returning the matched source words.
pub(crate) fn find_americanisms(text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for word in text.split(|c: char| !(c.is_alphanumeric())) {
        let lower = word.to_lowercase();
        if AMERICANISMS.contains(&lower.as_str()) {
            hits.push(word.to_string());
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;

    #[test]
    fn passes_on_british_prose() {
        let root = fixture(&[("a.md", "The colour of the licence.\n")]);
        assert!(super::check(&root).is_ok());
    }

    #[test]
    fn fails_on_american_spelling() {
        let root = fixture(&[("a.md", "the color of the behavior\n")]);
        let errs = super::check(&root).unwrap_err();
        assert_eq!(errs.len(), 2);
    }

    #[test]
    fn identifiers_are_not_flagged() {
        assert!(super::find_americanisms("CategorizeSender and summarize_thread_id").is_empty());
        assert_eq!(super::find_americanisms("the color").len(), 1);
    }

    #[test]
    fn fenced_code_is_exempt() {
        let root = fixture(&[(
            "a.md",
            "prose\n\n```rust\nlet s = \"the color\";\n```\n\nmore prose\n",
        )]);
        assert!(super::check(&root).is_ok());
    }
}
