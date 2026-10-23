// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: no source file exceeds 500 lines. Markdown documents are
//! exempt: prose length is judged by review, not by a line budget. Generated
//! lockfiles are exempt: their length belongs to the package manager.

pub(crate) const MAX_LINES: usize = 500;

/// The upstream-ported trees the cap deliberately does not measure: code kept
/// byte-diffable with its source so port diffs stay reviewable (the crypto
/// workspace vendored from kylins, spec D3). Exempt only while unrewritten;
/// a slice that rewrites these files natively must shrink the list to match.
const UPSTREAM_EXEMPT_PREFIXES: &[&str] = &[
    "crates/astra-crypto-core/",
    "crates/astra-crypto-openpgp/",
    "crates/astra-crypto-smime/",
    // The SP4b store is likewise a kylins port (sqlx -> rusqlite being the
    // only sanctioned rewrite), kept diffable against its source.
    "crates/astra-crypto-store/",
];

/// True for package manager lockfiles, which are generated output.
fn is_lockfile(p: &std::path::Path) -> bool {
    matches!(
        p.file_name().and_then(|n| n.to_str()),
        Some("package-lock.json" | "pnpm-lock.yaml")
    )
}

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
        if is_lockfile(&f) {
            continue;
        }
        if UPSTREAM_EXEMPT_PREFIXES
            .iter()
            .any(|prefix| r.starts_with(prefix))
        {
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

    #[test]
    fn lockfiles_are_generated_output() {
        let long = "line\n".repeat(super::MAX_LINES + 1);
        let root = fixture(&[("client/web/package-lock.json", &long)]);
        assert!(super::check(&root).is_ok());
    }

    #[test]
    fn upstream_ported_trees_are_exempt() {
        let long = "line\n".repeat(super::MAX_LINES + 1);
        let root = fixture(&[
            ("crates/astra-crypto-core/src/lib.rs", &long),
            ("crates/astra-crypto-smime/src/lib.rs", &long),
        ]);
        assert!(super::check(&root).is_ok());
    }
}
