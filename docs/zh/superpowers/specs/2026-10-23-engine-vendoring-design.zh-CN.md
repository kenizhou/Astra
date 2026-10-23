# 引擎引入设计：同步引擎进入树内（vendor/engine/）

**状态。** 2026-10-23 已定稿。从 SP4b 尖端切出 `engine-vendoring` 分支
（SP4b 携带本切换要改写的 `engine-rfc5322` 依赖行）。用户决策
2026-10-23：引入引擎的**完整树**（全部 22 个 crate 加 fuzz/、docker/、
tools/、docs/、scripts/ 与许可文件，除 `.git` 与构建产物外的一切）；每
晚增量同步由本机定时任务执行（docs/upstream-sync.md 已记录的 22:00 本
机契约，不用 GitHub Actions cron）；Astra 对引擎的修改**直接落在树
内**，由 `vendor-sync` 合并分支持有上游原样副本，使每晚同步的冲突在 PR
中显形，绝不被静默丢弃。

**切换的理由。** 今天引擎以 workspace 根 `Cargo.toml` 中 10 条 git 钉定
依赖到达（钉在 `68eab00e6a9a9fe5a200218e156dbaae1ba00caa`，另有 8 个
crate 经传递依赖进入）。钉定模型挡住了路线图上的两件事：不做 fork 的
Astra 侧引擎修改（SP4b 加密发件箱已把缺失的 raw-MIME 提交接缝记为钉定
引擎无法弥补的已知缺口），以及用活的增量同步取代手工重新钉定。按钉定
版本引入使切换行为中性；`vendor-sync` 分支与 22:00 任务使此后每一次上
游变化都成为可审阅的合并 PR。`refs/refs-lock.toml` 的引擎行继续作为溯
源记录（baseline = `last_ported` = 68eab00）。

**范围。** 四个 PR：按 68eab00 引入（PR 1）、同步工具与 dry-run
（PR 2）、经该机制的首次真实同步（PR 3）、定时任务（PR 4）。不在范围
内：改变 Astra 消费引擎的方式（`engine-api` 唯一之门不变）、把任何
Astra 代码移入 `vendor/`、把引擎自身电池并入 Astra 的 workspace 电池
（见 D6）。

## D1：vendor/engine/ 全量镜像上游树

vendored 树是上游在引入版本上的 `git archive`，解出到
`vendor/engine/`：全部 22 个 crate（`engine-*`、`provider-*`、
`store-sqlite`、`stalwart-harness`、`mailbox-fixture`、`dav-cli`、
`engine-cli`），外加 `fuzz/`（自带的 nightly-only workspace，惰性）、
`docker/`、`tools/`、`scripts/`、`docs/`、上游根 manifest、许可文件、
CI 文件与点文件溯源。任何 Astra 自己的东西绝不进 `vendor/`：所有胶水
（path 依赖、豁免、同步脚本）都在目录外，因此同步 PR 的 diff 永远只含
vendor 变更。

复制一律用 `git -C <upstream> archive <rev> | tar -x -C vendor/engine`
（恰好是该版本的被跟踪文件；不带 `.git` 与未跟踪垃圾），绝不用文件系
统复制。`.github/` 与 `Cargo.lock` 作为惰性溯源一并带来（只有在
`vendor/engine` 目录内构建时才起作用）。

## D2：嵌套 workspace 是承重的（exclude + 上游 manifest）

两条已验证的 Cargo 事实塑造一切：

- Cargo 会把 workspace 目录内的 path 依赖自动收编为成员。vendored 树
  自己是一个 workspace（其根 `Cargo.toml` 有 `[workspace] members =
  ["crates/*"]`），而一个成员只能属于一个 workspace。因此 Astra 根的
  `[workspace]` 必须声明 **`exclude = ["vendor"]`**，否则整个 workspace
  无法解析。
