// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: every tracked path is ASCII outside docs/zh.

pub(crate) fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for f in super::walk_files(root, &[]) {
        let r = super::rel(root, &f);
        if r.starts_with("docs/zh/") {
            continue;
        }
        if !r.is_ascii() {
            bad.push(format!("non-ASCII path: {r}"));
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
    fn passes_for_ascii_paths() {
        let root = fixture(&[("src/main.rs", "fn main() {}\n")]);
        assert!(super::check(&root).is_ok());
    }

    #[test]
    fn fails_for_a_non_ascii_path() {
        let root = fixture(&[("docs/\u{5e7d}.md", "body\n")]);
        assert!(super::check(&root).is_err());
    }

    #[test]
    fn ignores_docs_zh() {
        let root = fixture(&[("docs/zh/\u{5e7d}.zh-CN.md", "body\n")]);
        assert!(super::check(&root).is_ok());
    }
}
