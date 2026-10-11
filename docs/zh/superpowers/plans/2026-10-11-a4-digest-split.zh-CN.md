# A4 Digest、分析、分箱收件箱实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-11-a4-digest-split.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 交付带生成服务与面板的 digest/报告文档、由已存分类派生的分箱
收件箱与列表开关、eval 夹具、门控活体验证。

**架构：** 两类文档宿主侧经 A1 缝生成、以已存分类做汇总；分箱映射从
ai_results 服务、零模型调用；CLI 读。

**技术栈：** 既有 workspace；所供端点作活体门。

**Spec：** `docs/superpowers/specs/2026-10-11-a4-digest-split-design.md`

## 全局约束

- 沿家规：SPDX、500 行上限、deny-everything lints、提交前 gate（直接退
  出码）、conventional commits、无破折号、英式英语、zh 镜像同提交、只允许
  增量线上变更。
- 连接环境（绝不提交）：同前序切片；活体代理经控制台请求转发。
- 安全线：邮件内容 untrusted 包裹；分箱映射是纯派生（零模型调用）；人发
  送规则不变。

---

### 任务 1：ai_documents 库
**文件：** `crates/astra-ai/src/migrations/0010_ai_documents.sql` + 库 API
+ 测试。
**接口：** 以（account、kind、period_start、period_end）为键的
insert-or-replace；latest-per-(account, kind) 读；CLI 用列表。
- [ ] 测试先行：往返；同期替换；kind 分离。
- [ ] 实现；提交 `feat: add the ai documents store`。

### 任务 2：服务
**文件：** `crates/astra-app/src/ai_docs/{mod.rs, digest.rs, digest_tests.rs,
report.rs, report_tests.rs, split.rs, split_tests.rs}`。
**接口：** `generate_digest(account, days, messages, stored_categories,
correlation, 缝) -> String`（汇总；每类头五；untrusted 包裹；
DIGEST_PROMPT_VERSION；DEFAULT）；`generate_report(account, days, messages,
stored_categories, sent_count, correlation, 缝) -> String`（先统计再一次结
构化调用；REPORT_PROMPT_VERSION）；`split_map(stored_categories) ->
Vec<(key, SplitSide)>` 加钉死映射常量 SPLIT_MAPPING_VERSION。
- [ ] 按 spec 单元清单测试先行。
- [ ] 实现；提交 `feat: add the digest, report, and split services`。

### 任务 3：eval 夹具
**文件：** 三个常量的 `evals/` 夹具；运行器注册。
- [ ] 测试先行：夹具 pin；scratch bump 失败。
- [ ] 实现；提交 `feat: pin the a4 evals`。

### 任务 4：宿主命令 + ipc
**文件：** `client/desktop/src/commands/ai.rs` 增（+测试）；
`client/web/src/services/ai.ts`（+测试）。
**接口：** `digest_generate(account, days?)`、`report_generate(account,
days?)`、`latest_document(account, kind)`、`split_map(account)`；线上钉死。
- [ ] 测试先行；提交 `feat: expose the a4 commands over ipc`。

### 任务 5：UI
**文件：** `client/web/src/features/ai-panel/{AiPanel.tsx + 测试}`
（digest/report tab）；消息列表分箱开关（+测试）。
- [ ] 测试先行：面板生命周期；开关过滤 + 徽标 + 无类别规则。
- [ ] 实现；提交 `feat: add the ai panel and the split inbox toggle`。

### 任务 6：CLI + 活体验证 + 文档
**文件：** `cli/src/verbs/ai.rs` 增 `digest`/`report` 读 + 拒绝的
`digest generate`/`report generate` 拼写 + 测试；
`client/desktop/src/ai/live_docs_tests.rs`（门控）；`docs/ai-digest.md` +
zh；capabilities + zh（digest/report/split partial；会议简报行点名依赖）；
fragments。
- [ ] CLI 测试先行；门控活体套件；文档；全量验证。
- [ ] 提交 `feat: prove a4 live and record the digest contract`。

### 任务 7：收尾
- [ ] 全量验证；推 `a4-digest-split` 分支；开 PR（预期 squash）；验收证
  据；清台账。

## 自审记录

- Spec 覆盖：D1（T1/T2/T4/T5/T6）、D2（T1/T2/T4/T5/T6）、D3（T2 split/
  T4/T5）、D4（T6 文档行）、eval（T3）、活体 + 文档（T6）。
- 接口链：库（T1）-> 服务（T2）-> eval（T3）-> ipc（T4）-> UI（T5）->
  CLI + 活体 + 文档（T6）。
- 风险：窗口读的成本（最坏 31 天——上限钉死）；分箱映射的拉取形态（增
  量；随快照旁）；面板放置选择记录。
