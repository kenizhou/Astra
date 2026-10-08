# SP0 引导实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-08-sp0-bootstrap.md`）。
> 代码块、命令与文件路径保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 引导 Astra 仓库：锁定四个参考项目、铺工作区与规则书、构建带七项契约检查的 `xtask` gate、生成图标集、铺设文档骨架、立起 CI 与 nightly 骨架。

**架构：** 一个 Cargo workspace，唯一成员是 `xtask`——暴露 `gate`（全部契约检查，从廉到贵）与 `icons`（品牌图标生成）的小任务运行器。所有检查都是对仓库树的纯函数并带内联单元测试。参考仓库记录在 `refs/refs-lock.toml` 作为移植基线。GitHub Actions 在 push 时镜像 gate，并跑不发布产物的三 OS nightly 骨架。

**技术栈：** Rust（edition 2021，检查部分零依赖；仅 icons 命令用 `image`）、Windows 上的 Git Bash、GitHub Actions。

**Spec：** `docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 3、4、10、11、12、13.1 节）

## 全局约束

- 文档、代码、注释与文件夹名只用英文；`docs/zh/` 只放非规范性中文审阅版。
- 散文用英式英语；破折号不作标点：所有被跟踪文本文件禁止 U+2014 与 U+2013，Markdown 额外禁止空格连字符作标点（列表标记不算标点）。
- 许可证 GPL-3.0-only；每个 `.rs`、`.ts`、`.tsx` 文件以 `// SPDX-License-Identifier: GPL-3.0-only` 开头。
- 源码文件不超过 500 行（gate 强制；Markdown 散文豁免）。
- `VERSION` 始终等于 `Cargo.toml` 的 `[workspace.package] version`；`VERSION` 只在 release PR 中移动。
- 范围内英文文档的每次变更都在同一提交携带其 `.zh-CN.md` 镜像。范围内：`AGENTS.md`、`docs/*.md`、`docs/superpowers/**/*.md`。范围外：`docs/changelog/**` 与 `docs/zh/` 下的一切。
- 每次 push 前通过 `cargo run -p xtask -- gate`。
- Conventional commits（`feat:`、`docs:`、`chore:`、`test:`、`build:`）。
- 仓库是 `D:\Projects\mailclient\Astra`；参考根是 `D:\Projects\mailclient`。

---

### 任务 1：拉取并锁定参考项目

**文件：**
- 新建：`refs/refs-lock.toml`
- 新建：`docs/upstream-sync.md`
- 新建：`docs/zh/upstream-sync.zh-CN.md`

**接口：**
- 消费：无（首任务）。
- 产出：`refs/refs-lock.toml`，含 `version`、`root` 与四个 `[[repo]]` 表（`name`、`path`、`url`、`role`、`baseline`、`last_ported`、`locked_at`）；任务 12 的 `docs/README.md` 链接 `docs/upstream-sync.md`。

- [ ] **步骤 1：确认参考工作树可拉取**

运行：
```bash
for r in kylins opensource/mail-calendar opensource/inbox-zero; do
  git -C "D:/Projects/mailclient/$r" status --porcelain
done
```
预期：每个仓库至多输出未跟踪文件（`.zcodeignore` 无妨）；没有已修改的被跟踪文件。若有工作树不干净，先停下报告再拉取。

- [ ] **步骤 2：拉取三个被跟踪参考并 fetch engine upstream**

运行：
```bash
git -C "D:/Projects/mailclient/kylins" pull --ff-only origin main
git -C "D:/Projects/mailclient/opensource/mail-calendar" pull --ff-only origin main
git -C "D:/Projects/mailclient/opensource/inbox-zero" pull --ff-only origin main
git -C "D:/Projects/mailclient/opensource/email-calendar-sync-engine" fetch upstream main
```
预期：快进拉取（或 "Already up to date"）；fetch 无错。engine fork 只 fetch，绝不在此合并。

- [ ] **步骤 3：用实际 revision 写 `refs/refs-lock.toml`**

在 `D:/Projects/mailclient/Astra` 运行：
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
预期：文件存在，每个 `baseline` 是 40 字符 commit id。

- [ ] **步骤 4：校验锁文件与现实一致**

运行：
```bash
grep -c 'baseline = "' refs/refs-lock.toml
git -C "D:/Projects/mailclient/opensource/mail-calendar" rev-parse HEAD
grep 'name = "mail-calendar"' -A 4 refs/refs-lock.toml | grep baseline
```
预期：`4`，且 mail-calendar 的 commit id 与 `baseline` 行一致。

