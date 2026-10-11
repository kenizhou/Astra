# A3 知识、Snippets、自动补全、翻译设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-11-a3-knowledge-snippets-design.md`）。

日期：2026-10-11
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 9.3 节）
消费：A1/A2 骨架（客户端、缝、裁决器、台账、eval 工装）、阅读管线、撰写器。

## 1. 目的

助手的记忆与延展：提取的知识条目（事实、偏好、进行中的事项、人设注记）
塑造草稿；带 AI 搜索的 snippet 库；撰写自动补全建议；阅读窗的翻译动作。

## 2. 决策

- **D1（知识库与提取）**：迁移 `0007_knowledge`（id、account、kind:
  fact | preference | project | contact | persona、text、source_message、
  created_at、correlation_id）。提取是批处理：宿主命令
  `knowledge_extract(account, limit)` 对账户最近 N 封（N 上限 30）跑结构
  化提取，逐封 untrusted 包裹，产出零或多条带来源的条目
  （`KNOWLEDGE_EXTRACT_VERSION`、ECONOMY 档）。精确文本去重守卫拒绝重复
  条目。条目绝不自动删除；`knowledge_forget <id>` 移除（CLI + 宿主）。
  A2 的草稿 prompt 增长账户知识条目作为信任侧带标数据，按 kind 相关度序
  （persona + 偏好优先、然后事实/项目）取最新十条封顶，本身随
  `DRAFT_REPLY_PROMPT_VERSION` bump 版本化。
- **D2（snippets）**：迁移 `0008_snippets`（id、title、body、created_at、
  updated_at）。CRUD 经 CLI（`snippets list|create|delete`，agent-first 面）
  与撰写器选择器；搜索是**一次**结构化排名调用
  （`SNIPPET_SEARCH_VERSION`、ECONOMY 档）对标题与正文返回有序匹配 id，
  失败或空库回退子串搜索。撰写器选择器列出 snippets（最近优先）带搜索框；
  插入将正文放在光标处。
- **D3（自动补全）**：宿主命令 `compose_autocomplete(before, after)`
  （编辑器光标两侧文本，后文可选）返回一条简短续写
  （`AUTOCOMPLETE_VERSION`、CHAT 档为时延；花费护栏适用）。编辑器将建议
  以暗色内联芯片呈现在当前行下方，Tab 或点击插入；芯片绝不无手势插入。
  去抖空闲触发（停键两秒、未变文本跳过）；会话级忽略。
- **D4（翻译）**：阅读窗动作将打开的消息译入选定目标语言（固定小清单：
  English、Chinese、German、French、Japanese、Spanish；默认 English），由
  补全调用服务（`TRANSLATE_VERSION`、DEFAULT 档），正文 untrusted 包裹，
  缓存在迁移 `0009_text_cache`，键为（消息账户、消息键、kind=translate、
  目标语言、正文哈希）。结果在窗内作为可折叠块渲染于摘要旁；缓存命中无
  调用。
- **D5（线与 CLI）**：宿主命令 `knowledge_list`、`knowledge_extract`、
  `knowledge_forget`、`snippet_search`、`snippet_list`、
  `compose_autocomplete`、`translate`；CLI 动词读库（变更 CRUD 动词按契约
  带 `--preview`）；提取、搜索、自动补全、翻译绝不从 CLI 运行（never-dials
  规则）。

## 3. 测试

- 单元（astra-app/astra-ai，假件 + 夹具缝）：提取形态（条目 + 来源 + 去重
  + 上限）、prompt 内知识封顶与排序、snippet 搜索（排名、空库回退、失败
  回退）、自动补全（before/after 上下文、版本常量、未变跳过）、翻译（缓存
  命中无调用、语言/正文变更失效）。
- 夹具服务器：ECONOMY/CHAT/DEFAULT 档线形态。
- 前端（vitest）：snippet 选择器（列表、搜索、光标处插入）、自动补全芯片
  （去抖、忽略、仅手势插入、重渲染不插入）、翻译块（语言选择、缓存、新
  取、错误）。
- 活体（门控，所供端点 + harness）：对种子邮件的真实提取（条目带来源落
  地）；真实 snippet 排名；真实自动补全建议；真实翻译且缓存第二次调用无
  网络请求。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建、vitest、
   sidecar 测试全绿。
2. 离线：各功能完整生命周期钉死（提取 -> 入库 -> 塑形 prompt；snippet
   CRUD -> 搜索 -> 插入；自动补全去抖 -> 芯片 -> 插入；翻译 -> 缓存）。
3. 活体：提取、snippet 排名、自动补全、翻译对端点运行、成本合理；缓存的
   翻译第二次调用无请求。
4. `docs/ai-assist.md`（+zh）载契约与已知缺口，capabilities 行随镜像移
   动，fragment 落地，每个新 prompt 常量有带版本 pin 的 eval 夹具。
5. 只允许增量线上变更；既有夹具与测试原样通过。
