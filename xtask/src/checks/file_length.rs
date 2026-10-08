// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: no source file exceeds 500 lines. Markdown documents are
//! exempt: prose length is judged by review, not by a line budget.

pub(crate) const MAX_LINES: usize = 500;

pub(crate) fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for f in super::walk_files(
        root,
        &[
            "rs", "ts", "tsx", "css", "json", "yml", "yaml", "toml", "sh",
        ],
    ) {
        let r = super::rel(root, &f);
        if r == "LICENSE" || r.starts_with("docs/zh/") || super::is_zh_mirror(&f) {
            continue;
        }
        let lines = super::read_lossy(&f).lines().count();
        if lines > MAX_LINES {
            bad.push(format!("{r}: {lines} lines (max {MAX_LINES})"));
        }
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;

    #[test]
    fn passes_within_the_cap() {
        let root = fixture(&[("a.rs", "fn a() {}\n")]);
        assert!(super::check(&root).is_ok());
    }

    #[test]
    fn fails_over_the_cap() {
        let long = "line\n".repeat(super::MAX_LINES + 1);
        let root = fixture(&[("a.rs", &long)]);
        assert!(super::check(&root).is_err());
    }

    #[test]
    fn markdown_is_exempt() {
        let long = "line\n".repeat(super::MAX_LINES + 1);
        let root = fixture(&[("a.md", &long), ("docs/zh/a.zh-CN.md", &long)]);
        assert!(super::check(&root).is_ok());
    }
}
