// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: vendored font files stay inside the UI1 D2 budget (400KB per
//! file), so the offline bundle never grows silently.

use std::fs;

/// The per-file budget in bytes (the UI1 D2 subset budget).
pub(crate) const MAX_FONT_BYTES: u64 = 400 * 1024;

pub(crate) fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for f in super::walk_files(root, &["woff2"]) {
        let bytes = fs::metadata(&f).map_or(0, |m| m.len());
        if bytes > MAX_FONT_BYTES {
            bad.push(format!(
                "{}: {bytes} bytes (max {MAX_FONT_BYTES})",
                super::rel(root, &f)
            ));
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
    fn passes_within_the_budget() {
        let root = fixture(&[("a.woff2", "small")]);
        assert!(super::check(&root).is_ok());
    }

    #[test]
    fn fails_over_the_budget() {
        let budget = usize::try_from(super::MAX_FONT_BYTES).expect("budget fits usize");
        let big = "x".repeat(budget + 1);
        let root = fixture(&[("a.woff2", &big)]);
        assert!(super::check(&root).is_err());
    }
}
