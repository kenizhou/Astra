// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: every in-scope English document has its Chinese review copy,
//! and no review copy is orphaned. Scope: AGENTS.md, docs/*.md, and
//! docs/superpowers/**/*.md. Excluded: docs/changelog/** and docs/zh/**.

use std::fs;
use std::path::{Path, PathBuf};

pub fn check(root: &Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    let docs = english_docs(root);
    for en in &docs {
        let mirror = mirror_of(root, en);
        if !mirror.is_file() {
            bad.push(format!("missing Chinese review copy for {}", super::rel(root, en)));
        }
    }
    let mut zh_files: Vec<PathBuf> = super::walk_files(&root.join("docs/zh"), &["md"]);
    zh_files.retain(|p| super::is_zh_mirror(p));
    for zh in &zh_files {
        let r = super::rel(root, zh);
        let stripped = r
            .strip_prefix("docs/zh/")
            .unwrap_or(&r)
            .strip_suffix(".zh-CN.md")
            .map(|s| s.to_string())
            .unwrap_or_default();
        let english_rel = if stripped == "AGENTS" {
            "AGENTS.md".to_string()
        } else {
            format!("docs/{stripped}.md")
        };
        if !root.join(&english_rel).is_file() {
            bad.push(format!("orphan Chinese review copy: {r} (no {english_rel})"));
        }
    }
    if bad.is_empty() { Ok(()) } else { Err(bad) }
}

pub fn english_docs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let agents = root.join("AGENTS.md");
    if agents.is_file() {
        out.push(agents);
    }
    if let Ok(rd) = fs::read_dir(root.join("docs")) {
        for entry in rd.flatten() {
            let p = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if p.is_file() && name.ends_with(".md") && !super::is_zh_mirror(&p) {
                out.push(p);
            }
        }
    }
    out.extend(
        super::walk_files(&root.join("docs/superpowers"), &["md"])
            .into_iter()
            .filter(|p| !super::is_zh_mirror(p)),
    );
    out.sort();
    out
}

pub fn mirror_of(root: &Path, english: &Path) -> PathBuf {
    let r = super::rel(root, english);
    let stripped = r.strip_prefix("docs/").unwrap_or(&r);
    let stem = stripped.strip_suffix(".md").unwrap_or(stripped);
    root.join("docs/zh").join(format!("{stem}.zh-CN.md"))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;

    #[test]
    fn passes_when_every_doc_has_its_mirror() {
        let root = fixture(&[
            ("AGENTS.md", "rules\n"),
            ("docs/zh/AGENTS.zh-CN.md", "rules\n"),
            ("docs/foo.md", "doc\n"),
            ("docs/zh/foo.zh-CN.md", "doc\n"),
            ("docs/changelog/unreleased/x.md", "note\n"),
        ]);
        assert!(super::check(&root).is_ok());
    }

    #[test]
    fn fails_when_a_mirror_is_missing() {
        let root = fixture(&[("docs/foo.md", "doc\n")]);
        assert!(super::check(&root).unwrap_err().len() == 1);
    }

    #[test]
    fn fails_on_an_orphan_mirror() {
        let root = fixture(&[("docs/zh/foo.zh-CN.md", "doc\n")]);
        assert!(super::check(&root).unwrap_err().len() == 1);
    }

    #[test]
    fn mirror_paths_follow_the_rule() {
        let root = fixture(&[]);
        assert!(super::mirror_of(&root, &root.join("AGENTS.md")).ends_with("docs/zh/AGENTS.zh-CN.md"));
        assert!(super::mirror_of(&root, &root.join("docs/superpowers/specs/s.md")).ends_with("docs/zh/superpowers/specs/s.zh-CN.md"));
    }
}
