# SP0 Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bootstrap the Astra repository: lock the four reference projects, lay the workspace and rulebook, build the `xtask` gate with seven contract checks, generate the icon set, seed the documentation, and stand up CI and nightly skeletons.

**Architecture:** A Cargo workspace whose only member is `xtask`, a small task runner exposing `gate` (all contract checks, cheapest first) and `icons` (brand icon generation). All checks are pure functions over the repository tree with inline unit tests. Reference repositories are recorded in `refs/refs-lock.toml` as the porting baseline. GitHub Actions mirror the gate on push and run a three-OS nightly skeleton without publishing.

**Tech Stack:** Rust (edition 2021, dependency-free checks; `image` only for the icons command), Git Bash on Windows, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md` (sections 3, 4, 10, 11, 12, 13.1)

## Global Constraints

- English only across documentation, code, comments, and folder names; `docs/zh/` holds non-normative Chinese review copies only.
- British English in prose; no dash as punctuation: U+2014 and U+2013 are banned in every tracked text file, and a spaced hyphen as punctuation is banned in Markdown (list markers are not punctuation).
- Licence GPL-3.0-only; every `.rs`, `.ts`, `.tsx` file starts with `// SPDX-License-Identifier: GPL-3.0-only`.
- Files stay under 500 lines (gate-enforced).
- `VERSION` equals `[workspace.package] version` in `Cargo.toml` at all times; `VERSION` moves only in release PRs.
- Every in-scope English document change carries its `.zh-CN.md` mirror in the same commit. In scope: `AGENTS.md`, `docs/*.md`, `docs/superpowers/**/*.md`. Out of scope: `docs/changelog/**`, everything under `docs/zh/`.
- `cargo run -p xtask -- gate` passes before every push.
- Conventional commit messages (`feat:`, `docs:`, `chore:`, `test:`, `build:`).
- The repository is `D:\Projects\mailclient\Astra`; the reference root is `D:\Projects\mailclient`.

---

### Task 1: Pull and lock the reference projects

**Files:**
- Create: `refs/refs-lock.toml`
- Create: `docs/upstream-sync.md`
- Create: `docs/zh/upstream-sync.zh-CN.md`

**Interfaces:**
- Consumes: nothing (first task).
- Produces: `refs/refs-lock.toml` with keys `version`, `root`, and four `[[repo]]` tables (`name`, `path`, `url`, `role`, `baseline`, `last_ported`, `locked_at`); Task 12's `docs/README.md` links to `docs/upstream-sync.md`.

- [ ] **Step 1: Verify the reference working trees are pullable**

Run:
```bash
for r in kylins opensource/mail-calendar opensource/inbox-zero; do
  git -C "D:/Projects/mailclient/$r" status --porcelain
done
```
Expected: each prints at most untracked files (`.zcodeignore` is fine); no modified tracked files. If a tree is dirty, stop and report before pulling.

- [ ] **Step 2: Pull the three tracked references and fetch the engine upstream**

Run:
```bash
git -C "D:/Projects/mailclient/kylins" pull --ff-only origin main
git -C "D:/Projects/mailclient/opensource/mail-calendar" pull --ff-only origin main
git -C "D:/Projects/mailclient/opensource/inbox-zero" pull --ff-only origin main
git -C "D:/Projects/mailclient/opensource/email-calendar-sync-engine" fetch upstream main
```
Expected: fast-forward pulls (or "Already up to date"); the fetch completes without error. The engine fork is only fetched, never merged here.

- [ ] **Step 3: Write `refs/refs-lock.toml` from the actual revisions**

Run from `D:/Projects/mailclient/Astra`:
```bash
mkdir -p refs
MC=$(git -C "D:/Projects/mailclient/opensource/mail-calendar" rev-parse HEAD)
IZ=$(git -C "D:/Projects/mailclient/opensource/inbox-zero" rev-parse HEAD)
KY=$(git -C "D:/Projects/mailclient/kylins" rev-parse HEAD)
EN=$(git -C "D:/Projects/mailclient/opensource/email-calendar-sync-engine" rev-parse upstream/main)
cat > refs/refs-lock.toml <<EOF
# Reference projects tracked by Astra. Baselines are locked at bootstrap and
# frozen until feature parity (spec section 13.2). The 22:00 sync task
# advances last_ported; see docs/upstream-sync.md for the full contract.
version = 1
root = "D:/Projects/mailclient"

[[repo]]
name = "mail-calendar"
path = "opensource/mail-calendar"
url = "https://github.com/allodia-eu/mail-calendar.git"
role = "features-and-organisation"
baseline = "$MC"
last_ported = "$MC"
locked_at = "2026-10-08"

[[repo]]
name = "inbox-zero"
path = "opensource/inbox-zero"
url = "https://github.com/elie222/inbox-zero.git"
role = "ai-design"
baseline = "$IZ"
last_ported = "$IZ"
locked_at = "2026-10-08"

[[repo]]
name = "kylins"
path = "kylins"
url = "git@github.com:kenizhou/kylins-client.git"
role = "stack-ui-sidecar-security"
baseline = "$KY"
last_ported = "$KY"
locked_at = "2026-10-08"

[[repo]]
name = "email-calendar-sync-engine"
path = "opensource/email-calendar-sync-engine"
url = "https://github.com/allodia-eu/email-calendar-sync-engine.git"
role = "engine-upstream"
baseline = "$EN"
last_ported = "$EN"
locked_at = "2026-10-08"
EOF
```
Expected: the file exists and each `baseline` is a 40 character commit id.

- [ ] **Step 4: Verify the lock file matches reality**