- [ ] **步骤 5：按确切内容写 `docs/upstream-sync.md`**

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

- [ ] **步骤 6：写中文镜像 `docs/zh/upstream-sync.zh-CN.md`**

将上文译为中文，开头加
`> 非规范性译本，仅供审阅；以英文版为准。`，`refs/refs-lock.toml`、
`last_ported` 与文件路径原样保留。

- [ ] **步骤 7：提交**

```bash
git add refs/refs-lock.toml docs/upstream-sync.md docs/zh/upstream-sync.zh-CN.md
git commit -m "chore: lock reference baselines and add upstream sync contract"
```

---

### 任务 2：工作区地基

**文件：**
- 删除：`LICENSE`（Apache-2.0），从 mail-calendar 替换
- 新建：`.gitignore`、`rust-toolchain.toml`、`Cargo.toml`、`VERSION`、`branding/default.env`
- 重命名：`Images/logo.jpg` -> `branding/icon-source.jpg`

**接口：**
- 产出：workspace `Cargo.toml` 带 `members = ["xtask"]`（成员在任务 3 加入）与 `[workspace.package] version = "0.1.0"`；`VERSION` 内容 `0.1.0`；`branding/default.env` 键 `ASTRA_PRODUCT_NAME` 与 `ASTRA_APP_ID`，供后续品牌注入消费。

- [ ] **步骤 1：许可证替换为 GPL-3.0**

运行：
```bash
git -C "D:/Projects/mailclient/Astra" rm LICENSE
cp "D:/Projects/mailclient/opensource/mail-calendar/LICENSE" "D:/Projects/mailclient/Astra/LICENSE"
head -3 "D:/Projects/mailclient/Astra/LICENSE"
```
预期：开头为 `GNU GENERAL PUBLIC LICENSE` 与 `Version 3, 29 June 2007`。

- [ ] **步骤 2：探测已装工具链并锁定**

运行 `rustc --version` 与 `cargo --version`。然后以探测到的 stable 版本创建 `rust-toolchain.toml`，例如 1.98.0：
```toml
[toolchain]
channel = "1.98.0"
```
（代入探测到的确切版本；若 rustc 低于 1.85，先报告再继续——edition 2021 仍可用但要记 MSRV。）

- [ ] **步骤 3：创建 `.gitignore`、`VERSION`、`branding/default.env` 与 workspace 清单**

`.gitignore`：
```
/target
node_modules/
dist/
.env
*.local
```

`VERSION`：
```
0.1.0
```

`branding/default.env`：
```
ASTRA_PRODUCT_NAME=Astra Mail
ASTRA_APP_ID=com.kenizhou.astra
```

`Cargo.toml`：
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
（`members` 只为这一次提交留空；任务 3 加入 `xtask`。）

- [ ] **步骤 4：图标源图移入 branding**

运行：
```bash
mkdir -p branding
git mv Images/logo.jpg branding/icon-source.jpg
rmdir Images 2>/dev/null || true
git status --short
```
预期：暂存了重命名 `Images/logo.jpg -> branding/icon-source.jpg`。

- [ ] **步骤 5：验证 workspace 可解析**

运行：`cargo metadata --no-deps --format-version 1 > /dev/null && echo OK`
预期：`OK`（空成员 workspace 可解析）。

- [ ] **步骤 6：提交**

```bash
git add LICENSE .gitignore rust-toolchain.toml Cargo.toml VERSION branding/
git commit -m "build: lay workspace foundation, GPL-3.0 licence, branding, version"
```

---

### 任务 3：xtask 骨架与 gate 运行器

**文件：**
- 新建：`xtask/Cargo.toml`、`xtask/src/main.rs`、`xtask/src/checks/mod.rs`
- 修改：`Cargo.toml`（加成员）

**接口：**
- 产出：
  - `xtask::checks::Check { name: &'static str, run: fn(&Path) -> Result<(), Vec<String>> }`
  - `xtask::checks::all() -> Vec<Check>`（初始为空；任务 4 至 10 逐个注册）
  - `xtask::checks::walk_files(root: &Path, exts: &[&str]) -> Vec<PathBuf>`（递归，跳过 `.git`、`target`、`node_modules`、`dist` 与 `docs/zh`）
  - `xtask::checks::read_lossy(path: &Path) -> String`
  - `xtask::checks::rel(root: &Path, p: &Path) -> String`（正斜杠相对路径）
  - `xtask::checks::test_support::fixture(files: &[(&str, &str)]) -> PathBuf`（仅测试）
  - CLI：`cargo run -p xtask -- gate` 与 `cargo run -p xtask -- check <name>`

