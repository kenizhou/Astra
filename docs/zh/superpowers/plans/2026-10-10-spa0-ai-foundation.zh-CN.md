# SPA0 AI 基础实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-10-spa0-ai-foundation.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 立起 AI 骨架：astra-ai 注册表与台账（产品 SQLite 到来）、带 tier
回退的双协议后台 LLM 客户端、带审计的 astra-cli agent 工具面、kylins 模式
sidecar（产物、协议、safeExec、权限桥、工具）、宿主 relay/监督器/面板、
带端到端 correlation id 的离线 e2e 对话。

**架构：** astra-ai 持类型、tier 表与 sqlx 库（llm_calls + audit_log）供宿主
与 CLI 共享；client/desktop 跑后台客户端与 sidecar relay；sidecar/ 是 pi
SDK 上的 esbuild CJS 包；cli/ 是引擎库之上的 agent-first 二进制。

**技术栈：** Rust（sqlx sqlite 入 workspace 表）、Node >= 22 sidecar（pi SDK
按 kylins pin、esbuild）、客户端用 reqwest、既有 harness 模式。

**Spec：** `docs/superpowers/specs/2026-10-10-spa0-ai-foundation-design.md`

## 全局约束

- 沿前序家规：SPDX、500 行上限、deny-everything lints、提交前 gate（直接
  退出码、绝不接管道）、conventional commits、无破折号、英式英语、zh 镜像
  同提交、只允许增量线上变更。
- 移植根：kylins = D:/Projects/mailclient/kylins（kylins.client.agent-
  sidecar、kylins.client.backend/src/agent、kylins-cli）；inbox-zero =
  D:/Projects/mailclient/opensource/inbox-zero（apps/web/utils/llms 的
  tier/回退形态）；引擎经 `git show 68eab00:<path>`。
- 安全线（父 spec 第 8 节）：key 仅存内存；safeExec 净化环境 + 前缀白名单；
  权限桥 fail-closed；发送类动词拒绝；每条台账与审计行带 correlation id。
- AR1-AR7 适用（kylins 侧重命名 kylins -> astra；无历史注释）。

---

### 任务 1：astra-ai crate（注册表、台账、加固类型）

**文件：**
- 新建：`crates/astra-ai/src/{lib.rs, use_case.rs, tier.rs, tier_tests.rs,
  store.rs, store_tests.rs, migrations/0001_init.sql, correlation.rs,
  hardening.rs, hardening_tests.rs}` + Cargo.toml
- 形态移植自：inbox-zero `apps/web/utils/llms/{model.ts, use-cases.ts}`
  （tier 列表语法、use-case 命名）；kylins 的 llm_calls 列与审计形态。

**接口：**
- 产出：`LlmUseCase`（SPA0 六变体，serde snake_case）；`ModelTier::
  {Default, Economy, Chat, Draft, Nano}`（线词 default/economy/chat/
  draft/nano）；`TierTable::from_env()`（use case 解析为有序
  provider:model 链；环境变量 `ASTRA_LLM_DEFAULT` 等、逗号列表、首主余
  从；解析错误响亮）；`AiStore::open(path)`（sqlx sqlite、内嵌迁移）带
  `record_llm_call(LlmCallRow)` 与 `record_audit(AuditRow)`；
  `LlmCallRow { correlation: CorrelationId, use_case, tier, model,
  prompt_tokens, completion_tokens, cost_micros: Option<u64>, latency_ms,
  outcome }`；`AuditRow { correlation, verb, arg_preview, outcome }`；
  `CorrelationId` newtype（serde、`CorrelationId::generate()` 生成）；
  `tag_untrusted(heading, content) -> String`（<untrusted> 包装）+
  `DlpPolicy::{Allow, Redact, Block}` 及 SPA0 返回 Allow 的
  `resolve(policy)`（redact/block 随 A1，类型文档记录）。

