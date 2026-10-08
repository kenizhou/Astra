# Astra：AI Agent 驱动的开发工作流与系统设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`）。

日期：2026-10-08
状态：proposed（待审阅）
许可证：Astra 仓库为 GPL-3.0-only（见 2.3 节）

## 1. 目的

Astra 是一个新的桌面邮件客户端（"Astra Mail"），由完全 AI agent 驱动的工作流
构建，覆盖开发、测试、交付（CI/CD）以及从参考项目的持续移植。本文件是系统
与构建系统的工作流的顶层设计。

本文件的每一条决策都受三条规则支配：

1. 技术栈完全跟随 kylins PoC（`D:\Projects\mailclient\kylins`）。
2. 功能集、实现约束、文档规则与仓库组织方式完全跟随 mail-calendar
   （`D:\Projects\mailclient\opensource\mail-calendar`），只移植
   mail-calendar 已实现的功能。
3. AI 相关设计完全跟随 inbox-zero
   （`D:\Projects\mailclient\opensource\inbox-zero`）。

Astra 的所有项目文档、代码、注释与文件夹名称只用英文；英文是唯一的规范
语言。为便于审阅，每份 spec、plan 与契约文档在 `docs/zh/` 下以镜像路径附一份
非规范性中文审阅版；两者有出入时以英文版为准（见第 10 节）。

## 2. 参考项目与移植策略

### 2.1 分工

| 参考 | 角色 | 取什么 |
|---|---|---|
| kylins | 技术栈 | Tauri 2 + Rust host、React 19 SPA（Vite、Tailwind v4、Zustand、React Aria）、UI 布局与样式、Node agent sidecar（pi SDK、JSONL over stdio、权限桥）、安全实现、CLI 架构 |
| mail-calendar | 功能与组织 | 全部已实现功能集（见其 `docs/capabilities.md`）、Intent/Surface 核心架构、契约文档规则、AGENTS.md 式规则书、测试约定、`xtask` gate |
| inbox-zero | AI 设计 | LLM 抽象模式（use-case 注册表、model tiers、fallback 链）、结构化输出、注入防护、DLP、逐调用台账、eval 套件、AI 功能目录 |
| allodia-eu/email-calendar-sync-engine | 同步引擎 | 作为依赖消费，按精确 revision git-pin，与 mail-calendar 同源同锁定方式 |

### 2.2 直接移植策略

因为 Astra 是 GPL-3.0（见 2.3），我们直接移植代码而不是照文档重新实现：

- mail-calendar 的 Rust 核心（14 个 crate，约 955 个文件）复制进 `astra-*`
  crate，适配包名与 upstream engine API。这是最大的加速来源：核心本来就是
  客户端无关设计。
- inbox-zero 的纯 TypeScript 逻辑（prompt 构建、规则匹配、JSON 修复、注入
  防护）适配进 Node sidecar。
- inbox-zero 的 eval 套件与 LLM judge 模式移植为 Astra 的 AI 行为门禁。
- 直接移植覆盖不到的部分：React SPA 按 kylins 模式新写（mail-calendar 没有
  web 客户端）；inbox-zero 的运行时胶水（Next.js 路由、Prisma、Redis）换成
  Tauri IPC、SQLite 与本地任务；engine host 层按 upstream engine API 编写而
  非照搬 kylins fork。

### 2.3 许可证

- mail-calendar 是 GPL-3.0-only；inbox-zero 是 AGPL-3.0；kylins 是作者本人
  的作品。
- Astra 采用 GPL-3.0-only。AGPL-3.0 文件保留其文件级义务与署名；对本地桌面
  应用，AGPL 网络条款实际不触发。
- 逐文件许可证头沿用 REUSE 约定；`xtask` 检查。
- 现有 Apache-2.0 的 `LICENSE` 文件在 SP0 替换为 GPL-3.0。

## 3. 产品标识、版本与图标

- 产品名 **Astra Mail**；仓库与包前缀 `astra`；bundle id
  `com.kenizhou.astra`。名称与 id 构建时从 `branding/default.env` 注入，
  任何客户端都不硬编码（mail-calendar 的 branding 规则）。