- `version.workspace = true`、`edition.workspace = true` 与
  `[lints] workspace = true` 一律向包自身 manifest 向上找到的第一个
  workspace 根解析。引入上游根 manifest 后，引擎 crate 继续继承自己的
  lint 表、edition 2024 与 MPL-2.0 许可元数据；Astra 的更严 lint 表
  （pedantic、`missing_docs = "deny"`）只作用于 Astra 成员。因此引入
  上游根 manifest 是双重必需，绝非可选。

上游的 `rust-toolchain.toml`（1.97.0）只管辖 cwd 位于 `vendor/engine`
内的命令（rustup 按 cwd 解析）；Astra 构建仍用本仓库钉定的工具链，与
今天 git-dep 构建完全同条件。

## D3：依赖切换是 10 条 git 行变 10 条 path 行

workspace 根的 `[workspace.dependencies]` 把 10 条 git 钉定行换成指向
vendored 树的 path 行，feature 保留（`engine-tls` 保留
`tls-native-certs`）。各成员 crate 的 `engine-x.workspace = true` 行不
动。`Cargo.lock` 重新生成（18 条 `source = "git+...allodia-eu..."` 消
失）。`.cargo/config.toml` 中注释态的 `[patch]` 块删除：path 依赖不需
要 patch（且该块本就漏了 `engine-rfc5322`）；其头部注释改写为：树内
引擎开发即在 `vendor/engine` 中编辑，vendor-sync 流程保证溯源。

xtask gate 在上游内容会触发处增加 `vendor/` 豁免：`license_headers`
（VENDORED_PREFIXES，适用其自身许可）、`file_length`
（UPSTREAM_EXEMPT_PREFIXES，与上游保持可逐字节 diff）、`british_english`
与 `dash_hygiene`（上游文档与 CI 文件）。`doc_sync` 无需改动（其范围是
AGENTS.md、`docs/*.md` 与 `docs/superpowers/**`；vendored 文档不欠中文
镜像）。`paths_ascii` 继续扫描 `vendor/`（白给的持续保证）。

## D4：vendor-sync 分支与每晚的合并 PR

`vendor-sync` 分支永远持有上游在上次同步版本上的原样副本；`main` 在其
上叠加 Astra 的树内修改。仓库**之外**的常驻 worktree
（`D:/Projects/mailclient/Astra-vendor-sync`，gate 永不走查）是每晚脚
本的工作场所。

`scripts/dev/upstream-sync.sh`（bash，幂等，支持 `--dry-run`）执行循
环：拉取上游；从 `vendor-sync` 最近一条 `vendor: upstream <rev>` 提交
信息读出当前引入版本（兜底 refs-lock 的 baseline；PR 1 的引入提交同样
携带该格式）；相等即空转；否则把新版本 `git archive` 进 worktree 的
`vendor/engine`，提交 `vendor: upstream <rev> <date>`，推送，并开（或
更新）`vendor-sync -> main` 的 PR，正文带版本区间的分类 digest：
docs-only / tests-only / ci-docker / dependencies（`Cargo.toml`、
`Cargo.lock` —— rusqlite 盯防项）/ behaviour / security 关键词命中。
security 类必在 PR 正文标出且绝不自动合并。脚本绝不把 main 合进
vendor-sync、绝不 force push；任何失败以非零退出进日志，留给晨审。

**同步 PR 一律用 merge commit 合并，绝不 squash**：squash 意味着分支尖
端永远不是 main 的祖先，此后每次同步都会重新全量 diff，冲突显形机制
随之失效。（备选方案 —— 每次合并后在 `vendor-sync` 上 `git merge -s
ours main` 重新锚定 merge-base —— 已记录但不采用。）

锚点取 `vendor-sync` 自身历史（而非 `last_ported`），因此同步 PR 挂几
晚未合并既不会重发旧 digest 也不会跳空版本；`last_ported` 在 PR 合并
时于 main 上推进。

## D5：Astra 自己的修改留在树内，冲突在 PR 中显形