Run:
```bash
grep -c 'baseline = "' refs/refs-lock.toml
git -C "D:/Projects/mailclient/opensource/mail-calendar" rev-parse HEAD
grep 'name = "mail-calendar"' -A 4 refs/refs-lock.toml | grep baseline
```
Expected: `4`, and the mail-calendar commit id matches the `baseline` line.

- [ ] **Step 5: Write `docs/upstream-sync.md` with this exact content**

```markdown
# Upstream sync contract

How Astra tracks its four reference projects. The rulebook (AGENTS.md) links
here; this document is the single place the sync process is described.

## Lock table

`refs/refs-lock.toml` records, per reference: `url`, `path` (relative to
`root`), `baseline` (the commit locked at bootstrap), `last_ported` (advances
as ports land), and `locked_at`.

## Lock phase

References are frozen at their baselines until feature parity: every
mail-calendar row in docs/capabilities.md is complete or explicitly N/A, AI
phases A1 to A4 are done, and the plugin host is in place. Until then no
reference is pulled forward into Astra.

## The 22:00 task

A local scheduled automation runs nightly at 22:00, idempotent, anchored on
`last_ported`:

1. Fetch all four references; diff `last_ported..HEAD`; skip when empty.
2. Classify changes: docs only, tests only, refactor, behaviour, new feature,
   security.
3. Port per the rulebook. Contract doc, capability matrix row, changelog
   fragment, and the Chinese review copy of any document land in the same
   change.
4. Security tagged changes are always held for review and never auto-merged.
5. Run the fast local gate subset; the full gate runs in CI.
6. Open a PR with a per-repository summary. Auto-merge only low-risk classes
   with a green gate; everything else waits for morning review with a digest.
7. On merge, advance `last_ported`.
8. UI style decisions and architectural conflicts are listed in the PR for
   the author to decide. Silent deviation is forbidden.

## Engine updates

The engine is git-pinned by revision in the workspace Cargo.toml. An upstream
update arrives by bumping the pin, adapting the core, and following the same
PR flow as any other port.
```

- [ ] **Step 6: Write the Chinese mirror `docs/zh/upstream-sync.zh-CN.md`**

Translate the document above into Chinese, open with the line
`> 非规范性译本，仅供审阅；以英文版为准。`, keep `refs/refs-lock.toml`,
`last_ported`, and file paths verbatim.

- [ ] **Step 7: Commit**

```bash
git add refs/refs-lock.toml docs/upstream-sync.md docs/zh/upstream-sync.zh-CN.md
git commit -m "chore: lock reference baselines and add upstream sync contract"
```

---

### Task 2: Workspace foundation

**Files:**
- Delete: `LICENSE` (Apache-2.0), replaced from mail-calendar
- Create: `.gitignore`, `rust-toolchain.toml`, `Cargo.toml`, `VERSION`, `branding/default.env`
- Rename: `Images/logo.jpg` -> `branding/icon-source.jpg`

**Interfaces:**
- Produces: workspace `Cargo.toml` with `members = ["xtask"]` (member added in Task 3) and `[workspace.package] version = "0.1.0"`; `VERSION` containing `0.1.0`; `branding/default.env` keys `ASTRA_PRODUCT_NAME` and `ASTRA_APP_ID` consumed by later branding injection.

- [ ] **Step 1: Replace the licence with GPL-3.0**

Run:
```bash
git -C "D:/Projects/mailclient/Astra" rm LICENSE
cp "D:/Projects/mailclient/opensource/mail-calendar/LICENSE" "D:/Projects/mailclient/Astra/LICENSE"
head -3 "D:/Projects/mailclient/Astra/LICENSE"
```
Expected: the header reads `GNU GENERAL PUBLIC LICENSE` and `Version 3, 29 June 2007`.

- [ ] **Step 2: Detect the installed toolchain and pin it**

Run: `rustc --version` and `cargo --version`. Then create `rust-toolchain.toml` with the detected stable version, for example for 1.98.0:
```toml
[toolchain]
channel = "1.98.0"
```
(Substitute the exact detected version; if the detected rustc is older than 1.85, report before continuing since edition 2021 still works but note the MSRV.)

- [ ] **Step 3: Create `.gitignore`, `VERSION`, `branding/default.env`, and the workspace manifest**

`.gitignore`:
```
/target
node_modules/
dist/
.env
*.local
```

`VERSION`:
```
0.1.0
```

`branding/default.env`:
```
ASTRA_PRODUCT_NAME=Astra Mail
ASTRA_APP_ID=com.kenizhou.astra
```

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = []

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "GPL-3.0-only"
repository = "https://github.com/kenizhou/Astra"