- 版本方案沿用 mail-calendar：根目录 `VERSION` 文件是唯一版本源，只在
  release PR 中移动；`xtask` 跑 version-sync 检查。移植期 semver `0.x.y`；
  功能完整时 `1.0.0`。
- Nightly 产物版本戳 `X.Y.Z-nightly.YYYYMMDD.N`，tag `nightly-YYYYMMDD`。
  手动发布 tag `vX.Y.Z` 永久保留；只留十个的规则仅作用于 nightly。
- 图标：`Images/logo.jpg` 移入 `branding/` 作为源图。脚本再生成 Tauri 全套
  图标（多尺寸 `icon.ico`、512 的 `icon.png`、`icon.icns`、32 px favicon）到
  `client/desktop/icons/`。可复现、可替换。

## 4. 仓库布局

mail-calendar 的组织方式适配到 kylins 技术栈的单桌面客户端：

```
astra/
  Cargo.toml            # workspace: crates/*、client/desktop、cli、xtask
  VERSION
  branding/             # default.env（名称、app id）+ 图标源
  crates/
    astra-app/          # 命令面：Intent 派发 -> Surface 快照
    astra-viewmodel/    # 纯不可变快照
    astra-account/      # 账户配置 -> provider 连接、重连、令牌刷新
    astra-autodetect/   # 账户自动探测策略
    astra-composer/     # ComposerDocument -> 确定性 HTML
    astra-oauth/        # OAuth 2.0 PKCE 客户端
    astra-mcp/          # 本地 MCP agent 服务器（opt-in，默认关闭）
    astra-plugin/       # 插件宿主：manifest、生命周期、监督、hook API
  client/
    desktop/            # 唯一客户端：Tauri 2 Rust host（kylins 后端模式）
    web/                # React 19 SPA（kylins 前端模式）
  sidecar/              # AI agent sidecar（Node >= 22、pi SDK、JSONL 协议）
  cli/                  # astra-cli：AI agent 的工具调用入口
  plugins/              # 一方插件包（先移植 example-plugin）
  docs/                 # 契约文档 + capabilities.md + changelog/
  refs/                 # refs-lock.toml：锁定基线与移植进度
  scripts/{ci,dev}/     # harness.sh（Stalwart）、changed-areas.sh、图标管线
  xtask/                # gate + 契约检查
  .github/workflows/    # ci.yml + nightly.yml
  AGENTS.md             # 唯一规则书
  LICENSE               # GPL-3.0-only
```

与作者商定的有意偏离：

- kylins 的 crypto 工作区（SMIME/OpenPGP）**不**移植进核心。加密功能后续以
  一方运行时插件回归（第 6 节）。
- composer 用 kylins 的 TipTap in-SPA 编辑器而非 mail-calendar 的独立
  web-view 编辑器；composer-security 契约仍然适用。
- 本地化暂缓；初期 UI 纯英文。确有第二种语言需求时再加 `astra-l10n` crate。
- 插件系统**在**范围内（来自 kylins 的架构能力，不是 mail-calendar 功能）；
  `capabilities.md` 含 Astra 专属的插件与 hook 矩阵段。

## 5. 核心架构

### 5.1 单向数据环（来自 mail-calendar）

React SPA 发出 serde 可序列化的 **Intent**。`astra-app` 经 engine 的 durable
outbox 应用它、重算 view model 并通知客户端；SPA 再经 Tauri IPC 拉取不可变
**Surface** 快照。Surface 集合对应 mail-calendar 的（MailboxList、Calendar、
Reading、Sending、SyncProgress、Connectivity、Contacts、InvitationReply、
DraftStatus 等）。SPA 是薄渲染器；与 mail-calendar 的 UniFFI 绑定只差传输层。

### 5.2 引擎

upstream `allodia-eu/email-calendar-sync-engine`，在 workspace
`Cargo.toml` 按精确 revision git-pin（与 mail-calendar 相同的锁定方式），
**只**经其 `engine-api` 门面消费。不使用 kylins fork；fork 特有的适配在
Astra 核心层按 upstream 的 raw 形状实现。双库模型同 kylins：engine 持有私有
库；Astra 持有产品库（drafts、receipts、filters、插件状态、AI 表）。

### 5.3 宿主服务端口（mail-calendar 的端口落到 Tauri）

