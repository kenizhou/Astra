# SPA0 AI 基础设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-10-spa0-ai-foundation-design.md`）。

日期：2026-10-10
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 7、8、9、15 节）
兑现：SP1 spec（D4）记录的产品数据库到来——AI 台账现在需要它。

## 1. 目的

把 AI 骨架立起来并用一次真实对话证明：Node sidecar（轻量、安全、全
追踪）、带逐调用台账的 tier 与 use-case 注册表、双协议后台 LLM 客户端、
带审计日志的 astra-cli agent 工具面、壳里的最小 agent 面板。A1 至 A4 的
功能都长在这些接缝里。

## 2. 决策

- **D1（astra-ai crate）**：`crates/astra-ai` 持有注册表与台账：
  `LlmUseCase`（起始集随各 AI 切片增长；SPA0 交付 AssistantChat、
  DraftReply、Summarise、CategorizeSender、CleanInbox、AgentTool）、
  `ModelTier`（Default、Economy、Chat、Draft、Nano）、tier 表（逗号列表
  可由环境变量配置，inbox-zero 语法：首项主选、其余回退）、产品 SQLite
  库（sqlx；迁移在本 crate：`llm_calls` 与 `audit_log` 表；由宿主与 CLI
  对同一数据目录打开）、`CorrelationId` newtype、提示注入加固包装
  （不可信内容打标函数加 DLP 策略枚举；默认策略 allow，redact 与 block
  随 A1 功能到来，记录）。
- **D2（后台 LLM 客户端）**：双协议客户端（`openai-completions` 与
  `anthropic-messages` 线形态）在 `client/desktop`，由注册表驱动：可重试
  或配额错误时 tier 回退、结构化输出的 JSON 修复、逐调用台账行（use
  case、tier、模型、token、有报则含 cost、时延、结果、correlation id）、
  流式仅用于用户可见文本。模型 API key 由环境变量在宿主启动时进入、只存
  宿主内存（绝不入库）；sidecar 经 init 握手接收其模型配置。
- **D3（astra-cli）**：`cli/` crate 交付 agent-first 二进制：已有表面的
  动词（`accounts list`、`folders list`、`messages list`、`messages
  read`、`drafts save`、`drafts discard`），每动词 `--json` 带版本化信封
  （`{schema: 1, ok, data | error}`）、变更动词 `--preview`、每次调用带
  correlation id 落审计（`--correlation-id`，缺省生成）、发送类动词在参数
  解析器里结构性拒绝（目前尚不存在；拒绝是结构性的）。读取经数据目录
  （`ASTRA_DATA_DIR`，默认在产品库旁）打开引擎库；草稿写入经引擎
  `put_draft`/`delete_draft`。二进制入 CI 构建并暂存为 Tauri
  `externalBin`。
- **D4（sidecar）**：`sidecar/` 移植裁剪后的 kylins 骨架：esbuild 单文件
  CJS 产物、TS 与 Rust 镜像的 JSONL 协议（下行 `init`、`prompt`、
  `abort`、`command`、`permission_response`；上行 `ready`、`event`、
  `done`、`error`、`result`、`llm_call`、`permission_request`、
  `tool_result`）、单模型注册下的 pi SDK 会话、`safeExec`（净化环境、
  前缀白名单仅 `astra-cli`）、fail-closed 权限桥（宿主下推策略表；受门
  工具需批准）、表面允许的工具集：`list_folders`、`list_messages`、
  `read_message`、`save_draft`、`discard_draft`（经 safeExec 走
  astra-cli；归档与标签随各自切片到来）。一个管线版本常量。测试工装：
  MockHarness 夹具模式（无网络）。
- **D5（宿主 agent 模块 + 面板）**：`client/desktop` 增 relay（从 PATH
  或 `ASTRA_AGENT_NODE` 拉起 Node >= 22、版本门、陈旧代际监督器带线性
  退避与 Lost 态）、权限桥（`permission_request` 路由为 Tauri 事件、
  `permission_response` 送回）、`agent_prompt`/`agent_abort`/
  `agent_status` 命令携带逐 prompt 的 correlation id、sidecar 的
  `llm_call` 信封写入台账、工具结果事件端到端携带 correlation id。SPA
  增最小 AgentPanel：流式聊天记录、输入框、权限请求的批准卡。
- **D6（可追踪契约）**：每个 agent 轮次一个 correlation id：宿主在
  `agent_prompt` 生成，随 prompt 信封传入 sidecar、进入每次工具执行
  （safeExec 环境）、进入 CLI 审计行、进入 sidecar 上报的 `llm_call` 行、
  进入后台客户端的台账行。审计记录每个 CLI 动词与参数预览及结果；台账
  记录两条路径的每次 LLM 调用。

## 3. 测试

- 单元（astra-ai）：注册表解析与回退顺序；内存库上台账与审计往返；
  correlation id 传播类型；加固包装打标。
- 客户端（后台）：本地一次性 HTTP 服务器夹具驱动两协议（成功、触发回退
  tier 的可重试失败、配额错误、坏 JSON 修复）；台账行断言。
- CLI：内存引擎上逐动词单测；信封 schema 钉死；preview 输出等于动作
  描述；审计行携带 correlation id；未知动词退出码。
- Sidecar（node --test + MockHarness）：协议往返；权限门 fail-closed
  （未批准的受门工具拒绝）；工具派发到桩 CLI 二进制；产物可构建。
- 宿主：桩 sidecar 脚本的监督器测试（ready 握手、代际拒绝、退避步进）；
  权限往返；sidecar 上报复用的台账写入。
- E2E（离线）：宿主 + 真 sidecar + 桩 CLI：一段脚本化对话走过 prompt、
  一次受门工具批准、一条 llm_call 台账行、done。
- 活体 LLM：给作者的手动冒烟清单（届时请求连接信息）；无自动化网络测试。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建与 vitest 全
   绿；sidecar 产物可构建且其 node 测试在 CI 通过。
2. 离线 e2e 对话端到端跑通：prompt 流式回复、受门工具调用浮出批准卡、
   批准执行 CLI 动词、审计与台账行以同一 correlation id 落地。
3. 一次后台客户端调用（夹具服务器）在可重试错误上跨 tier 回退且每次尝试
   落一条台账行。
4. CLI 结构性拒绝发送类动词；每个交付动词遵守 `--json` 与 `--preview`。
5. `docs/capabilities.md` 行移动（AI 基础行、CLI 行），zh 镜像、
   changelog fragment，同变更交付 `docs/ai-foundation.md`（sidecar 三
   原则契约、注册表、可追踪契约、eval 工装随 A1 的推迟）。

## 5. 范围外（A1+）

功能提示词与 eval（eval 工装随 A1 落地；按规则书的 eval 规则，提示词变
更时 sidecar 的管线常量 bump）、记忆与压缩、tier 设置 UI、DLP
redact/block 执行、归档与标签工具、MCP、周度花费护栏的 UI。