- [ ] **步骤 1：为 fixture 辅助与注册表写失败测试**

创建 `xtask/src/checks/mod.rs`，只含许可证头与：
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
（`test_support` 模块尚不存在——这正是失败点。）

- [ ] **步骤 2：跑测试确认失败**

加入成员（步骤 3）后运行 `cargo test -p xtask`——此前 crate 不存在；接好后应看到指名 `test_support` 缺失的编译错误。

- [ ] **步骤 3：创建 crate 并注册成员**

`xtask/Cargo.toml`：
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

修改根 `Cargo.toml`：`members = ["xtask"]`。

`xtask/src/main.rs`：
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

- [ ] **步骤 4：在 `checks/mod.rs` 实现辅助函数**

在 `all()` 之后、tests 模块之前追加：
```rust
use std::fs;
use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", "dist"];

/// Every file under `root` whose extension is one of `exts` (all files when
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

- [ ] **步骤 5：跑测试与 clippy**

运行：
```bash
cargo test -p xtask
cargo clippy -p xtask --all-targets -- -D warnings
```
预期：1 个测试通过；clippy 干净。

- [ ] **步骤 6：提交**

```bash
git add Cargo.toml xtask/
git commit -m "feat: add xtask gate runner with file-walking helpers"
```

---

### 任务 4：paths-ascii 检查

**文件：**
- 新建：`xtask/src/checks/paths_ascii.rs`
- 修改：`xtask/src/checks/mod.rs`（注册）、`xtask/src/main.rs`（无需）

**接口：**
- 消费：任务 3 的 `walk_files`、`rel`。
- 产出：以名称 `paths-ascii` 注册的检查；`pub fn check(root: &Path) -> Result<(), Vec<String>>`。

- [ ] **步骤 1：写失败测试**

`xtask/src/checks/paths_ascii.rs`：
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

- [ ] **步骤 2：接模块并跑**

在 `checks/mod.rs` 顶部模块声明区加 `pub mod paths_ascii;`（步骤 4 给出布局）。

- [ ] **步骤 3：注册检查**

扩展 `all()`：
```rust
pub fn all() -> Vec<Check> {
    vec![Check { name: "paths-ascii", run: crate::checks::paths_ascii::check }]
}
```

- [ ] **步骤 4：模块声明布局**

模块声明统一放在 `checks/mod.rs` 顶部文档注释之后（后续每个检查重复此模式）：
```rust
pub mod paths_ascii;
```

- [ ] **步骤 5：跑 gate 与测试**

运行：
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
预期：全部测试通过；gate 打印 `paths-ascii ... PASS` 与 `gate: all checks passed`。

- [ ] **步骤 6：提交**

```bash
git add xtask/
git commit -m "feat: add paths-ascii gate check"
```

---

### 任务 5：file-length 检查

**文件：**
- 新建：`xtask/src/checks/file_length.rs`
- 修改：`xtask/src/checks/mod.rs`

**接口：**
- 消费：任务 3 的 `walk_files`、`rel`。
- 产出：检查 `file-length`；常量 `pub const MAX_LINES: usize = 500;`，作用于 `.rs`、`.ts`、`.tsx`、`.css`、`.json`、`.yml`、`.yaml`、`.toml`、`.sh`（Markdown 豁免）。

- [ ] **步骤 1：写失败测试**

`xtask/src/checks/file_length.rs`：
```rust
// SPDX-License-Identifier: GPL-3.0-only
//! Gate check: no source file exceeds 500 lines. Markdown documents are
//! exempt: prose length is judged by review, not by a line budget.

pub const MAX_LINES: usize = 500;