Observer -> Tauri 事件；账户凭据 -> OS keyring；OAuth 浏览器腿 -> 系统浏览器
+ deep link；后台同步 -> host 内 tokio 任务；AgentHostUi -> sidecar relay。

### 5.4 桌面壳（来自 kylins）

无边框窗口 + 自定义标题栏；可调三栏布局（FolderPane | MessageList |
ReadingPane）；主题系统；Zustand stores 消费 surface；React Aria 组件；
Tailwind v4。

### 5.5 三前端，一命令层

SPA、`astra-cli` 与 MCP 服务器都是同一 `astra-app` Intent 面上的薄前端。每个
产品操作只有一份实现。

## 6. 插件系统（运行时）

- `crates/astra-plugin/` 承载插件：manifest 声明、生命周期、进程监督（复用
  sidecar relay 与 supervisor 模式）、**版本化的 hook API**、fail-closed
  权限模型（gated 工具默认需要批准，与权限桥一致）。
- `client/web/src/plugins/` 是从 kylins 移植的 JS slot-injection 运行时；
  `plugins/` 放一方插件包，先以移植的 `example-plugin` 作为活体契约示例。
- Hook 面 v1，以加密为第一消费者设计，保持最小：
  - `compose.transform`：发送前变换（加密、签名），位于 astra-composer
    输出接缝。
  - `message.decode`：抓取后解码（解密、验签），产出可渲染部件与安全横幅
    数据。
  - UI slots：阅读窗横幅、撰写选项、设置面板、账户向导贡献点。
- Hook 永不触碰 engine。发送纪律（durable outbox、never-send-twice）不能被
  插件绕过。
- `docs/plugins.md` 是契约：hook 面、manifest、权限、API 版本化与稳定性
  承诺。Hook API 变更遵循其中的版本化流程；插件矩阵在 `capabilities.md`。
- 加密（SMIME、OpenPGP）后续以一方插件包交付，复用 kylins crypto crate。
  在功能对标完成后启动，不阻塞主线。

## 7. astra-cli：AI agent 的工具调用入口

CLI 按 **agent-first** 设计：它是 Astra 内置 AI agent（现在与未来）唯一的
工具调用入口。人类终端使用与脚本自动化共享同一契约，属副产品。

- 每个 agent 工具映射到 CLI 动词。kylins 的六个工具（safe_exec、
  session_search、generate_draft、archive_email、modify_labels、skill_view）
  以及 AI 各分期新增的能力（规则管理、分类、digest、split inbox 操作等）
  一律经 CLI 动词触达数据面。没有旁路。
- 权限门控永远在 CLI 之前：safeExec（净化环境、前缀白名单只含
  `astra-cli`）把危险动词映射为带预览与硬确认的虚拟权限工具；发送类动词
  对 agent 一律拒绝。人决定，人发送。
- 每个动词都有 `--json`：稳定、版本化的 JSON schema；错误也机器可读。它首先
  为 agent 服务（工具解析），然后才是脚本与测试夹具。
- 变更动词有 `--preview`：干跑输出即权限确认卡的数据源。预览与行为不会
  漂移，因为两者都由 CLI 产出。
- CLI 的一切变更动作落 audit log，与 GUI 操作同一审计流。
- 用 clap_complete 生成 shell 补全（bash、zsh、fish、pwsh）。
- 动词表面随功能切片生长：每个 SP2+/SPA 切片在同一次变更中交付自己的动词、
  JSON schema 增量与所需的新虚拟权限工具。跨切面的强化收进 SP-CLI 切片。
- `docs/cli.md` 是契约：动词表、权限门控映射、JSON 与退出码契约。
  `capabilities.md` 含 CLI 段。
- MCP 服务器面向**外部** agent（opt-in、默认关闭、空 allow list、读经无状态
  `query_*`、写经 Intent、无不可逆原语，与 mail-calendar 规定一致）。CLI
  面向**内置** agent。两者都派发 `astra-app` Intent。

## 8. 安全架构

基线：kylins 的实现。裁决规则（写入 AGENTS.md）：实现有分歧时，安全机制上
kylins 有优势则 kylins 优先；功能契约上 mail-calendar 优先。