- [ ] 步骤 1：tier_tests 先行（环境解析：单项、链、畸形响亮；各 tier 默认；
  六个 use-case 到 tier 的映射）。
- [ ] 步骤 2：store_tests 先行（内存打开、迁移应用、llm_call 与审计往返逐
  字段保持；correlation id 往返同串）。
- [ ] 步骤 3：hardening_tests 先行（包装带标题与围栏；对已包装内容幂等）。
- [ ] 步骤 4：实现；sqlx 入 workspace 表（sqlite、runtime-tokio）；crate 入
  成员与 default-members；全绿；提交
  `feat: add the ai registry, ledgers, and hardening types`。

### 任务 2：后台 LLM 客户端（双协议、回退、台账）

**文件：**
- 新建：`client/desktop/src/ai/{mod.rs, client.rs, client_tests.rs,
  wire.rs}`（测试夹具服务器辅助在测试下）
- 形态移植自：kylins `kylins.client.backend/src/agent/llm_client.rs`（双
  协议线）+ inbox-zero `utils/llms/index.ts`（回退错误类别）。

**接口：**
- 产出：`LlmClient::new(http, registry: TierTable, store: Arc<AiStore>)`；
  `async fn complete(&self, use_case, prompt, correlation) -> Result<String,
  LlmError>`（非流式补全；流式变体随 A2 对话 UI，记录）；协议按 tier 项
  前缀（`openai:`/`anthropic:`——kylins 约定；读其配置拼写并镜像）；可重试
  错误（连接、5xx、429 带 retry-after）推进回退链；配额与鉴权错误终止该次
  调用；每次尝试写 LlmCallRow；结构化输出的 JSON 修复辅助 `repair_json(
  raw)`（平衡括号提取；调用方以修复结果重试一次）。

- [ ] 步骤 1：client_tests 先行配本地一次性 HTTP 夹具（autodetect 模式）：
  各协议成功（按夹具捕获断言两种线形态字节精确）；主选可重试失败后回退成
  功（两条台账行、tier 前进）；配额终止（一行、类型化错误）；坏 JSON 修复。
- [ ] 步骤 2：实现；key 启动时按 `ASTRA_LLM_API_KEY_<PROVIDER>` 风格读一次
  （镜像 kylins 命名；记录）并持于内存。
- [ ] 步骤 3：全绿；提交
  `feat: add the background llm client with tier fallback and ledger`。

### 任务 3：astra-cli（agent-first 动词、信封、preview、审计）

**文件：**
- 新建：`cli/src/{main.rs, envelope.rs, envelope_tests.rs, verbs/{mod.rs,
  accounts.rs, folders.rs, messages.rs, drafts.rs}, audit.rs}` + 逐动词
  测试；修改 `.github/workflows/ci.yml`（desktop job 构建 CLI）、
  `client/desktop/tauri.conf.json`（externalBin 暂存；beforeBuildCommand
  构建 CLI——读 kylins 暂存脚本机制并以 scripts/stage-cli.mjs 最小镜像）

**接口：**
- 产出：bin `astra-cli` 六动词；`--json` 信封 `{schema: 1, ok: true,
  data}` / `{schema: 1, ok: false, error: {code, message}}`（schema 钉
  死）；`drafts save` 与 `drafts discard` 带 `--preview`（预览打印引擎调用
  将执行的动作，单一来源：动词用的同一构建器）；每次调用在数据目录旁打开
  `AiStore` 审计（动词、脱敏参数预览、ok/err、`--correlation-id` 或生成
  的 correlation id）；读取按 `ASTRA_DATA_DIR` 打开引擎库（只读打开；诚实
  记录第二写者缝：草稿写入是引擎自身 put/delete 于其锁下；应用经下次刷新
  的同步路径得知——记为已知交互）；退出码：0 成功、1 运行错误、2 用法；
  发送类动词（`send`、`queue`）解析即拒、退出码 3 并点名人发送规则。