[workspace.lints.rust]
unsafe_code = "forbid"
```
(The `members` list is empty for exactly one commit; Task 3 adds `xtask`.)

- [ ] **Step 4: Move the icon source into branding**

Run:
```bash
mkdir -p branding
git mv Images/logo.jpg branding/icon-source.jpg
rmdir Images 2>/dev/null || true
git status --short
```
Expected: a rename `Images/logo.jpg -> branding/icon-source.jpg` staged.

- [ ] **Step 5: Verify the workspace resolves**

Run: `cargo metadata --no-deps --format-version 1 > /dev/null && echo OK`
Expected: `OK` (an empty-members workspace resolves).

- [ ] **Step 6: Commit**

```bash
git add LICENSE .gitignore rust-toolchain.toml Cargo.toml VERSION branding/
git commit -m "build: lay workspace foundation, GPL-3.0 licence, branding, version"
```

---

### Task 3: xtask skeleton with the gate runner

**Files:**
- Create: `xtask/Cargo.toml`, `xtask/src/main.rs`, `xtask/src/checks/mod.rs`
- Modify: `Cargo.toml` (add member)

**Interfaces:**
- Produces:
  - `xtask::checks::Check { name: &'static str, run: fn(&Path) -> Result<(), Vec<String>> }`
  - `xtask::checks::all() -> Vec<Check>` (starts empty; Tasks 4 to 10 register into it)
  - `xtask::checks::walk_files(root: &Path, exts: &[&str]) -> Vec<PathBuf>` (recursive, skips `.git`, `target`, `node_modules`, `dist`, and `docs/zh`)
  - `xtask::checks::read_lossy(path: &Path) -> String`
  - `xtask::checks::rel(root: &Path, p: &Path) -> String` (forward-slash relative path)
  - `xtask::checks::test_support::fixture(files: &[(&str, &str)]) -> PathBuf` (test-only)
  - CLI: `cargo run -p xtask -- gate` and `cargo run -p xtask -- check <name>`

- [ ] **Step 1: Write the failing test for the fixture helper and registry**

Create `xtask/src/checks/mod.rs` containing only the license header and:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Contract checks and shared file-walking helpers.

pub type CheckFn = fn(&Path) -> Result<(), Vec<String>>;

pub struct Check {
    pub name: &'static str,
    pub run: CheckFn,
}

/// All gate checks, cheapest first. Tasks register checks here as they land.
pub fn all() -> Vec<Check> {
    Vec::new()
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
```
(The `test_support` module does not exist yet; that is the failure.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p xtask` after adding the member (Step 3). Before the crate is wired the command cannot run at all, which is the red state; the first green run comes after Step 4.

- [ ] **Step 3: Create the crate and register the member**

`xtask/Cargo.toml`:
```toml
[package]
name = "xtask"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[lints]
workspace = true
```

Modify root `Cargo.toml`: `members = ["xtask"]`.

`xtask/src/main.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Astra task runner. `gate` runs every contract check; `check <name>` runs one.
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod checks;

fn main() -> ExitCode {
    let root = repo_root();
    let mut args = env::args().skip(1);
    match (args.next().as_deref(), args.next().as_deref()) {
        (Some("gate"), None) => gate(&root),
        (Some("check"), Some(name)) => one(&root, name),
        _ => {
            eprintln!("usage: cargo run -p xtask -- gate | check <name>");
            ExitCode::FAILURE
        }
    }
}

fn repo_root() -> PathBuf {
    env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|d| d.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn gate(root: &Path) -> ExitCode {
    let mut violations = 0usize;
    for check in checks::all() {
        print!("{} ... ", check.name);
        match (check.run)(root) {
            Ok(()) => println!("PASS"),
            Err(msgs) => {
                violations += msgs.len();
                println!("FAIL");
                for m in msgs {
                    println!("  {m}");
                }
            }
        }
    }
    if violations == 0 {
        println!("gate: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!("gate: {violations} violation(s)");
        ExitCode::FAILURE
    }
}

fn one(root: &Path, name: &str) -> ExitCode {
    let Some(check) = checks::all().into_iter().find(|c| c.name == name) else {
        eprintln!("unknown check: {name}");
        return ExitCode::FAILURE;
    };
    match (check.run)(root) {
        Ok(()) => {
            println!("{name}: PASS");
            ExitCode::SUCCESS
        }
        Err(msgs) => {
            for m in msgs {
                eprintln!("{m}");
            }
            ExitCode::FAILURE
        }
    }
}
```

- [ ] **Step 4: Implement the helpers in `checks/mod.rs`**

Append to `xtask/src/checks/mod.rs` (below the `all()` function, above the tests module):
```rust
use std::fs;
use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", "dist"];

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
```

- [ ] **Step 5: Run the tests and clippy**

Run:
```bash
cargo test -p xtask
cargo clippy -p xtask --all-targets -- -D warnings
```
Expected: 1 test passed; clippy clean.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml xtask/
git commit -m "feat: add xtask gate runner with file-walking helpers"
```

---

### Task 4: paths-ascii check

**Files:**
- Create: `xtask/src/checks/paths_ascii.rs`
- Modify: `xtask/src/checks/mod.rs` (register), `xtask/src/main.rs` (add `mod`)

**Interfaces:**
- Consumes: `walk_files`, `rel` from Task 3.
- Produces: check registered under the name `paths-ascii`; `pub fn check(root: &Path) -> Result<(), Vec<String>>`.

- [ ] **Step 1: Write the failing tests**

`xtask/src/checks/paths_ascii.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: every tracked path is ASCII outside docs/zh.

pub fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
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
    if bad.is_empty() { Ok(()) } else { Err(bad) }
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
```

- [ ] **Step 2: Wire the module and run to see the failure**

Add `pub mod paths_ascii;` to `checks/mod.rs` (module declarations block, see Step 4) and register later. Run: `cargo test -p xtask paths_ascii`
Expected: before wiring the module declaration the crate fails to compile (the red state); after wiring, all three tests pass on the first run. The registration in Step 3 is what makes `gate` see the check.

- [ ] **Step 3: Register the check**

In `checks/mod.rs`, extend `all()`:
```rust
pub fn all() -> Vec<Check> {
    vec![Check { name: "paths-ascii", run: crate::checks::paths_ascii::check }]
}
```

- [ ] **Step 4: Add module declarations layout**

Keep all module declarations in one block at the top of `checks/mod.rs` after the doc comment (repeat this pattern for every later check):
```rust
pub mod paths_ascii;
```

- [ ] **Step 5: Run gate and tests**

Run:
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
Expected: all tests pass; gate prints `paths-ascii ... PASS` and `gate: all checks passed`.

- [ ] **Step 6: Commit**

```bash
git add xtask/
git commit -m "feat: add paths-ascii gate check"
```

---

### Task 5: file-length check

**Files:**
- Create: `xtask/src/checks/file_length.rs`
- Modify: `xtask/src/checks/mod.rs`

**Interfaces:**
- Consumes: `walk_files`, `rel` from Task 3.
- Produces: check `file-length`; limit constant `pub const MAX_LINES: usize = 500;` applied to `.rs`, `.ts`, `.tsx`, `.md`.

- [ ] **Step 1: Write the failing tests**

`xtask/src/checks/file_length.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: no source or document file exceeds 500 lines.

pub const MAX_LINES: usize = 500;

pub fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for f in super::walk_files(root, &["rs", "ts", "tsx", "md"]) {
        let r = super::rel(root, &f);
        if r == "LICENSE" || r.starts_with("docs/zh/") || super::is_zh_mirror(&f) {
            continue;
        }
        let lines = super::read_lossy(&f).lines().count();
        if lines > MAX_LINES {
            bad.push(format!("{r}: {lines} lines (max {MAX_LINES})"));
        }
    }
    if bad.is_empty() { Ok(()) } else { Err(bad) }
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
    fn chinese_review_copies_are_exempt() {
        let long = "line\n".repeat(super::MAX_LINES + 1);
        let root = fixture(&[("docs/zh/a.zh-CN.md", &long)]);
        assert!(super::check(&root).is_ok());
    }
}
```

- [ ] **Step 2: Add `is_zh_mirror` to `checks/mod.rs`**

```rust
/// True when the file is a Chinese review copy (`*.zh-CN.md`).
pub fn is_zh_mirror(p: &Path) -> bool {
    p.to_string_lossy().contains(".zh-CN.")
}
```

- [ ] **Step 3: Register and run**

Add `pub mod file_length;`, extend `all()` with `Check { name: "file-length", run: crate::checks::file_length::check }` after `paths-ascii`. Run:
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
Expected: tests pass; `file-length ... PASS`.

- [ ] **Step 4: Commit**

```bash
git add xtask/
git commit -m "feat: add file-length gate check"
```

---

### Task 6: version-sync check

**Files:**
- Create: `xtask/src/checks/version_sync.rs`
- Modify: `xtask/src/checks/mod.rs`

**Interfaces:**
- Consumes: `read_lossy` from Task 3.
- Produces: check `version-sync`; `pub fn workspace_version(cargo_toml: &str) -> Option<String>` parsing the `[workspace.package]` version.

- [ ] **Step 1: Write the failing tests**

`xtask/src/checks/version_sync.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: VERSION and the workspace package version agree.

pub fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let version_file = root.join("VERSION");
    let cargo_file = root.join("Cargo.toml");
    if !version_file.is_file() || !cargo_file.is_file() {
        return Err(vec!["VERSION and Cargo.toml must both exist".to_string()]);
    }
    let declared = super::read_lossy(&version_file).trim().to_string();
    let Some(workspace) = workspace_version(&super::read_lossy(&cargo_file)) else {
        return Err(vec!["Cargo.toml has no [workspace.package] version".to_string()]);
    };
    if declared == workspace {
        Ok(())
    } else {
        Err(vec![format!("VERSION says {declared} but Cargo.toml says {workspace}")])
    }
}

