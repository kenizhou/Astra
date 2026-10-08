// SPDX-License-Identifier: GPL-3.0-only
//! Contract checks and shared file-walking helpers.

pub mod dash_hygiene;
pub mod file_length;
pub mod paths_ascii;
pub mod version_sync;

pub type CheckFn = fn(&Path) -> Result<(), Vec<String>>;

pub struct Check {
    pub name: &'static str,
    pub run: CheckFn,
}

/// All gate checks, cheapest first. Tasks register checks here as they land.
pub fn all() -> Vec<Check> {
    vec![
        Check { name: "paths-ascii", run: crate::checks::paths_ascii::check },
        Check { name: "file-length", run: crate::checks::file_length::check },
        Check { name: "version-sync", run: crate::checks::version_sync::check },
        Check { name: "dash-hygiene", run: crate::checks::dash_hygiene::check },
    ]
}

use std::fs;
use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", "dist", ".superpowers"];

/// Every file under `root` whose name ends with one of `exts` (all files when
/// `exts` is empty), skipping build output and the Chinese review copies.
pub fn walk_files(root: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, root, exts, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            if rel(root, &path) == "docs/zh" {
                continue;
            }
            walk(root, &path, exts, out);
        } else if exts.is_empty() || path.extension().is_some_and(|x| exts.contains(&x.to_string_lossy().as_ref())) {
            out.push(path);
        }
    }
}

pub fn read_lossy(path: &Path) -> String {
    fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// True when the file is a Chinese review copy (`*.zh-CN.md`).
pub fn is_zh_mirror(p: &Path) -> bool {
    p.to_string_lossy().contains(".zh-CN.")
}

/// Forward-slash relative path of `p` under `root`.
pub fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
pub mod test_support {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Creates a throwaway directory tree from `(relative path, contents)`
    /// pairs and returns its root.
    pub fn fixture(files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "astra-xtask-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        for (path, body) in files {
            let p = dir.join(path);
            fs::create_dir_all(p.parent().expect("fixture path has a parent")).expect("mkdir");
            fs::write(p, body).expect("write fixture");
        }
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::fixture;

    #[test]
    fn fixture_writes_files_under_a_temp_root() {
        let root = fixture(&[("docs/a.md", "hello\n"), ("xtask/src/main.rs", "fn main() {}\n")]);
        assert!(root.join("docs/a.md").is_file());
        assert!(root.join("xtask/src/main.rs").is_file());
        assert!(super::walk_files(&root, &["md"]).iter().any(|p| p.ends_with("docs/a.md")));
    }
}
