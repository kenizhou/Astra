# 引擎引入计划：vendor/engine/ 与 vendor-sync 流程

规格：docs/superpowers/specs/2026-10-23-engine-vendoring-design.md。从
SP4b 尖端切出 `engine-vendoring` 分支。每个 PR：行为变化处先写红灯
（xtask 豁免的夹具测试）、跑电池、跑 gate、按约定式提交。

### PR 1：按 68eab00 的 vendor 切换

**新文件：** `vendor/engine/**`（`git -C <upstream> archive
68eab00e6a9a9fe5a200218e156dbaae1ba00caa | tar -x -C vendor/engine`）；
`docs/changelog/unreleased/engine-vendoring.md`（Platforms/Bump 标签）。

**修改文件：** `Cargo.toml`（`[workspace]` 加 `exclude = ["vendor"]`；
10 条 git 行变 10 条 path 行，feature 保留；区段注释指向
docs/upstream-sync.md）；四个 xtask 检查（`license_headers.rs`、
`file_length.rs`、`british_english.rs`、`dash_hygiene.rs`，各加
`vendor/` 豁免与一个夹具测试，可在 `checks/mod.rs` 提升共享常量
`VENDORED_ROOT`）；`.cargo/config.toml`（删除注释态 `[patch]` 块、改写
头部注释）；`Cargo.lock`（重新生成）；`AGENTS.md` + 中文镜像；
`CLAUDE.md`；`docs/upstream-sync.md` + 中文镜像（重写 Engine updates
一节：vendored 模型、vendor-sync 分支、merge commit 规则、last_ported
推进、引擎电池在 `vendor/engine` 内运行、CARGO_BUILD_BUILD_DIR 规则）。

**提交信息：** 正文携带字面行 `vendor: upstream
68eab00e6a9a9fe5a200218e156dbaae1ba00caa initial import`（每晚锚点格式）。

- [ ] D7 验证协议（全部八项）；推送；PR `engine: vendor the sync engine at 68eab00`。

### PR 2：同步工具与 dry-run

**文件：** `scripts/dev/upstream-sync.sh`（幂等、`--dry-run`、digest 分
类器、gh PR 创建或更新）；`docs/upstream-sync.md` + 中文镜像（调度小
节与冲突 runbook）。分支与 worktree：从 main 切 `vendor-sync` 并推
送；`git worktree add D:/Projects/mailclient/Astra-vendor-sync vendor-sync`。

- [ ] `--dry-run` 打印 CUR=68eab00、NEW=73650cc 与 digest；`gh auth
  status` 通过。提交 `feat: the engine upstream-sync driver`。

### PR 3：首次真实同步（68eab00 -> 73650cc）

**文件：** 脚本自身的产出（`vendor-sync -> main` PR）；
`refs/refs-lock.toml`（`last_ported` 推进）；上游行为变更的 changelog
fragment；13 个上游 commit 所需的 Astra 侧适配（以 main 上的普通
commit 落地）。

- [ ] 同步 PR 的 diff 只含 `vendor/engine`；合并前电池 + gate +
  harness + 引擎电池（私有 CARGO_BUILD_BUILD_DIR）；用 merge commit 合
  并；推进 `last_ported`。

### PR 4：定时任务

**命令：** `schtasks /Create /F /TN "Astra\vendor-upstream-sync"
/SC DAILY /ST 22:00 /TR "\"C:\Program Files\Git\bin\bash.exe\" -lc
'D:/Projects/mailclient/Astra/scripts/dev/upstream-sync.sh >>
D:/Projects/mailclient/logs/vendor-sync.log 2>&1'"`（日志在仓库外）。

- [ ] `schtasks /Query` 可见；定时命令行手动原样跑一遍，空转路径
  exit 0。

## 自审记录

- 规格覆盖：每个 D 恰好落入一个 PR（D1-D3 -> PR 1，D4/D5 -> PR 2，
  D4 的首次执行 -> PR 3，调度 -> PR 4，D6/D7 -> 各 PR 的验证步骤）。
- 接口链：vendored 树（PR 1）是脚本驱动对象（PR 2），首次合并证明闭
  环（PR 3），定时任务维持其活性（PR 4）。
- 风险：exclude 行与 path 行同提交落地（绝不分家）；引入提交携带锚点
  格式；同步 PR 用 merge commit；原样性证明比对 blob hash 而非字节。