- 进程隔离与监督（kylins）：sidecar 是独立进程；supervisor 拒绝过期代际、
  排空 pending prompt、线性退避并上报 Lost 状态。
- Fail-closed 权限桥（kylins）：策略表由 host 下发；每个 gated 工具初始即
  需批准；预览与硬确认；发送类动词对 agent 拒绝。
- safe_exec（kylins）：净化环境、前缀白名单、危险动词映射虚拟权限工具。
- 台账与审计（kylins）：每次 LLM 调用记录进 `llm_calls` 表（扩展 use case、
  tier 与 cost 字段）；audit log；circuit breaker；AI cache。
- 密钥（kylins）：模型 API key 只存 sidecar 内存；账户凭据进 OS keyring；
  CSP 放行 `https:` 与本地模型端点（Ollama、LM Studio）。
- 隐私模式（kylins 设计文档）：LocalOnly、Balanced、Cloud。
- 沙箱 HTML 渲染与远程图片门控、证书例外与 pinning，随 mail-calendar 对应
  功能移植并遵守其契约。
- 注入防护与 DLP（inbox-zero 模式）：不可信邮件内容打标包裹；DLP 策略
  allow/redact/block；叠加在权限桥之上。

## 9. AI 架构与功能分期

### 9.1 进程模型与 sidecar 要求（kylins）

Node >= 22 sidecar 由 Rust host 拉起，JSONL over stdio，镜像协议文件
（sidecar 的 TypeScript、host 的 Rust），supervisor 带退避，fail-closed
权限桥，一切邮件变更经 `astra-cli`。"AI 建议与起草；人决定与发送。"

交互式 agent 对话跑在 sidecar（pi SDK）。后台与结构化任务（规则匹配、分类、
摘要、digest）跑在 Rust LLM client。两侧共享 tier/use-case 配置与使用台账。

sidecar 按轻量、安全、全操作可追踪三项要求设计：

- **轻量**：esbuild 打包为单一 CommonJS 产物，依赖精简且锁定版本；不直接
  访问数据库或 engine（数据面只经 `astra-cli` 触达）；除会话转录外无状态；
  拉起、监督、替换都很廉价。
- **安全**：第 8 节全部适用；模型 API key 只存 sidecar 内存；每次 exec 都
  净化环境；权限桥 fail closed。
- **可追踪**：每个 agent 操作端到端可复原。每次 LLM 调用落 `llm_calls`
  台账；每次工具调用以动词、参数预览、结果与 correlation id 落 audit
  log；每次权限决定（命中策略、人工批准、拒绝）都有记录；会话转录以
  JSONL 持久化在应用数据目录；每个 agent 轮次一个 correlation id，从 UI
  经 Rust host、sidecar、LLM provider、CLI 贯通传播进每条台账记录——
  任何观察到的邮件变更都能回溯到引发它的 prompt 与批准。

### 9.2 inbox-zero 模式映射到本栈

- Use-case 注册表（约 40 个 use case：AssistantChat、DraftReply、
  CategorizeSender、CleanInbox、Summarise 等）映射五档 tier（DEFAULT、
  ECONOMY、CHAT、DRAFT、NANO）。单一事实源在 Rust 设置，推送给 sidecar 与
  Rust LLM client 两侧。
- 可重试/配额错误走 fallback 链；结构化输出带 schema 约束与 JSON 修复；
  流式仅用于用户可见文本。
- 逐调用使用台账（use case、tier、cost）加 NANO 档周度花费护栏。
- 每个 AI 功能一个 pipeline 版本常量；prompt 或管线变更必须过 eval（规则书
  条目）。
- 助手 agent：带墙钟预算的 tool loop、durable 状态先读后写守卫、破坏性动作
  pending confirmation（映射到权限桥虚拟工具）、渐进式工具披露、上下文压缩
  与记忆抽取。

### 9.3 分期（inbox-zero 全目录，除仅 web 形态者）

- **A1**：AI 规则（prompt-to-rules、来信经后台档匹配规则、确定性动作执行、
  学习模式）、发件人分类、冷邮件拦截。