- [ ] 步骤 1：envelope_tests 先行（两形态字节钉死；schema 字段常量）。
- [ ] 步骤 2：逐动词测试先行配内存引擎（list/read 形态；drafts save/
  discard 往返含 preview 等值；每次调用的审计行带 correlation id）。
- [ ] 步骤 3：实现 + CI/暂存接线；desktop beforeBuildCommand 变更经本地
  `cargo build -p astra-cli` 与暂存脚本的占位模式验证；提交
  `feat: add the agent-first astra-cli with audited verbs`。

### 任务 4：sidecar 包

**文件：**
- 新建：`sidecar/{package.json, tsconfig.json, esbuild.config.mjs,
  src/index.ts, src/protocol.ts, src/protocol.test.ts, src/harness/
  {pi-adapter.ts, mock.ts}, src/exec/safeExec.ts, src/exec/safeExec.test.ts,
  src/permissions/{bridge.ts, bridge.test.ts}, src/tools/mailTools.ts,
  src/tools/mailTools.test.ts, src/skills/draft-emails.md}`
- 移植自：kylins `kylins.client.agent-sidecar/**` 按 spec D4 裁剪（去掉
  memory 模块、triage、rules——A 切片带来）。

**接口：**
- 产出：JSONL 协议模块（TS 镜像；类型经共享夹具测试与 Rust 镜像的 serde
  对钉——双侧解析同一批夹具文件，`fixtures/protocol/*.json` 提交于
  sidecar/src 下并由 Rust 测试 include_str）；safeExec（环境净化到最小白
  名单 + ASTRA_* 透传、argv 前缀检查 `astra-cli`、cwd 在数据目录）；权限
  桥（init 下推策略表；每个受门工具需批准除非表授予；未知 fail closed）；
  mailTools（五工具经 safeExec 调用并带 `--json --correlation-id` 接自轮
  次 id）；MockHarness（无 pi SDK 的夹具转录驱动协议）；pi 适配器挂载工具
  与单模型注册（init 的 modelConfigs）；产物经 esbuild 构建为
  dist/index.cjs（单文件、CJS、无外部依赖）。

- [ ] 步骤 1：protocol.test 先行（每个消息形态往返；未知类型报错）。
- [ ] 步骤 2：safeExec.test + bridge.test + mailTools.test 先行（桩 CLI
  二进制：夹具脚本以 JSON 回显其 argv/环境子集；断言净化环境；fail-closed
  门控；工具载荷携带 correlation id）。
- [ ] 步骤 3：实现；`node --test` 绿；esbuild 产物可构建且字节级重建稳定；
  提交 `feat: add the agent sidecar with safe exec and the permission bridge`。

### 任务 5：宿主 relay、监督器、权限路由、面板

**文件：**
- 新建：`client/desktop/src/agent/{mod.rs, relay.rs, relay_tests.rs,
  supervisor.rs, supervisor_tests.rs, permissions.rs, commands.rs,
  commands_tests.rs, protocol.rs}`（sidecar 协议的 Rust 镜像）；修改：
  `client/desktop/src/lib.rs`（命令 + 事件接线）、
  `client/desktop/tauri.conf.json`（打包 sidecar 资源）；新建：
  `client/web/src/features/agent/{AgentPanel.tsx, AgentPanel.test.tsx,
  agentStore.ts}` + services/ipc 增量
- 移植自：kylins `kylins.client.backend/src/agent/**` 裁剪（relay/
  supervisor/permission_bridge/commands 形态）+ AgentPanel 骨架。