pub fn check(root: &std::path::Path) -> Result<(), Vec<String>> {
    let mut bad = Vec::new();
    for f in super::walk_files(root, &["rs", "ts", "tsx", "css", "json", "yml", "yaml", "toml", "sh"]) {
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
    fn markdown_is_exempt() {
        let long = "line\n".repeat(super::MAX_LINES + 1);
        let root = fixture(&[("a.md", &long), ("docs/zh/a.zh-CN.md", &long)]);
        assert!(super::check(&root).is_ok());
    }
}
```

- [ ] **步骤 2：在 `checks/mod.rs` 加 `is_zh_mirror`**

```rust
/// True when the file is a Chinese review copy (`*.zh-CN.md`).
pub fn is_zh_mirror(p: &Path) -> bool {
    p.to_string_lossy().contains(".zh-CN.")
}
```

- [ ] **步骤 3：注册并跑**

加 `pub mod file_length;`，在 `paths-ascii` 之后注册 `file-length`。运行：
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
预期：测试通过；`file-length ... PASS`。

- [ ] **步骤 4：提交**

```bash
git add xtask/
git commit -m "feat: add file-length gate check"
```

---

### 任务 6：version-sync 检查

**文件：**
- 新建：`xtask/src/checks/version_sync.rs`
- 修改：`xtask/src/checks/mod.rs`

**接口：**
- 消费：任务 3 的 `read_lossy`。
- 产出：检查 `version-sync`；`pub fn workspace_version(cargo_toml: &str) -> Option<String>` 解析 `[workspace.package]` 的 version。

- [ ] **步骤 1：写失败测试**

`xtask/src/checks/version_sync.rs`：
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

- [ ] **步骤 2：注册并跑**

加 `pub mod version_sync;` 与 `version-sync` 条目。运行：
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
预期：测试通过；对真实树 gate 绿（`VERSION` 0.1.0 与任务 2 一致）。

- [ ] **步骤 3：提交**

```bash
git add xtask/
git commit -m "feat: add version-sync gate check"
```

---

### 任务 7：dash-hygiene 检查

**文件：**
- 新建：`xtask/src/checks/dash_hygiene.rs`
- 修改：`xtask/src/checks/mod.rs`

**接口：**
- 消费：任务 3 与 5 的 `walk_files`、`rel`、`read_lossy`、`is_zh_mirror`。
- 产出：检查 `dash-hygiene`。

- [ ] **步骤 1：写失败测试**

`xtask/src/checks/dash_hygiene.rs`：
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
```

- [ ] **步骤 2：注册并跑**

加 `pub mod dash_hygiene;` 与 `dash-hygiene` 条目。运行：
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
预期：测试通过；gate 绿（SP0 至此的树里没有破折号）。

- [ ] **步骤 3：提交**

```bash
git add xtask/
git commit -m "feat: add dash-hygiene gate check"
```

---

### 任务 8：british-english 检查

**文件：**
- 新建：`xtask/src/checks/british_english.rs`
- 修改：`xtask/src/checks/mod.rs`

**接口：**
- 消费：任务 3 与 5 的 `walk_files`、`rel`、`read_lossy`、`is_zh_mirror`。
- 产出：检查 `british-english`；词表 `pub const AMERICANISMS: &[&str]`；扫描函数 `pub fn find_americanisms(text: &str) -> Vec<String>`。

- [ ] **步骤 1：写失败测试**

`xtask/src/checks/british_english.rs`：
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

    #[test]
    fn fenced_code_is_exempt() {
        let root = fixture(&[("a.md", "prose\n\n```rust\nlet s = \"the color\";\n```\n\nmore prose\n")]);
        assert!(super::check(&root).is_ok());
    }
}
```

- [ ] **步骤 2：注册并跑**

加 `pub mod british_english;` 与 `british-english` 条目。运行：
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
预期：测试通过；gate 绿。若 gate 标记了既有文档，同一提交里改文档措辞，绝不改词表放水。

- [ ] **步骤 3：提交**

```bash
git add xtask/
git commit -m "feat: add british-english gate check"
```

---

### 任务 9：license-headers 检查

**文件：**
- 新建：`xtask/src/checks/license_headers.rs`
- 修改：`xtask/src/checks/mod.rs`

**接口：**
- 消费：任务 3 的 `walk_files`、`rel`、`read_lossy`。
- 产出：检查 `license-headers`；`pub const HEADER: &str = "// SPDX-License-Identifier: GPL-3.0-only";`。

- [ ] **步骤 1：写失败测试**

`xtask/src/checks/license_headers.rs`：
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

- [ ] **步骤 2：注册并跑**

加 `pub mod license_headers;` 与 `license-headers` 条目。运行：
```bash
cargo test -p xtask
cargo run -p xtask -- gate
```
预期：测试通过；gate 绿（至此所有 xtask 源文件都以头部开场）。

- [ ] **步骤 3：提交**

```bash
git add xtask/
git commit -m "feat: add license-headers gate check"
```

---

### 任务 10：doc-sync 检查

**文件：**
- 新建：`xtask/src/checks/doc_sync.rs`
- 修改：`xtask/src/checks/mod.rs`

**接口：**
- 消费：任务 3 与 5 的 `walk_files`、`rel`、`is_zh_mirror`。
- 产出：检查 `doc-sync`；`pub fn english_docs(root: &Path) -> Vec<PathBuf>`；`pub fn mirror_of(root: &Path, english: &Path) -> PathBuf` 实现镜像路径规则（`AGENTS.md` -> `docs/zh/AGENTS.zh-CN.md`、`docs/foo.md` -> `docs/zh/foo.zh-CN.md`、`docs/superpowers/plans/p.md` -> `docs/zh/superpowers/plans/p.zh-CN.md`）。

- [ ] **步骤 1：写失败测试**

`xtask/src/checks/doc_sync.rs`：
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

- [ ] **步骤 2：测试通过，但注册推迟**

加 `pub mod doc_sync;`。运行 `cargo test -p xtask` 预期全绿。真实树上 doc-sync 会因计划文件自身的镜像尚未落地而失败，所以本任务**不**把它加入 `all()`；注册在任务 12 最后一个镜像落地时完成。

- [ ] **步骤 3：提交**

```bash
git add xtask/
git commit -m "feat: add doc-sync gate check (registered in Task 12)"
```

---

### 任务 11：icons 命令与生成的图标集

**文件：**
- 新建：`xtask/src/icons.rs`
- 修改：`xtask/Cargo.toml`（加 `image` 依赖）、`xtask/src/main.rs`（接 `icons`）

**接口：**
- 消费：任务 2 的 `branding/icon-source.jpg`。
- 产出：`pub fn generate(src: &Path, out_dir: &Path) -> Result<Vec<PathBuf>, String>`，向 `client/desktop/icons/` 写 `1024x1024.png` 至 `16x16.png` 与 `icon.ico`；CLI `cargo run -p xtask -- icons`。

- [ ] **步骤 1：加 image 依赖**

运行：`cargo add image --package xtask`（接受最新 0.x；不加其他依赖）。

- [ ] **步骤 2：写测试与实现**

`xtask/src/icons.rs`：
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

- [ ] **步骤 3：接 CLI**

在 `xtask/src/main.rs` 的 `mod checks;` 后加 `mod icons;`，并在 match 中、兜底 `_` 分支之前加：
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

- [ ] **步骤 4：跑测试、clippy 并生成真实图标集**

运行：
```bash
cargo test -p xtask
cargo clippy -p xtask --all-targets -- -D warnings
cargo run -p xtask -- icons
git status --short
```
预期：测试通过；`client/desktop/icons/` 含 9 个文件（8 个 PNG 与 `icon.ico`）。

- [ ] **步骤 5：提交**

```bash
git add xtask/ client/desktop/icons/
git commit -m "feat: add icon generation pipeline and checked-in icon set"
```

---

### 任务 12：AGENTS.md 规则书与文档骨架

**文件：**
- 新建：`AGENTS.md`、`docs/README.md`、`docs/capabilities.md`、`docs/changelog.md`、`docs/changelog/unreleased/.gitkeep`
- 新建：`docs/zh/AGENTS.zh-CN.md`、`docs/zh/README.zh-CN.md`、`docs/zh/capabilities.zh-CN.md`、`docs/zh/changelog.zh-CN.md`、`docs/zh/superpowers/plans/2026-10-08-sp0-bootstrap.zh-CN.md`
- 修改：`xtask/src/checks/mod.rs`（注册 `doc-sync`）

**接口：**
- 消费：任务 10 的 doc-sync 检查及其测试中的镜像规则。
- 产出：规则书与能力矩阵，后续切片与功能同变更更新。

- [ ] **步骤 1：按下述内容写 `AGENTS.md`**

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

- Source files stay under 500 lines. The gate enforces it for every source
  language; Markdown prose is exempt.
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

- [ ] **步骤 2：按下述内容写 `docs/README.md`**

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

- [ ] **步骤 3：播种 `docs/capabilities.md`**

对 `D:/Projects/mailclient/opensource/mail-calendar/docs/capabilities.md`
做变换：保留共享核心状态为 `✅` 或 `🚧` 的每一行，丢弃在所有客户端都为
`⬜` 或标记为不适用（短横线标记）的行（上游尚未实现），并把每行重排为
`| Area | Feature | Status | Source |`，其中 Status 一律 `⬜`（Astra 尚无
实现），Source 在存在契约文档时链接之。保持原文件的分组与顺序。文件开头：

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

（其余行按上述变换继续产出。）

然后追加 Astra 专属段：

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

- [ ] **步骤 4：写 `docs/changelog.md` 与 fragment 目录**

`docs/changelog.md`：
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

运行：`mkdir -p docs/changelog/unreleased && touch docs/changelog/unreleased/.gitkeep`

- [ ] **步骤 5：写中文镜像**

创建忠实译本，开头为
`> 非规范性译本，仅供审阅；以英文版为准。`：
`docs/zh/AGENTS.zh-CN.md`、`docs/zh/README.zh-CN.md`、
`docs/zh/capabilities.zh-CN.md`（翻译头部说明与 Astra 专属段；移植的
mail-calendar 行翻译表头并译功能文字）、`docs/zh/changelog.zh-CN.md`。
把本计划的中文审阅版复制为
`docs/zh/superpowers/plans/2026-10-08-sp0-bootstrap.zh-CN.md`（与本计划
同行）。

- [ ] **步骤 6：注册 doc-sync 并跑全量 gate**

在 `checks/mod.rs` 加 `pub mod doc_sync;`（若尚未加）并把
`Check { name: "doc-sync", run: crate::checks::doc_sync::check }` 追加进
`all()`。运行：
```bash
cargo run -p xtask -- gate
```
预期：全部七项检查 PASS，含 doc-sync。

- [ ] **步骤 7：提交**

```bash
git add AGENTS.md docs/ xtask/
git commit -m "docs: add rulebook, capability matrix, changelog contract, doc mirrors"
```

---

### 任务 13：CI 与 nightly 骨架、推送、验证

**文件：**
- 新建：`.github/workflows/ci.yml`、`.github/workflows/nightly.yml`

**接口：**
- 消费：任务 3 至 12 的 gate。
- 产出：push 与 PR 上的必需 `ci-ok` 检查；cron `30 16 * * *` UTC 的三 OS nightly 骨架（发布延后）。

- [ ] **步骤 1：写 `.github/workflows/ci.yml`**

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

- [ ] **步骤 2：写 `.github/workflows/nightly.yml`**

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

（发布、版本戳与「只留十个」清理任务在 SP1 有了 Tauri 包后激活；本骨架
证明矩阵可构建。）

- [ ] **步骤 3：最后跑一次本地 gate**

运行：`cargo run -p xtask -- gate`
预期：`gate: all checks passed`。

- [ ] **步骤 4：提交并推送**

```bash
git add .github/
git commit -m "build: add CI gate workflow and nightly skeleton"
git push origin main
```

- [ ] **步骤 5：验证 CI 已运行**

运行：`gh run list --repo kenizhou/Astra --limit 3`（若无 `gh`，打开
`https://github.com/kenizhou/Astra/actions` 核对 CI 运行）。
预期：推送的 commit 上有一次名为 `CI` 的运行且成功。若失败，读日志、修复、
再提交推送（本地 gate 必须先绿；仅 CI 失败意味着环境差异，值得在规则书
Traps 里记一行）。

---

## 自审记录

- 对照 spec 第 15 节的 SP0 项：参考拉取锁定（任务 1）、仓库骨架（任务
  2、11、12）、LICENSE 换 GPL-3.0（任务 2）、AGENTS.md（任务 12）、
  xtask gate 骨架（任务 3 至 10）、图标管线（任务 11）、CI 骨架（任务 13）、
  nightly 骨架（任务 13）、VERSION 与 branding（任务 2）、文档骨架与
  种子能力矩阵（任务 12）。全覆盖。
- 22:00 定时自动化本身在功能对标时排练并激活（spec 第 15 节末条）；现在
  创建 ZCode 调度会违反锁定期，故有意不进 SP0。
- 顺序：doc-sync 在任务 10 实现、任务 12 注册，因为计划文件自身的中文
  镜像随任务 12 落地。