- **A2**：回复起草（上下文收集器、写作风格）、摘要（带缓存）、助手对话。
- **A3**：知识与 persona 抽取、snippets、撰写自动补全、翻译。
- **A4**：digest、会议简报（日历集成）、分析报告、split inbox（FYI/
  Actioned）。
- MCP 双向：暴露（随 mail-calendar 功能集移植）与消费（助手读外部 MCP
  服务器取上下文）。
- Slack/Telegram 表面与 web onboarding 助手不适用于桌面应用；若需要，A4 中
  可适配桌面首启助手。
- 每个 AI 功能随附契约文档 `docs/ai-<feature>.md`，mail-calendar 契约风格。

## 10. 文档与规则书

`AGENTS.md` 从 mail-calendar 规则书适配，英文，保留其纪律：500 行文件上限
（xtask 强制）、身份用 newtype、行为变更 test-first、bug 修复在最低共享层带
回归测试、engine 只经 engine-api、协议解析归 engine、Intent 保持 serde 可
序列化、`capabilities.md` 与功能同变更移动、四步契约变更流程（文档规则 +
矩阵 + 每个交付平台 + changelog fragment）、"say it once"、注释只描述现在、
日志行不含仓库路径、changelog fragment 放 `docs/changelog/unreleased/
<slug>.md` 并带 Platforms 与 Bump 标签、`VERSION` 只在 release PR 移动、
每次 push 前跑 `cargo xtask gate`、英式英语、破折号不作标点。

Astra 新增规则：文档、代码、注释与文件夹名只用英文（xtask 检查 `docs/zh/`
以外的非 ASCII 路径）；AI 行为变更必须过 eval；插件 hook API 变更遵循
`docs/plugins.md` 的版本化流程；第 8 节的安全裁决规则。

`docs/` 按 mail-calendar 方式组织：README 契约文档索引表；契约文档随切片
逐域移植（accounts、folder-pane、search、drafts、sending、calendar、
invitations、contacts、mcp、rendering-security、composer-security 等）；
Astra 专属文档：`plugins.md`、`cli.md`、`upstream-sync.md` 与 `ai-*.md`。
`capabilities.md` 以 mail-calendar 矩阵限定桌面客户端列为起点，加上插件、
CLI 与 AI 功能的 Astra 段。

审阅副本：每份 spec、plan 与契约文档（含 `capabilities.md`）在
`docs/zh/<同路径>.zh-CN.md` 附非规范性中文译本，开头注明以英文版为准。
中文副本与英文源在同一次变更中移动，作者因此可以用中文审阅每份文档。代码、
注释、标识符与 changelog fragment 仍纯英文。`xtask` gate 含
`check-doc-sync` 契约检查：范围内每份英文文档都有中文镜像，且无孤儿镜像。

## 11. 测试策略

- Rust 核心：内联同目录测试文件（`<module>_tests.rs`）、fakes harness 与
  门控的活体测试，遵循 mail-calendar 约定。
- 测试环境，三层：
  - **Tier 1，可复现、无密钥**：Astra 按 mail-calendar 的方式自建 Docker
    测试环境：`scripts/dev/harness.sh` 起种子化 Stalwart 服务器，提供 JMAP
    与 IMAP/SMTP 收发往返。这是开发者与 CI 的默认活体环境。
  - **Tier 2，真实 provider 账号，环境门控，仅本地**：作者提供本地
    Exchange 服务器、outlook.com 账号与 Gmail 账号。这些套件沿用
    mail-calendar 的门控活体登录模式（`ASTRA_LIVE_*` 环境门控）。凭据只放
    本地未提交的环境文件，绝不进仓库，且绝不在公开 CI 运行。切片需要时会
    显式请求账号或凭据。
  - **Tier 3，AI 测试**：作者按请求提供 LLM 连接信息。sidecar 集成测试与
    AI eval 经应用自身的模型配置面消费；只存本地，绝不提交。
- SPA：Vitest + Testing Library（kylins 配置）。
- Sidecar：`node --test` 加夹具驱动的 MockHarness e2e（kylins）。
- 插件宿主：manifest、权限与生命周期单测加 example-plugin e2e。
- AI eval：场景文件与 LLM judge（inbox-zero 模式）；AI 行为变更的门禁。
- 台账与审计覆盖：MockHarness e2e 断言每条 gated 工具路径都产出其
  `llm_calls`、audit log 与权限决定记录，且 correlation id 端到端贯通。