**接口：**
- 产出：`SidecarRelay`（以环境拉起 `node dist/index.cjs`：模型配置取自
  `ASTRA_LLM_*`、数据目录、除模型 key 外无秘密；init 握手 -> ready；JSONL
  泵）；监督器（陈旧代际拒绝、线性退避 1s/2s/4s 封顶、预算尽后 Lost 态、
  `sidecar-lost` 事件）；`agent_prompt(prompt, correlation) -> prompt id`
  命令（缺省生成 correlation id；流式发 `agent:event` Tauri 事件：文本
  增量、工具调用、带 correlation id 的权限请求）；`agent_abort`、
  `agent_status`；权限桥路由到 `agent:permission-requested` 事件并经
  `permission_response` 送回；sidecar 的 `llm_call` 信封写 AiStore 台账；
  AgentPanel（流式聊天记录、输入框、派发响应的批准卡、中止按钮）。

- [ ] 步骤 1：supervisor/relay 测试先行配桩 sidecar 脚本（夹具回显脚本走
  ready/event/done；代际拒绝；退避步数；预算尽 Lost）。
- [ ] 步骤 2：commands_tests 先行（对桩的 prompt 往返；correlation id 生成
  且事件回显；权限往返；abort）。
- [ ] 步骤 3：AgentPanel 测试先行（记录渲染流式增量；批准卡派发响应；
  abort 派发）。
- [ ] 步骤 4：实现；全绿；提交
  `feat: host the sidecar with supervision, permissions, and the agent panel`。

### 任务 6：离线 e2e、文档、能力行

**文件：**
- 新建：`client/desktop/tests/agent_e2e.rs`（或 crate 测试目录；离线 e2e：
  宿主 + 真 sidecar 产物 + 任务 4 夹具的桩 CLI；脚本化对话：prompt -> 文本
  增量 -> 受门 save_draft 批准 -> tool_result -> done；断言审计行与
  llm_call 行共享轮次 correlation id）；修改：`docs/ai-foundation.md`（+
  zh 镜像）、`docs/capabilities.md`（+zh 镜像）、
  `docs/changelog/unreleased/ai-foundation.md`

- [ ] 步骤 1：e2e 先行（红：T5 前无宿主；T5 后转绿），运行并记录输出。
- [ ] 步骤 2：ai-foundation.md 的手动 LLM 冒烟清单章节（作者的步骤配其
  连接信息；清单点名每条命令）。
- [ ] 步骤 3：capabilities 行（AI 基础行 partial；CLI 行 partial）+ zh
  镜像 + fragment（Platforms: desktop、Bump: minor）；ai-foundation.md
  载三原则、注册表、可追踪契约、产品库到来、eval 工装随 A1 的推迟、已知
  缺口（单模型注册、无流式客户端、DLP 仅 allow、CLI 第二写者缝）。
- [ ] 步骤 4：全量验证（gate 直接退出码、workspace、doc、desktop、web、
  vitest、sidecar node 测试、CLI 构建）；提交
  `feat: prove the agent loop offline and record the ai contract`。

### 任务 7：收尾

- [ ] 步骤 1：全量验证绿；推 `spa0-ai-foundation` 分支；开 PR（按规则书
  预期 squash 合并）链接 spec；验收证据；清台账。

## 自审记录

- Spec 覆盖：D1（T1）、D2（T2）、D3（T3）、D4（T4）、D5（T5 + T3 暂存）、
  D6（T1 correlation + T3 审计 + T5 传播 + T6 e2e 断言）、文档（T6）。
- 接口链：注册表/库（T1）-> 客户端 + CLI（T2/T3 各消费 T1）-> sidecar 走
  CLI（T4）-> 宿主（T5）-> e2e/文档（T6）。
- 携带风险：kylins pin 上的 pi SDK API 面（T4 裁剪前逐字读 kylins 适配
  器）；Rust/TS 协议镜像漂移（T4 的共享夹具测试是网）；引擎库上 CLI 第二
  写者缝（T3 诚实记录 + ai-foundation.md 已知缺口）；CI 的 Node 发现与
  版本门（T5 监督器测试覆盖逻辑；CI runner 有 Node——desktop job 的 node
  步骤已在）。