对 `vendor/engine` 的本地修改以 main 上的普通 commit 落地（本切换存在
的意义就是这条树内修改流程）。当每晚同步的合并触及相同行时，
`vendor-sync -> main` 的 PR 会显示冲突；runbook 手工解决（`git
checkout vendor-sync && git merge main`，在 `vendor/engine` 的冲突中保
留 Astra 的修改，推送），PR 随之更新。静默偏离在结构上不可能：一句
`git diff vendor-sync main -- vendor/engine` 永远给出 Astra 对引擎全部
修改的完整清单。

## D6：引擎自身电池在 vendor/engine 内运行

有了 `exclude = ["vendor"]`，引擎 crate 是非成员 path 依赖，其单元测
试**不会**并入 Astra 的 `cargo test --workspace`（cms 先例会跑只因为
它是意外成员）。同步 PR 的验证在 `vendor/engine` 内显式运行引擎电
池：嵌套 workspace 独立成立，live-server 测试无环境变量时自动跳过。
Astra 电池保持只含成员。

## D7：验证协议

PR 1：(1) 以 git blob hash 做原样性证明（`git ls-files -s
vendor/engine` 去前缀排序后与 `git ls-tree -r <rev>` 一致 —— 绝不比对
工作区字节，因为 autocrlf 只对部分扩展名钉 LF）；(2) 全量 workspace
电池绿；(3) xtask gate 绿；(4) `cargo tree` 与 `Cargo.lock` 无
`allodia-eu` 行；(5) `cargo metadata --no-deps` 的 `workspace_members`
中无任何引擎 crate（cms 仍在，证明 exclude 未误伤）；(6) 嵌套
workspace 在 `vendor/engine` 内独立编译；(7) live harness 串行通过；
(8) `tls-native-certs` feature 保留。

PR 2：`--dry-run` 打印当前与新版本及分类 digest；`gh auth status` 通
过；外部 worktree 存在。

PR 3：同步 PR 的 diff 只含 `vendor/engine`；合并前检查（全量电池、
gate、harness、`vendor/engine` 内引擎电池）在指向独立目录的
`CARGO_BUILD_BUILD_DIR` 下运行（共享 build-dir 的新鲜度陷阱：vendor 分
叉的 worktree 绝不可共享默认构建目录）。

PR 4：定时命令行手动原样跑一遍，空转路径 exit 0。

## 已记录的陷阱，不隐藏

- T1：`exclude = ["vendor"]` 必需；没有它 Cargo 的自动收编与嵌套
  workspace 冲突，一切无法解析。
- T2：引擎测试不并入 Astra 电池；在同步 PR 流程中于 `vendor/engine`
  内运行，并在 upstream-sync.md 写明。
- T3：本仓库共享 build-dir 的新鲜度按 workspace 相对路径判定；vendor
  分叉的 worktree 必须设 `CARGO_BUILD_BUILD_DIR` 为自己的目录，否则可
  能吃到陈旧引擎产物。
- T4：同步 PR 用 merge commit 合并（见 D4）；squash 会破坏增量模型。
- T5：`british_english` 与 `dash_hygiene` 也需要 vendor 豁免；
  `doc_sync` 不需要。
- T6：CLAUDE.md 没有中文镜像；只有 AGENTS.md 与 `docs/*.md` 的修改欠
  镜像。
- T7：复制一律 `git archive`；`.github/` 与 `Cargo.lock` 作为惰性溯源
  保留。
- T8：原样性证明比对 git blob hash，绝不比对工作区字节（autocrlf 只
  对部分扩展名钉 LF）。
- T9：rusqlite 耦合（两家共享一个 libsqlite3-sys，钉 0.40）变为同仓
  库：上游 bump rusqlite 会经同步 PR 到达并可能同构建内打坏 astra-ai
  —— digest 把 `Cargo.toml`/`Cargo.lock` 单独分类，审查清单点名。
- T10：每晚锚点取 `vendor-sync` 自身的 `vendor:` 提交历史（兜底
  refs-lock baseline），绝不只取 `last_ported`。
- T11：`gh` 已装但 auth 在 PR 2 验证，不假定。
- T12：每晚 worktree 放在仓库外（`D:/Projects/mailclient/Astra-vendor-sync`），
  gate 永不走查。
