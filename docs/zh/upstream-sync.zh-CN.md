> 非规范性译本，仅供审阅；以英文版为准。

# 上游同步契约

Astra 如何跟踪其四个参考项目。规则手册（AGENTS.md）链接到本文档；
本文档是同步流程得到描述的唯一场所。

## 锁定表

`refs/refs-lock.toml` 按参考项目记录：`url`、`path`（相对于
`root`）、`baseline`（引导时锁定的提交）、`last_ported`（随移植落地
而推进）以及 `locked_at`。

## 锁定阶段

在达到功能对等之前，参考项目冻结在其基线：docs/capabilities.md 中的
每一个 mail-calendar 行均已完成或明确标记为 N/A，AI 阶段 A1 至 A4 已
完成，且插件宿主已就位。在此之前，不将任何参考项目向前拉入 Astra。

## 22:00 任务

一个本地定时自动化任务于每晚 22:00 运行，幂等，以
`last_ported` 为锚点：

1. 拉取全部四个参考项目；计算 `last_ported..HEAD` 的差异；差异为空时
   跳过。
2. 对变更分类：仅文档、仅测试、重构、行为、新功能、
   安全。
3. 按规则手册移植。契约文档、能力矩阵行、变更日志片段，
   以及任何文档的中文审阅副本，随同一个变更
   落地。
4. 安全类变更始终留待审阅，绝不自动合并。
5. 运行快速本地门禁子集；完整门禁在 CI 中运行。
6. 开启一个附带各仓库摘要的 PR。仅对带有绿色门禁的低风险类别
   自动合并；其余一切等待早晨审阅并附带摘要。
7. 合并时，推进 `last_ported`。
8. UI 风格决策与架构冲突在 PR 中列出，
   由作者决定。禁止静默偏离。

## 引擎更新

引擎以完整上游树 vendored 于 `vendor/engine/`（钉在锁定表记录的版本
上，MPL-2.0），因此 Astra 对引擎的修改以 main 上的普通 commit 落地，
上游变化以可审阅的合并到来，取代手工重新钉定。循环：

- **`vendor-sync` 是原样分支。** 它始终持有上游在上次同步版本上的
  原样树；`main` 在其上叠加 Astra 的树内修改。本仓库**之外**的常驻
  worktree（`D:/Projects/mailclient/Astra-vendor-sync`，gate 永不走
  查）是每晚脚本的工作场所。
- **每晚（22:00 任务）。** `scripts/dev/upstream-sync.sh` 拉取上游，
  从 `vendor-sync` 最近一条 `vendor: upstream <rev>` 提交信息读出
  当前引入版本（绝不只取 `last_ported`，因此同步 PR 挂几晚未合并既
  不会重发旧 digest 也不会跳空版本），相等即空转。否则把新版本
  `git archive` 进 worktree，提交 `vendor: upstream <rev> <date>`，
  推送，并开（或更新）`vendor-sync -> main` 的 PR，正文带版本区间的
  分类 digest：docs only、tests only、ci-docker、dependencies
  （`Cargo.toml`、`Cargo.lock`）、behaviour、security。dependencies
  桶显式点名 rusqlite/libsqlite3-sys：两家存储共享一个 libsqlite3-sys，
  上游 bump rusqlite 可能在同一次构建中打坏 Astra。
- **同步 PR 一律用 merge commit 合并，绝不 squash。** squash 意味着分
  支尖端永远不是 main 的祖先，此后每次同步都会重新全量 diff，冲突显
  形机制随之失效。（备选 —— 每次 squash 后在 `vendor-sync` 上
  `git merge -s ours main` —— 已记录但非规则。）
- **冲突在 PR 中显形，绝不静默。** 当上游改动触及 Astra 在树内修改过
  的行时，同步 PR 的合并会显示冲突。runbook：`git checkout vendor-sync
  && git merge main`，在 `vendor/engine` 的冲突中保留 Astra 的修改，
  推送，PR 随之更新。`git diff vendor-sync main -- vendor/engine` 永
  远给出全部树内修改的清单，因此静默偏离上游在结构上不可能。
- **合并时推进 `last_ported`。** 锁定表的引擎行继续作为溯源记录。
- **引擎自身电池不属于 `cargo test --workspace`。** vendored 树是被
  排除于成员之外的嵌套 workspace，其测试只显式运行：`cd vendor/engine
  && cargo test --workspace`（live-server 测试无环境变量时自动跳过）。
  同步 PR 的审查清单运行它、Astra 电池、xtask gate 与 live harness。
- **构建目录。** 本仓库在检出之间共享一个构建目录，新鲜度按工作区相
  对路径判定。任何 `vendor/` 与其他检出分叉的 worktree（首推
  vendor-sync worktree）必须把 `CARGO_BUILD_BUILD_DIR` 设为自己的目
  录，否则构建可能吃到陈旧引擎产物。
- security 分类的同步一律扣留审阅，绝不自动合并，与 22:00 契约对每个
  参考项目的规则一致。

### 22:00 接线

一个 Windows Task Scheduler 任务每晚运行该驱动，日志写在仓库**之外**
（`D:/Projects/mailclient/logs/vendor-sync.log`）：

```bash
schtasks /Create /F /TN "Astra\vendor-upstream-sync" /SC DAILY /ST 22:00 \
  /TR "\"C:\Program Files\Git\bin\bash.exe\" -lc 'D:/Projects/mailclient/Astra/scripts/dev/upstream-sync.sh >> D:/Projects/mailclient/logs/vendor-sync.log 2>&1'"
```

`upstream-sync.sh --dry-run` 打印当前与新版本及分类 digest 而不改动任
何状态，是在放行同步前观察它的方式。任何失败（文件锁、网络、`gh`
auth、原样性证明失败）以非零退出进日志，成为晨审事项，绝不静默跳
过。
