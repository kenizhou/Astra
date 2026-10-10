# A2 回复起草实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-10-a2-reply-drafting.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 交付带浮现门与智能回复芯片的草稿池、带缓存的摘要动作、逐账户
风格档案、宿主命令与 CLI 动词、门控活体验证。

**架构：** 生成/摘要/infer 宿主侧经 A1 客户端缝运行；astra-app 持池、缓
存、档案与 OpenMessage 武装；阅读窗消费芯片与摘要；撰写器接受芯片作为
预填回复。

**技术栈：** 既有 workspace；所供 DeepSeek 端点作活体门。

**Spec：** `docs/superpowers/specs/2026-10-10-a2-reply-drafting-design.md`

## 全局约束

- 沿家规：SPDX、500 行上限、deny-everything lints、提交前 gate（直接
  退出码）、conventional commits、无破折号、英式英语、zh 镜像同提交、
  只允许增量线上变更。
- 连接环境（绝不提交）：同 A1。
- 安全线：每个 prompt 中邮件内容 untrusted 包裹；风格注记按数据；芯片绝不
  自动发送任何内容；人发送规则不变。

---

### 任务 1：库增长与草稿服务
**文件：** `crates/astra-ai/src/migrations/{0004_draft_pool.sql,
0005_summarise_cache.sql}` + 库 API + 测试；
`crates/astra-app/src/ai_replies/{mod.rs, draft.rs, draft_tests.rs}`；
新增 `LlmUseCase::StyleInfer`（ECONOMY）。
**接口：** 池行 CRUD + 按消息查询；缓存 get/put 键为（消息、正文哈希）；
`generate_reply(context..., correlation, AiComplete 缝) -> DraftPoolEntry`
（经修复的结构化 `{reply, confidence}`；`DRAFT_REPLY_PROMPT_VERSION`）；
上下文收集器（原信 + 封顶线程近史 + 作为数据的风格注记）；失败 ->
skipped 行。
- [ ] 测试先行：池往返；缓存命中/未中/哈希失效；生成形态 + 修复 + 失败；
  收集器封顶与包裹。
- [ ] 实现；提交 `feat: add the draft pool and reply generation`。

### 任务 2：门、芯片数据、摘要、风格
**文件：** `crates/astra-app/src/ai_replies/{gate.rs, gate_tests.rs,
summarise.rs, summarise_tests.rs, style.rs, style_tests.rs}`。
**接口：** `chips_for(message, pool, judge 缝) -> Vec<Chip>`（置信度常量 +
JudgeDraftSurface Yes；前两枚修剪；参考忽略标记）；带缓存的
`summarise(message, 缝, correlation)`；`style_profile(account, 缝,
correlation)`（二十封上限、无 Sent 无档案、built_at 每日上限、
`ai style rebuild` 强制）。
- [ ] 按简报单元清单测试先行。
- [ ] 实现；提交
  `feat: add the chips gate, cached summarise, and style profiles`。

### 任务 3：OpenMessage 武装与宿主命令
**文件：** `crates/astra-app/src/{reading.rs（武装挂钩）,
ai_replies/drive.rs}` + 测试；`client/desktop/src/ai/seams.rs` 增武装；
`client/desktop/src/commands/ai.rs`（+测试）；
`client/web/src/services/{ai.ts, ai.test.ts}`；surfaceEvents 或专用事件。
**接口：** OpenMessage 武装生成（spawn、池在即去重的一次一消息、携带
阅读 correlation）；宿主命令 `draft_chips`、`summarise`、
`style_profile`；池行落地发 `draft_pool` 事件；ipc.ts 类型 + 线上钉死。
- [ ] 测试先行：武装（一次一消息；失败跳过）；命令往返；双侧线。
- [ ] 实现；提交
  `feat: arm reply drafting on open and expose the ai commands`。

### 任务 4：阅读窗芯片与摘要 UI
**文件：** `client/web/src/components/reading-pane/{SmartReplies.tsx,
SummaryBlock.tsx}` + 测试；ReadingPane 接线；撰写器接受路径（派发 SP2f
Compose::Reply 后以草稿文档水合——诚实缝：新宿主命令
`compose_from_draft` 或既有 ComposeRequest 加草稿覆盖；择一记录）。
- [ ] 按 D3/D4 测试先行（渲染、接受派发、忽略、摘要各态、可折叠块）。
- [ ] 实现；提交 `feat: add smart reply chips and the summarise block`。

### 任务 5：CLI 动词
**文件：** `cli/src/verbs/ai.rs` 增 `drafts`、`summary`、
`style rebuild` + 测试；docs/cli.md 行（+zh）。
- [ ] 测试先行：从库读；rebuild 的 preview；审计行。
- [ ] 实现；提交 `feat: add the ai replies verbs to the cli`。

### 任务 6：活体验证与文档
**文件：** `client/desktop/src/ai/live_replies_tests.rs`（门控）；
`docs/ai-replies.md`（+zh）；capabilities + zh；fragments。
- [ ] 按 D3 活体清单的门控套件（草稿、门、摘要缓存第二次调用、Sent 风
  格、芯片端到端）；离线全量；文档。
- [ ] 提交 `feat: prove a2 live and record the replies contract`。

### 任务 7：收尾
- [ ] 全量验证；推 `a2-reply-drafting` 分支；开 PR（预期 squash）链接
  spec；验收证据；清台账。

## 自审记录

- Spec 覆盖：D1（T1）、D2/D3 数据（T2）、D4（T2）、D5（T2）、D6（T3/T5）、
  UI（T4）、活体 + 文档（T6）。
- 接口链：池/缓存/档案（T1）-> 门/摘要/风格（T2）-> 武装 + 命令（T3）
  -> UI（T4）-> CLI（T5）-> 活体（T6）。
- 风险：芯片接受进入 SP2f 回复流的缝（T4 择一记录）；锁定 rev 上的线程
  近史读（T1 先读）；风格每日上限的时钟（built_at 比较；无后台定时器——
  仅按需驱动，记录）。
