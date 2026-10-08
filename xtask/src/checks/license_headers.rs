// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: Rust and TypeScript sources carry the SPDX header.

pub(crate) const HEADER: &str = "// SPDX-License-Identifier: GPL-3.0-only";

pub(crate) fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for f in super::walk_files(root, &["rs", "ts", "tsx"]) {
        let r = super::rel(root, &f);
        let contents = super::read_lossy(&f);
        let first = contents.lines().next().unwrap_or_default();
        if !first.contains(HEADER) {
            bad.push(format!("{r}: missing SPDX licence header"));
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
    fn passes_with_header() {
        let root = fixture(&[(
            "a.rs",
            "// SPDX-License-Identifier: GPL-3.0-only\nfn a() {}\n",
        )]);
        assert!(super::check(&root).is_ok());
    }

    #[test]
    fn fails_without_header() {
        let root = fixture(&[("a.ts", "export const a = 1;\n")]);
        assert!(super::check(&root).is_err());
    }

    #[test]
    fn blank_first_line_does_not_excuse_a_missing_header() {
        let root = fixture(&[("a.rs", "\nfn a() {}\n")]);
        assert!(super::check(&root).is_err());
    }
}