/// Reads `version = "x.y.z"` from the `[workspace.package]` section.
pub fn workspace_version(cargo_toml: &str) -> Option<String> {
    let mut in_section = false;
    for line in cargo_toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t == "[workspace.package]";
            continue;
        }
        if in_section && t.starts_with("version") {
            let v = t.split('"').nth(1)?;
            return Some(v.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;

    const TOML: &str = "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"0.1.0\"\n";

    #[test]
    fn passes_when_versions_agree() {
        let root = fixture(&[("VERSION", "0.1.0\n"), ("Cargo.toml", TOML)]);
        assert!(super::check(&root).is_ok());
    }

    #[test]
    fn fails_when_versions_disagree() {
        let root = fixture(&[("VERSION", "0.2.0\n"), ("Cargo.toml", TOML)]);
        assert!(super::check(&root).is_err());
    }

    #[test]
    fn parser_reads_only_the_workspace_section() {
        let toml = "[package]\nversion = \"9.9.9\"\n\n[workspace.package]\nversion = \"0.1.0\"\n";
        assert_eq!(super::workspace_version(toml).as_deref(), Some("0.1.0"));
    }
}
```

- [ ] **Step 2: Register and run**

Add `pub mod version_sync;` and `Check { name: "version-sync", run: crate::checks::version_sync::check }`. Run:
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
Expected: tests pass; gate green against the real tree (`VERSION` 0.1.0 matches Task 2).

- [ ] **Step 3: Commit**

```bash
git add xtask/
git commit -m "feat: add version-sync gate check"
```

---

### Task 7: dash-hygiene check

**Files:**
- Create: `xtask/src/checks/dash_hygiene.rs`
- Modify: `xtask/src/checks/mod.rs`

**Interfaces:**
- Consumes: `walk_files`, `rel`, `read_lossy`, `is_zh_mirror` from Tasks 3 and 5.
- Produces: check `dash-hygiene`.

- [ ] **Step 1: Write the failing tests**

`xtask/src/checks/dash_hygiene.rs`:
```rust
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
        for (idx, line) in super::read_lossy(&f).lines().enumerate() {
            if line.contains(EM_DASH) || line.contains(EN_DASH) {
                bad.push(format!("{r}:{} contains a dash character", idx + 1));
            }
            if is_markdown && spaced_hyphen_as_punctuation(line) {
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
}
```

- [ ] **Step 2: Register and run**

Add `pub mod dash_hygiene;` and the `Check { name: "dash-hygiene", ... }` entry. Run:
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
Expected: tests pass; gate green (the SP0 tree so far contains no dashes).

- [ ] **Step 3: Commit**

```bash
git add xtask/
git commit -m "feat: add dash-hygiene gate check"
```

---

### Task 8: british-english check

**Files:**
- Create: `xtask/src/checks/british_english.rs`
- Modify: `xtask/src/checks/mod.rs`

**Interfaces:**
- Consumes: `walk_files`, `rel`, `read_lossy`, `is_zh_mirror` from Tasks 3 and 5.
- Produces: check `british-english`; `pub const AMERICANISMS: &[&str]` word list; `pub fn find_americanisms(text: &str) -> Vec<String>` returning matched words.

- [ ] **Step 1: Write the failing tests**

`xtask/src/checks/british_english.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: prose uses British spelling. Code identifiers are exempt by
//! the word boundary scan (casing inside identifiers does not match).

/// American spellings banned in prose. Extend as they are found; keep the
/// list alphabetical. `dialog`, `artifact`, and `catalog` stay American by
/// the mail-calendar rulebook.
pub const AMERICANISMS: &[&str] = &[
    "behavior", "behavioral", "canceled", "canceling", "color", "colored",
    "colors", "favorite", "favorites", "fulfill", "fulfillment", "labeled",
    "labeling", "maximize", "minimized", "organize", "organized",
];

pub fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for f in super::walk_files(root, &["md"]) {
        let r = super::rel(root, &f);
        if r.starts_with("docs/zh/") || super::is_zh_mirror(&f) {
            continue;
        }
        for word in find_americanisms(&super::read_lossy(&f)) {
            bad.push(format!("{r}: American spelling \"{word}\""));
        }
    }
    if bad.is_empty() { Ok(()) } else { Err(bad) }
}

/// Case-insensitive whole-word scan returning the matched source words.
pub fn find_americanisms(text: &str) -> Vec<String> {
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
}
```

- [ ] **Step 2: Register and run**

Add `pub mod british_english;` and the check entry named `british-english`. Run:
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
Expected: tests pass; gate green. If the gate flags an existing document, fix the document wording in the same commit, never the word list.

- [ ] **Step 3: Commit**

```bash
git add xtask/
git commit -m "feat: add british-english gate check"
```

---

### Task 9: license-headers check

**Files:**
- Create: `xtask/src/checks/license_headers.rs`
- Modify: `xtask/src/checks/mod.rs`

**Interfaces:**
- Consumes: `walk_files`, `rel`, `read_lossy` from Task 3.
- Produces: check `license-headers`; `pub const HEADER: &str = "// SPDX-License-Identifier: GPL-3.0-only";`.

- [ ] **Step 1: Write the failing tests**

`xtask/src/checks/license_headers.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: Rust and TypeScript sources carry the SPDX header.

pub const HEADER: &str = "// SPDX-License-Identifier: GPL-3.0-only";

pub fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for f in super::walk_files(root, &["rs", "ts", "tsx"]) {
        let r = super::rel(root, &f);
        let first = super::read_lossy(&f).lines().next().unwrap_or_default();
        if !first.contains(HEADER) {
            bad.push(format!("{r}: missing SPDX licence header"));
        }
    }
    if bad.is_empty() { Ok(()) } else { Err(bad) }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;

    #[test]
    fn passes_with_header() {
        let root = fixture(&[("a.rs", "// SPDX-License-Identifier: GPL-3.0-only\nfn a() {}\n")]);
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
```

- [ ] **Step 2: Register and run**

Add `pub mod license_headers;` and the `license-headers` entry. Run:
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
Expected: tests pass; gate green (every xtask source written so far already opens with the header).

- [ ] **Step 3: Commit**

```bash
git add xtask/
git commit -m "feat: add license-headers gate check"
```

---

### Task 10: doc-sync check

**Files:**
- Create: `xtask/src/checks/doc_sync.rs`
- Modify: `xtask/src/checks/mod.rs`

**Interfaces:**
- Consumes: `walk_files`, `rel`, `is_zh_mirror` from Tasks 3 and 5.
- Produces: check `doc-sync`; `pub fn english_docs(root: &Path) -> Vec<PathBuf>`; `pub fn mirror_of(root: &Path, english: &Path) -> PathBuf` implementing the mirrored path rule (`AGENTS.md` -> `docs/zh/AGENTS.zh-CN.md`, `docs/foo.md` -> `docs/zh/foo.zh-CN.md`, `docs/superpowers/plans/p.md` -> `docs/zh/superpowers/plans/p.zh-CN.md`).

- [ ] **Step 1: Write the failing tests**

`xtask/src/checks/doc_sync.rs`:
```rust
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
```

- [ ] **Step 2: Verify the tests pass, but hold the registration**

Add `pub mod doc_sync;` to the module declarations. Run `cargo test -p xtask`
and expect every doc_sync test to pass. Do NOT add the check to `all()` in
this task: on the real tree it would fail because this plan's own Chinese
review copy lands with Task 12. Task 12 Step 6 performs the registration
once the last mirror exists.

- [ ] **Step 3: Commit**

```bash
git add xtask/
git commit -m "feat: add doc-sync gate check (registered in Task 12)"
```

---

### Task 11: Icons command and generated icon set

**Files:**
- Create: `xtask/src/icons.rs`
- Modify: `xtask/Cargo.toml` (add `image` dependency), `xtask/src/main.rs` (wire `icons`), `xtask/src/checks/mod.rs` (nothing)

**Interfaces:**
- Consumes: `branding/icon-source.jpg` (Task 2).
- Produces: `pub fn generate(src: &Path, out_dir: &Path) -> Result<Vec<PathBuf>, String>` writing `1024x1024.png` ... `16x16.png` and `icon.ico` into `client/desktop/icons/`; CLI `cargo run -p xtask -- icons`.

- [ ] **Step 1: Add the image dependency**

Run: `cargo add image --package xtask` (accept the latest 0.x release; no other dependencies).

- [ ] **Step 2: Write the failing test and implementation**

`xtask/src/icons.rs`:
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Generates the full icon set from the branding source image.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::imageops::FilterType::Lanczos3;
use image::{DynamicImage, ImageFormat};

const SIZES: [u32; 8] = [1024, 512, 256, 128, 64, 48, 32, 16];
const ICO_SIZES: [u32; 4] = [256, 48, 32, 16];

pub fn generate(src: &Path, out_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let img = image::open(src).map_err(|e| format!("cannot read {}: {e}", src.display()))?;
    let img = img.into_rgba8();
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let mut written = Vec::new();
    let mut ico_entries: Vec<(u32, Vec<u8>)> = Vec::new();
    for size in SIZES {
        let resized = image::imageops::resize(&img, size, size, Lanczos3);
        let path = out_dir.join(format!("{size}x{size}.png"));
        image::DynamicImage::ImageRgba8(resized.clone())
            .save_with_format(&path, ImageFormat::Png)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if ICO_SIZES.contains(&size) {
            let mut buf = Vec::new();
            DynamicImage::ImageRgba8(resized)
                .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            ico_entries.push((size, buf));
        }
        written.push(path);
    }
    let ico_path = out_dir.join("icon.ico");
    write_png_ico(&ico_path, &ico_entries)?;
    written.push(ico_path);
    Ok(written)
}

/// Minimal ICO writer embedding PNG frames (Vista and later).
#[allow(clippy::cast_possible_truncation)]
fn write_png_ico(path: &Path, entries: &[(u32, Vec<u8>)]) -> Result<(), String> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0u8, 0, 1, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * entries.len();
    for (size, data) in entries {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in entries {
        out.extend_from_slice(data);
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;
    use image::{ImageBuffer, Rgba};

    #[test]
    fn generates_png_set_and_ico() {
        let root = fixture(&[]);
        let src = root.join("src.jpg");
        let img = ImageBuffer::from_pixel(64, 64, Rgba([10, 20, 30, 255]));
        DynamicImage::ImageRgba8(img)
            .save_with_format(&src, ImageFormat::Jpeg)
            .expect("fixture source");
        let out = root.join("icons");
        let written = super::generate(&src, &out).expect("generate");
        assert_eq!(written.len(), 9);
        assert!(out.join("icon.ico").is_file());
        let (w, h) = image::image_dimensions(out.join("128x128.png")).expect("dims");
        assert_eq!((w, h), (128, 128));
        let head = std::fs::read(out.join("icon.ico")).expect("ico");
        assert_eq!(&head[..4], &[0, 0, 1, 0]);
        assert_eq!(u16::from_le_bytes([head[4], head[5]]), 4);
    }
}
```

- [ ] **Step 3: Wire the CLI**

In `xtask/src/main.rs`, add `mod icons;` after `mod checks;` and extend the match:
```rust
        (Some("icons"), None) => {
            let written = icons::generate(&root.join("branding/icon-source.jpg"), &root.join("client/desktop/icons"));
            match written {
                Ok(files) => {
                    for f in files {
                        println!("{}", f.display());
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("icons: {e}");
                    ExitCode::FAILURE
                }
            }
        }
```
Place this arm before the catch-all `_` arm.

- [ ] **Step 4: Run tests, clippy, and generate the real set**

Run:
```bash
cargo test -p xtask
cargo clippy -p xtask --all-targets -- -D warnings
cargo run -p xtask -- icons
git status --short
```
Expected: tests pass; `client/desktop/icons/` contains 9 files (8 PNGs and `icon.ico`).

- [ ] **Step 5: Commit**

```bash
git add xtask/ client/desktop/icons/
git commit -m "feat: add icon generation pipeline and checked-in icon set"
```

---

### Task 12: AGENTS.md rulebook and documentation skeleton

**Files:**
- Create: `AGENTS.md`, `docs/README.md`, `docs/capabilities.md`, `docs/changelog.md`, `docs/changelog/unreleased/.gitkeep`
- Create: `docs/zh/AGENTS.zh-CN.md`, `docs/zh/README.zh-CN.md`, `docs/zh/capabilities.zh-CN.md`, `docs/zh/changelog.zh-CN.md`, `docs/zh/superpowers/plans/2026-10-08-sp0-bootstrap.zh-CN.md`
- Modify: `xtask/src/checks/mod.rs` (register `doc-sync`)

**Interfaces:**
- Consumes: doc-sync check from Task 10; mirror rule from its tests.
- Produces: the rulebook and the capability matrix that later slices update in the same change as the feature.

- [ ] **Step 1: Write `AGENTS.md` with this content**

```markdown
# AGENTS.md

The single rulebook for Astra, written for humans and coding agents alike.
Every rule lives here or in the one document it names; everywhere else links.

## Product

Astra Mail is a desktop mail client. The product is named "Astra Mail",
never "Astra" alone in user-facing copy. Name and app id come from
branding/default.env at build time.

## Writing

- Brief and concrete. Say it once: a rule lives in exactly one place.
- British English (`colour`, `licence` noun, `license` verb). `dialog`,
  `artifact`, and `catalog` stay American.
- No dash as punctuation: no em or en dash anywhere, and no spaced hyphen
  as punctuation in Markdown.
- Code comments describe the present only; no history, no plan phases,
  sparingly. Log lines never mention repository paths.
- English only across documentation, code, comments, and folder names.
  `docs/zh/**` holds non-normative Chinese review copies; when a document
  changes, its review copy changes in the same commit.

## Hard rules

- Files stay under 500 lines. The gate enforces it for every language.
- Newtypes for identities; no raw `String` ids across module boundaries.
- Test-first for behaviour changes. A bug fix lands with a regression test
  at the lowest shared layer that observes the contract.
- The engine is consumed only through `engine-api`. Protocol parsing belongs
  in the engine, never in Astra.
- Intents stay serde-serialisable. Surfaces are immutable snapshots.
- `docs/capabilities.md` moves in the same change as the feature it
  describes.
- `VERSION` moves only in a release PR and always together with the
  workspace package version.

## Contract changes

A contract is decided once in its document under `docs/` and binds the whole
product. Changing a contract takes: (1) the doc rule and matrix update,
(2) the implementation on every surface that ships it, (3) the
`docs/capabilities.md` row, (4) a changelog fragment in
`docs/changelog/unreleased/<slug>.md` with `Platforms:` and `Bump:` tags.
Shortfalls are recorded as known gaps, never left silent.

## Changelog fragments

One file per user-visible change: `docs/changelog/unreleased/<slug>.md`
containing the note, then `Platforms: desktop` and `Bump: none|minor|major`.
Fragments move to `docs/changelog/released/` in the release PR. Fragments
are English only and have no Chinese mirror.

## Security

kylins implementations win on security mechanics where they have the
advantage; mail-calendar wins on feature contracts. The sidecar is fail
closed: every gated tool starts as needs-approval, send-class verbs are
refused to the agent, and humans decide and send. Every LLM call, tool
call, and permission decision lands in its ledger with a correlation id.

## AI rules

- An AI behaviour change (prompt, pipeline, tool surface) requires an eval
  run in the same PR and a pipeline version bump.
- Model API keys live only in sidecar memory. Background tiers run from the
  host with the shared tier and use-case registry.

## CLI rules

`astra-cli` is the agent's tool-call entry point, designed agent-first.
Every verb ships with `--json` (versioned schema) and mutating verbs ship
with `--preview`. New verbs land in the same change as the feature they
expose, together with any new virtual permission tool.

## Plugin rules

Plugins run out of process against the versioned hook API in
`docs/plugins.md`. Hooks never touch the engine; send discipline cannot be
bypassed. Hook API changes follow the versioned process in that document.

## Upstream sync

References are locked per `refs/refs-lock.toml`; the process and the 22:00
task contract live in `docs/upstream-sync.md`. Silent deviation from a
reference decision is forbidden: disagreements are escalated in the PR.

## Build and verify

- Git Bash on Windows is the shell; bash 5 or newer.
- `cargo run -p xtask -- gate` before every push. CI mirrors the gate.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` are the floor.
- Traps discovered while working go into this section, appended one line
  each.
```

- [ ] **Step 2: Write `docs/README.md` with this content**

```markdown
# Astra documentation

| Document | Contract |
|---|---|
| ../AGENTS.md | The rulebook: writing, hard rules, contract changes, security, AI, CLI, plugins, sync |
| capabilities.md | The capability matrix: what ships, per area, with status |
| changelog.md | Fragment format and release flow |
| upstream-sync.md | Reference locks and the 22:00 porting task |
| superpowers/specs/ | Design specifications |
| superpowers/plans/ | Implementation plans |

Contract docs for feature areas (accounts, folder pane, search, drafts,
sending, calendar, invitations, contacts, mcp, rendering security, composer
security, plugins, cli, ai) are added as the slices that implement them
land. Each states the rule, then a matrix that makes compliance checkable.

Chinese review copies of everything in scope live under zh/ with `.md`
replaced by `.zh-CN.md`. They are non-normative.
```

- [ ] **Step 3: Seed `docs/capabilities.md`**

Transform `D:/Projects/mailclient/opensource/mail-calendar/docs/capabilities.md`:
keep every row whose shared core status is `✅` or `🚧`, drop rows that are
`⬜` or marked not applicable (the dash marker) in every client because
upstream has not implemented them, and reformat each
row as `| Area | Feature | Status | Source |` where Status is `⬜` (Astra has
nothing yet) and Source links the mail-calendar contract doc when one exists.
Keep the area grouping and order of the original. Open the file with:

```markdown
# Capabilities

What Astra ships, per area. A row moves from ⬜ to 🚧 to ✅ only in the same
change as the feature it describes (rulebook: Contract changes). Rows marked
N/A carry their reason in Notes. Source links the mail-calendar contract
this row ports.

| Area | Feature | Status | Source |
|---|---|---|---|
| Accounts | IMAP/SMTP account setup wizard with manual server entry | ⬜ | accounts.md |
| Accounts | Auto-detect from email address (JMAP probe, SRV, autoconfig, ISPDB, MX) | ⬜ | account-autodetect.md |
| Mail | Folder pane across accounts with nested folders and persisted expansion | ⬜ | folder-pane.md |
| Mail | Flat and threaded message list with windowed paging | ⬜ | |
| Calendar | Day, work week, and month grids with drag to create, move, resize | ⬜ | calendar.md |
```

(Continue with the remaining rows produced by the transformation above.)

Then append the Astra-specific sections:

```markdown
## Astra: plugins

| Feature | Status | Notes |
|---|---|---|
| Plugin host with manifest, lifecycle, supervision | ⬜ | SP1 |
| Versioned hook API v1 (compose.transform, message.decode) | ⬜ | SP1 skeleton |
| example-plugin e2e | ⬜ | SP1 |
| SMIME plugin package | ⬜ | SP-CRYPTO, post parity |
| OpenPGP plugin package | ⬜ | SP-CRYPTO, post parity |

## Astra: CLI

| Feature | Status | Notes |
|---|---|---|
| Read verbs with --json contract | ⬜ | grows with SP2+ slices |
| Gated mutating verbs with --preview | ⬜ | grows with SP2+ slices |
| Virtual permission tools mapping | ⬜ | SPA0 |
| Shell completions | ⬜ | SP-CLI |

## Astra: AI (inbox-zero port, phases per spec 9.3)

| Phase | Feature | Status |
|---|---|---|
| A1 | AI rules: prompt-to-rules, matching, deterministic execution | ⬜ |
| A1 | Sender categorisation | ⬜ |
| A1 | Cold email blocker | ⬜ |
| A2 | Reply drafting with writing style | ⬜ |
| A2 | Summarisation with cache | ⬜ |
| A2 | Assistant chat with tool budget | ⬜ |
| A3 | Knowledge, persona, snippets, autocomplete, translate | ⬜ |
| A4 | Digest, meeting briefs, analytics reports, split inbox | ⬜ |
```

- [ ] **Step 4: Write `docs/changelog.md` and the fragment folder**

`docs/changelog.md`:
```markdown
# Changelog contract

User-visible changes get a fragment in unreleased/ named `<slug>.md`:

    One to three sentences describing the change from the user's view.

    Platforms: desktop
    Bump: none

`Bump` is the suggested semver movement (none, minor, major). The release PR
moves fragments to released/ under the coming version, oldest first, and is
the only place VERSION moves. Fragments are English only.
```

Run: `mkdir -p docs/changelog/unreleased && touch docs/changelog/unreleased/.gitkeep`

- [ ] **Step 5: Write the Chinese mirrors**

Create faithful translations opening with
`> 非规范性译本，仅供审阅；以英文版为准。`:
`docs/zh/AGENTS.zh-CN.md`, `docs/zh/README.zh-CN.md`,
`docs/zh/capabilities.zh-CN.md` (translate the header and Astra sections
fully; for the ported mail-calendar rows translate the header note and keep
the table rows with feature text translated), `docs/zh/changelog.zh-CN.md`.
Copy this plan's Chinese review edition to
`docs/zh/superpowers/plans/2026-10-08-sp0-bootstrap.zh-CN.md` (it travels
with this plan).

- [ ] **Step 6: Register doc-sync and run the full gate**

In `checks/mod.rs`, add `pub mod doc_sync;` (if not already) and append
`Check { name: "doc-sync", run: crate::checks::doc_sync::check }` to `all()`.
Run:
```bash
cargo run -p xtask -- gate
```
Expected: all seven checks PASS, including doc-sync.

- [ ] **Step 7: Commit**

```bash
git add AGENTS.md docs/ xtask/
git commit -m "docs: add rulebook, capability matrix, changelog contract, doc mirrors"
```

---

### Task 13: CI and nightly skeletons, push, verify

**Files:**
- Create: `.github/workflows/ci.yml`, `.github/workflows/nightly.yml`

**Interfaces:**
- Consumes: the gate from Tasks 3 to 12.
- Produces: required `ci-ok` check on push and PR; nightly three-OS skeleton on cron `30 16 * * *` UTC with publishing deferred.

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:

jobs:
  ci-ok:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: Format
        run: cargo fmt --all --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Tests
        run: cargo test --workspace
      - name: Gate
        run: cargo run -p xtask -- gate
```

- [ ] **Step 2: Write `.github/workflows/nightly.yml`**

```yaml
name: Nightly
on:
  schedule:
    - cron: "30 16 * * *" # 00:30 Beijing time
  workflow_dispatch:

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: Build
        run: cargo build --workspace
      - name: Gate
        run: cargo run -p xtask -- gate
```

(Publishing, version stamping, and the keep-ten retention job activate in
SP1 when the Tauri bundle exists; this skeleton proves the matrix builds.)

- [ ] **Step 3: Run the gate locally one last time**

Run: `cargo run -p xtask -- gate`
Expected: `gate: all checks passed`.

- [ ] **Step 4: Commit and push**

```bash
git add .github/
git commit -m "build: add CI gate workflow and nightly skeleton"
git push origin main
```

- [ ] **Step 5: Verify CI ran**

Run: `gh run list --repo kenizhou/Astra --limit 3` (if `gh` is unavailable,
open `https://github.com/kenizhou/Astra/actions` and verify the CI run).
Expected: a run named `CI` on the pushed commit, conclusion success. If it
fails, read the log, fix, commit, push again (the gate must have passed
locally first; a CI-only failure means an environment difference worth a
rulebook trap line).

---

## Self-review notes

- Spec coverage against SP0 (spec section 15): reference pull and lock
  (Task 1), repository skeleton (Tasks 2, 11, 12), LICENSE to GPL-3.0
  (Task 2), AGENTS.md (Task 12), xtask gate skeleton (Tasks 3 to 10),
  icon pipeline (Task 11), CI skeleton (Task 13), nightly skeleton
  (Task 13), VERSION and branding (Task 2), docs skeleton and seeded
  capability matrix (Task 12). All covered.
- The `22:00` scheduled automation itself is rehearsed and activated at
  feature parity (spec section 15, final bullet); creating the ZCode
  schedule now would violate the lock phase, so it is intentionally absent
  from SP0.
- Sequencing: doc-sync is implemented in Task 10 but registered in Task 12
  because the plan file's own Chinese mirror lands with Task 12.
