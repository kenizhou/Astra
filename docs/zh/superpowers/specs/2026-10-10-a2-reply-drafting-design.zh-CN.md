# A2 回复起草与摘要设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-10-a2-reply-drafting-design.md`）。

日期：2026-10-10
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 9.3 节）、
`docs/superpowers/specs/2026-10-10-binary-judge-design.md`（JudgeDraftSurface）
消费：A1 骨架（客户端、裁决器、缝、台账）、SP2c 阅读管线、SP2d/SP2f 撰写
器与草稿。

## 1. 目的

阅读体验获得它的 AI 层：后台回复草稿以智能回复芯片浮现、带缓存的摘要动
作、塑造草稿的逐账户写作风格档案。接受芯片即以草稿预填打开撰写器作回
复；人仍然决定与发送。

## 2. 决策

- **D1（草稿生成）**：打开消息（OpenMessage）武装一次后台 DraftReply 生
  成（DRAFT 档、`DRAFT_REPLY_PROMPT_VERSION`），上下文收集器：原信加线程
  近史（引擎线程读、封顶）、有则逐账户风格档案、包裹 untrusted 的邮件内
  容。结构化输出携带 `{reply, confidence}`（inbox-zero 形态）；结果入草稿
  池（迁移 `0004_draft_pool`：消息键、回复、置信度、created_at、
  correlation id）。生成失败记 skipped 行；绝不打扰阅读 surface。
- **D2（浮现门）**：池条目仅当置信度过常量且 JudgeDraftSurface 答 Yes
  （nano 档；问题问这份草稿是否用户真可发送的有用回复、而非凑数）才浮
  现为芯片。Abstain 默认 No 保持芯片隐藏（安静失败、无所失：池行仍可经
  CLI 查询）。
- **D3（智能回复芯片）**：阅读窗显示至多两枚过门芯片（短预览、修剪）。
  接受即以草稿文本为文档体派发 SP2f 回复流（撰写器作为回复打开、人编辑
  并照常经 durable outbox 发送）。忽略为该消息隐藏芯片
  （`chips_dismissed` 标记按消息记）。
- **D4（摘要）**：`summarise(message)` 跑 Summarise 用例（DEFAULT 档、
  `SUMMARISE_PROMPT_VERSION`），untrusted 包裹正文；结果缓存在迁移
  `0005_summarise_cache`，按消息键 + 正文哈希（变更即失效）；缓存命中无
  调用返回。阅读窗增 Summarise 按钮，在可折叠块中渲染缓存或新取摘要。
- **D5（风格档案）**`StyleInfer`（新用例、ECONOMY 档）从至多二十封近
  期已发送邮件（引擎 Sent 文件夹读；结构化调用返回简短描述：语气、正式
  度、落款习惯）构建逐账户风格注记。档案缓存在 `0004` 的姊妹表
  `style_profiles`（账户 PK、注记、built_at），仅按需重建（CLI 动词
  `ai style rebuild` 与启动时至多每日一次刷新）。无已发送邮件则无档案：
  草稿 prompt 省略注记。infer prompt 中的邮件内容 untrusted 包裹；注记本
  身是模型派生文本，在草稿 prompt 中按数据处理。
- **D6（线）**：宿主命令 `draft_chips(account, key)`、`summarise(account,
  key)`、`style_profile(account)`；池行落地时发 `draft_pool` 刷新事件
  （阅读窗打开时与事件时拉取芯片）。CLI 增读动词 `ai drafts <account>
  <key...>` 与 `ai summary <account> <key...>` 加 `ai style rebuild`（唯
  一变更动词，`--preview` 描述重建）。

## 3. 测试

- 单元（astra-app，假件 + 夹具缝）：生成形态（结构化
  `{reply, confidence}`、修复路径、失败 -> skipped 行）；门（置信度阈、
  裁决 Yes/No/abstain 默认）；芯片数据（前两枚、修剪、忽略标记）；摘要
  缓存（命中/未中/哈希失效）；风格 infer（形态、无 Sent 无档案、每日上
  限）；correlation 纪律（每次生成一个 id；台账行）。
- 夹具服务器：DRAFT 与 DEFAULT 档线形态。
- 前端（vitest）：芯片从拉取渲染；接受以草稿文档派发回复流；忽略标记；
  摘要按钮（加载、缓存即回、新取、错误面）；可折叠块。
- 活体（门控，所供端点 + harness）：为种子富邮件生成真实草稿（池行 +
  台账成本）；真实裁决门决定；真实摘要（缓存第二次调用断言无第二条台账
  行）；对 astra1 的 Sent 做风格 infer（邮件套件的发送留有副本）；经宿主
  命令的芯片端到端。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建、vitest、
   sidecar 测试全绿。
2. 离线：完整芯片生命周期（生成 -> 门 -> 芯片 -> 接受 -> 撰写器作为回复
   预填）；缓存行为钉死；忽略钉死。
3. 活体：草稿 + 裁决 + 摘要 + 风格对端点运行、成本合理；缓存的摘要第二
   次调用无网络请求。
4. `docs/ai-replies.md`（+zh）载契约（池、门、芯片、缓存、风格、人发送
   规则），capabilities 行随镜像移动，fragment 落地。
5. 只允许增量线上变更；既有夹具与测试原样通过。