- Gate：`cargo xtask gate`，从廉到贵：格式化、clippy pedantic、workspace
  测试、契约检查（file length、version sync、dash hygiene、英式英语、
  许可证头、非 ASCII 路径、icon sync、doc sync），并有 `--ui`、`--sidecar`、
  `--plugins` 扩展位。CI 镜像 gate。

## 12. CI/CD

### 12.1 `ci.yml`（每次 push 与 PR）

changed-areas 门控跳过无关 job（移植 `changed-areas.sh`）。Job：契约检查加
格式化与 clippy；workspace 测试加 Stalwart 活体套件；SPA lint、test、
build；sidecar 测试与打包；example-plugin 构建；三操作系统桌面冒烟构建；
单一必需 `CI OK` 检查，跳过计成功（mail-calendar 模式）。

### 12.2 `nightly.yml`（每天凌晨）

cron `30 16 * * *` UTC，即北京时间 00:30，给 22:00 同步任务留两个半小时；
支持手动触发。步骤：checkout；打版本戳 `X.Y.Z-nightly.YYYYMMDD.N`；矩阵构建
（Windows NSIS `.exe`；macOS aarch64 与 x86_64 `.dmg`；Linux `AppImage` 与
`.deb`）；从提交、能力矩阵增量与上游移植摘要生成 release notes；发布 tag
`nightly-YYYYMMDD` 的 GitHub Release；随后保留任务删除超出最新十个的
`nightly-*` release。

产物初期不签名；SmartScreen 与 Gatekeeper 说明写进 release body。有证书后
经 secrets 加签名，不改流程。版本戳中 `N` 计当晚重跑次数，从 0 起。

### 12.3 发布流程

release PR bump `VERSION` 并把 changelog fragment 转正；合并后打 tag
`vX.Y.Z` 触发发布工作流。正式发布永久保留；只有 nightly 会被清理。

## 13. 参考管理与 22:00 同步任务

### 13.1 锁

`refs/refs-lock.toml` 按参考仓库记录：url、`baseline`（引导时锁定的
commit）、`last-ported`（随移植推进）与范围说明。四个仓库：mail-calendar
（功能与文档）、inbox-zero（AI）、kylins（栈、UI、sidecar、安全）与
upstream engine（pin revision 加最后适配的 revision）。

### 13.2 锁定期

参考仓库在引导基线处冻结，直到功能对标：`capabilities.md` 中每行
mail-calendar 能力完成或明确 N/A、AI 分期 A1 至 A4 完成、插件宿主就位。

### 13.3 22:00 任务（ZCode 定时自动化，本地）

每晚 22:00 本地时间运行，幂等，以 `last-ported` commit 为锚，错过的夜晚
下次运行自动补齐：

1. fetch 全部四仓；diff `last-ported..HEAD`；为空则跳过。
2. 变更分类：仅文档、仅测试、重构、行为、新功能、安全。
3. 按规则书移植（契约文档、矩阵、changelog fragment 与文档的中文审阅副本
   在同一次变更落地）。标记为安全的变更一律留审，绝不自动合并。
4. 跑本地 gate 快集（全量 gate 由 CI 跑）。
5. 开附分仓摘要的 PR。仅低风险类别且 gate 绿时自动合并；其余等待早晨审阅
   并附 digest。
6. 合并后推进 `last-ported`。
7. UI 样式决策与架构冲突在 PR 中显式列出交作者决定；严禁静默偏离。
8. engine 更新经 bump pin revision、适配并走同一 PR 流程。

00:30 nightly 构建发布 `main` 上的当前状态：低风险自动合并的移植当晚上线；
留审的移植在早晨审阅合并后进入下一晚。

## 14. 白天开发工作流（AI agent 驱动）

- 切片 backlog 是 `capabilities.md` 的未完成行按依赖排序，加上 AI 分期。
- 切片循环：非平凡时先写小 spec；计划；并行 subagent 实现（独立切片并行，
  共享 crate 串行）；行为变更 test-first；`xtask gate`；PR 带 conventional
  commits、changelog fragment 与 capabilities 更新；code-review agent 审；
  作者合并。
