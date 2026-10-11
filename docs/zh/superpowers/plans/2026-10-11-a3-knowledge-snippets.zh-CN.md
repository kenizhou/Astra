# A3 知识、Snippets、自动补全、翻译实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-11-a3-knowledge-snippets.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 交付带批提取与草稿塑形的知识库、带 AI 搜索与撰写器选择器的
snippet 库、带去抖芯片的自动补全建议、带缓存的翻译动作、宿主命令与
CLI 动词、eval 夹具、门控活体验证。

**架构：** 提取/搜索/自动补全/翻译宿主侧经 A1 客户端缝运行；astra-app 持
库与服务；撰写器与阅读窗消费；CLI 读与管理 CRUD。

**技术栈：** 既有 workspace；所供 DeepSeek 端点作活体门。

**Spec：** `docs/superpowers/specs/2026-10-11-a3-knowledge-snippets-design.md`

## 全局约束

- 沿家规：SPDX、500 行上限、deny-everything lints、提交前 gate（直接
  退出码）、conventional commits、无破折号、英式英语、zh 镜像同提交、
  只允许增量线上变更。
- 连接环境（绝不提交）：同 A1/A2。
- 安全线：每个 prompt 中邮件内容 untrusted 包裹；知识条目是信任侧的模型
  派生数据；自动补全绝无手势不插入；人发送规则不变。

---

### 任务 1：库与用例
**文件：** `crates/astra-ai/src/migrations/{0007_knowledge.sql,
0008_snippets.sql,0009_text_cache.sql}` + 库 API + 测试；新增
`LlmUseCase::{KnowledgeExtract, SnippetSearch, Autocomplete, Translate}`
（ECONOMY、ECONOMY、CHAT、DEFAULT）。
**接口：** 知识条目 CRUD（按账户列、带去重守卫插入、按 id forget）；
snippets CRUD（最新先列、建、删、更新触时间）；文本缓存键（账户、键、
kind、可空 lang、正文哈希）带 get/put。
- [ ] 测试先行：各库往返；去重守卫；缓存的 kind/lang 键。
- [ ] 实现；提交 `feat: add the knowledge, snippet, and text cache stores`。

### 任务 2：服务
**文件：** `crates/astra-app/src/ai_assist/{mod.rs, knowledge.rs,
knowledge_tests.rs, snippets.rs, snippets_tests.rs, autocomplete.rs,
autocomplete_tests.rs, translate.rs, translate_tests.rs}`。
**接口：** `extract_knowledge(messages, account, correlation, 缝) ->
Vec<KnowledgeEntry>`（结构化；来源追踪；30 封上限；插入时去重）；
`knowledge_context(account) -> Vec<String>`（草稿 prompt 的十条 kind 序
封顶）；`search_snippets(query, snippets, correlation, 缝) -> Vec<Snippet>`
（一次排名调用、子串回退）；`autocomplete(before, after, correlation,
缝) -> String`（版本化 prompt；CHAT）；`translate(text, lang,
correlation, 缝, cache)`（缓存；DEFAULT）。版本常量：KNOWLEDGE_EXTRACT、
SNIPPET_SEARCH、AUTOCOMPLETE、TRANSLATE。
- [ ] 按 spec 单元清单测试先行（形态、封顶、回退、缓存无调用）。
- [ ] 实现；提交
  `feat: add the knowledge, snippet, autocomplete, and translate services`。

### 任务 3：草稿塑形 + eval 夹具
**文件：** `crates/astra-app/src/ai_replies/draft.rs`（知识上下文槽；
DRAFT_REPLY_PROMPT_VERSION bump 至 "2"）；四个新常量 + 草稿 bump 的
`evals/` 夹具；eval 运行器注册。
- [ ] 测试先行：知识条目作为数据骑信任侧（钉死）；草稿夹具版本 pin 移至
  "2"；四个新夹具 pin 各自常量。
- [ ] 实现；提交 `feat: shape drafts with knowledge and pin the a3 evals`。

### 任务 4：宿主命令与 ipc
**文件：** `client/desktop/src/commands/ai.rs` 增（+同级测试）；
`client/web/src/services/{ai.ts, ai.test.ts}`；必要的事件（保持简单：命令
完成后返回；UI 显示 spinner）。
**接口：** knowledge_list/knowledge_extract/knowledge_forget、
snippet_list/snippet_search、compose_autocomplete、translate——双侧线上
钉死。
- [ ] 测试先行：假缝下命令往返；线上字面量。
- [ ] 实现；提交 `feat: expose the a3 commands over ipc`。

### 任务 5：UI
**文件：** `client/web/src/features/composer/{SnippetPicker.tsx,
AutocompleteChip.tsx}` + 测试；
`client/web/src/components/reading-pane/TranslateBlock.tsx` + 测试；接线。
- [ ] 测试先行：选择器（列表/搜索/光标处插入）、芯片（去抖/忽略/仅手势
  插入）、翻译块（语言选择/缓存/新取/错误）。
- [ ] 实现；提交
  `feat: add the snippet picker, autocomplete chip, and translate block`。

### 任务 6：CLI 动词 + 活体验证 + 文档
**文件：** `cli/src/verbs/{knowledge.rs, snippets.rs}` + 测试（读取 + 带
--preview 的 CRUD；提取/搜索/自动补全/翻译绝不从 CLI）；
`client/desktop/src/ai/live_assist_tests.rs`（门控）；
`docs/ai-assist.md` + zh；capabilities + zh；fragments。
- [ ] CLI 测试先行；按 spec 活体清单的门控套件；文档；全量验证。
- [ ] 提交 `feat: prove a3 live and record the assist contract`。

### 任务 7：收尾
- [ ] 全量验证；推 `a3-knowledge-snippets` 分支；开 PR（预期 squash）链
  接 spec；验收证据；清台账。

## 自审记录

- Spec 覆盖：D1（T1/T2/T3）、D2（T1/T2/T5/T6）、D3（T2/T4/T5）、D4
  （T1/T2/T4/T5）、D5（T4/T6）、eval（T3）、活体 + 文档（T6）。
- 接口链：库（T1）-> 服务（T2）-> 草稿塑形 + eval（T3）-> ipc（T4）->
  UI（T5）-> CLI + 活体 + 文档（T6）。
- 风险：草稿 prompt bump 是真实 prompt 变更（按 AI 规则，eval 重跑必须同
  变更落地）；jsdom 的自动补全去抖（假计时器）；TipTap 光标插入缝（在当
  前选区插入；核对锁定版本的编辑器 API）。