- UI 决策门：凡 kylins UI 不合理而 mail-calendar 或 inbox-zero 更好之处，
  先停下与作者讨论再实现。同一道门约束 22:00 任务。
- 每日节奏：白天切片开发；22:00 上游移植；00:30 nightly；早晨审 PR。

## 15. 子项目分解

每个子项目走自己的 spec -> plan -> implement 循环。

- **SP0 引导**：拉取并锁定四仓；按第 4 节搭全仓库骨架；LICENSE 换
  GPL-3.0；AGENTS.md 规则书；xtask gate 骨架；`logo.jpg` 图标管线；CI
  骨架；nightly 骨架；VERSION 与 branding；docs 骨架加种子能力矩阵。
- **SP1 核心架构**：crate 骨架；engine 集成（upstream pin；engine host 按
  upstream API 编写）；Intent/Surface 协议 v1；Tauri host 与 kylins 布局的
  React 壳；插件宿主骨架；首个垂直切片（添加 IMAP 账户、同步、看到文件夹），
  含其最小 CLI 动词。
- **SP2+ 功能切片**：账户、folder pane、消息列表、阅读、撰写、drafts、
  outbox 与发送、搜索、线程、日历、邀请、联系人、设置、MCP 服务器；每个
  切片带自己的 CLI 动词与契约文档。
- **SPA0 AI 基础**：sidecar 移植；tier 与 use-case 注册表；使用台账；注入
  防护。
- **SPA1 至 SPA4**：第 9.3 节的 AI 分期。
- **SP-CLI**：动词面变宽后的跨切面 CLI 打磨（补全、帮助质量、性能）。
- **SP-CRYPTO**（对标后）：插件 API 打磨；SMIME 与 OpenPGP 一方插件。
- 22:00 同步自动化在 SP0/SP1 期间构建并演练（对锁定参考做干跑），对标时
  激活。

## 16. 待决项

- 桌面 UI 自动化路线：kylins 的 MockHarness 驱动 e2e 还是 WebDriver 驱动
  webview 测试；SP1 期间定夺。
- Tier 2 账号的 provider 可达性（outlook.com 个人账号与 on-prem Exchange
  对上游 engine 的 provider 集合）在 SP2 账户切片用活体测试解决。
- Windows 与 macOS 的代码签名证书；有则加。
- macOS 公证与 Tauri updater 通道在签名具备前不在范围内。

## 17. 决策日志

| 决策 | 选择 | 被否决的备选 |
|---|---|---|
| 客户端架构 | 单 Tauri 客户端 + mail-calendar 核心纪律 | UniFFI 多客户端预留（YAGNI）；无 Intent/Surface 协议的 kylins 分包 |
| 平台 | Windows、macOS、Linux | 仅 Windows |
| 产物托管 | GitHub Releases，nightly 只留十个 | 自建服务器上传；两者都要 |
| 引擎 | upstream，按 revision 锁定 | kylins fork（作者决定：与 mail-calendar 同源） |
| 许可证 | GPL-3.0-only，直接移植 | Apache-2.0 加净室重写（慢得多）；仅私用 |
| 插件 | 运行时：进程外后端 hook 加 JS slot-injection 前端 | 编译期 trait crate；仅前端预留接缝 |
| AI 范围 | inbox-zero 全目录，A1 至 A4 分期 | 仅核心集；核心加个性化 |
| 编排 | 混合：云端 CI 与构建，本地 ZCode 自动化做移植 | 全云端 CI agent（密钥与成本）；全本地（无法产 macOS 构建） |
| CLI 定位 | Agent-first 工具调用入口；人类 CLI 共享契约 | 人类优先 CLI 加 agent 适配层 |
| Sidecar 姿态 | 轻量（单一打包产物、不触数据面）、安全（第 8 节）、全操作可追踪（台账加端到端 correlation id） | 富进程内 AI 运行时；尽力而为的日志 |
| 安全基线 | 机制上 kylins 优先；功能契约上 mail-calendar 优先 | 逐案临时裁决 |
| 文档语言 | 英文规范；`docs/zh/` 非规范中文审阅副本同变更移动 | 仅英文（作者难审阅）；双语规范（漂移风险） |
