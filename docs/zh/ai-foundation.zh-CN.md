> 非规范性译本，仅供审阅；以英文版为准。

# AI 基础：Astra Mail 契约

**范围。** SPA0 的 AI 骨架：层级与用例注册表、后台 LLM 客户端、`ai.sqlite`
账本、相关性 id 的可追溯链，以及它们所服务的 sidecar 循环契约。对之后交付的
每一个 AI 特性（A1+）都有约束；阶段本身见能力矩阵与父规范第 9 节。

**原则。** *AI 建议并起草；人决定并发送。* 每次模型调用都可从账本重建，agent
造成的每一次邮件变更都能追溯到产生它的提问与批准，且任何密钥或凭据都不会到达
可能泄漏的地方。

## 三项原则

sidecar 与其周围的一切都是**轻**、**安全**、**完全可追溯**的（父规范
9.1 节）：

- **轻。** 一个 esbuild 打包的 CommonJS 产物，依赖固定且最少；不直接访问
  数据库或引擎，数据面只通过 `astra-cli` 触达；除会话转录外无状态；生成、
  监督、替换都廉价。
- **安全。** 模型 API 密钥只存在于内存中；生成环境是白名单；sidecar 启动的
  每个子进程都经过 `safe_exec`，带清洗过的环境与 argv 前缀检查；权限桥失败
  关闭；发送类动词被结构性拒绝，发送始终是人的动作。
- **完全可追溯。** 每次 LLM 调用落入 `llm_calls` 账本；每次工具调用与权限
  决定连同动词、参数预览、结果、相关性 id 落入审计日志；每个 agent 轮次一个
  相关性 id，从 UI 穿过宿主、sidecar、提供方调用、CLI 直到每一行记录，因此
  任何观察到的邮件变更都能追溯到导致它的提问与批准。

## 注册表

SPA0 交付六个用例，各映射到五个层级之一（移植自 inbox-zero 的用例表）：

| 用例 | 层级 |
|---|---|
| `assistant_chat` | chat |
| `draft_reply` | draft |
| `summarise` | default |
| `categorize_sender` | default |
| `clean_inbox` | default |
| `agent_tool` | economy |

层级由环境变量配置，每层一个逗号列表：`ASTRA_LLM_DEFAULT`、
`ASTRA_LLM_ECONOMY`、`ASTRA_LLM_CHAT`、`ASTRA_LLM_DRAFT`、
`ASTRA_LLM_NANO`。每个条目是 `provider:model`（例如
`openai:gpt-4o-mini,anthropic:claude-3-5-haiku-latest`）；第一个条目是主选，
其余是回退链。提供方前缀（`openai` 或 `anthropic`）选择线上协议。空值视为
未配置；畸形列表是点名变量的显式错误。某层未配置时，economy、chat、draft
回退到 default 层的链，nano 先回退到 economy 再到 default。

提供方凭据与端点同样是环境变量，启动时读一次并保持在内存中：

- 密钥：`ASTRA_LLM_API_KEY_OPENAI`、`ASTRA_LLM_API_KEY_ANTHROPIC`
  （模式为 `ASTRA_LLM_API_KEY_<PROVIDER>`，提供方大写）。
- base-url 覆盖：`ASTRA_LLM_BASE_URL_OPENAI`、
  `ASTRA_LLM_BASE_URL_ANTHROPIC`。默认为
  `https://api.openai.com/v1` 与 `https://api.anthropic.com`；覆盖存在的
  意义是能指名兼容端点（fixture，或 DeepSeek 风格的 anthropic 兼容主机）。
  anthropic base 上误粘贴的尾部 `/v1` 会被原谅。

**行内层级语义。** 层级回退在客户端见到链之前已经折叠
（`TierTable::resolve`），因此一次调用的每一行账本都带该用例自身的层级；
链内的条目级推进表现为各行 `model` 值的不同。

## 账本与可追溯契约

随 SPA0，AI 账本作为产品数据落位：`ai.sqlite` 在应用程序数据目录中创建于
`astra.sqlite` 之侧，宿主启动时打开，每次 `astra-cli` 调用也打开，两个写入方
共用一本账（五秒忙超时覆盖它们的重叠）。

两张表：

- `llm_calls`：id、created_at、correlation_id、use_case、tier、model、
  prompt_tokens、completion_tokens、cost_micros（可空）、latency_ms、
  outcome。
- `audit_log`：id、created_at、correlation_id、verb、arg_preview、outcome。

写入方：

- 后台客户端每次 HTTP 尝试写一行 `llm_calls`（结果词：`ok`、
  `error:transport`、`error:auth`、`error:quota`、`error:rate_limited`、
  `error:http_<code>`、`error:bad_response`）。
- 宿主中继为每个 sidecar 上报的 `llm_call` 信封写一行 `llm_calls`，为每个
  被审计的工具结果写一行 `audit_log`（verb 为工具名），并为每个权限决定写
  一行（verb 为 `permission <tool>`，结果为 `approved`、`rejected` 或
  `fail-closed`）。
- CLI 为每次调用写一行 `audit_log`，包括拒绝与预览行，参数预览经过脱敏
  （只有形状与 id，绝无地址、主题、正文文本）。

相关性 id 是每个 agent 轮次生成的 UUID（或由调用方提供），是把这一切绑定
在一起的契约：提问把它带给 sidecar；该轮次的每次工具派生以 CLI 的
`--correlation-id` 标志与子进程的 `ASTRA_CORRELATION_ID` 环境变量携带它；
CLI 把它盖在自己的审计行上；sidecar 的 `llm_call` 信封把它带回宿主账本；
该轮次的每个权限请求与决定都以它审计。离线端到端测试
（`client/desktop/src/agent/e2e_tests.rs`）用真实 bundle 证明这条链：一个
脚本化轮次的权限行、工具行与 `llm_call` 行共享同一个 id，CLI 子进程实际
收到的 argv 与环境变量也携带它。

## 存储层：rusqlite 而非 sqlx（记录在案的决定）

计划点名 sqlx。这在当前工作区结构上不可能：sqlx 0.9 要求
`libsqlite3-sys >= 0.30.1, < 0.38`，而邮件引擎的锁定版本引入 `rusqlite
0.40`，需要 `libsqlite3-sys 0.38.x`；Cargo 每个 resolve 只允许一个带
`links = "sqlite3"` 的 crate，因此有 sqlx 在场工作区甚至无法解析。经过评审
维持的决定：`AiStore` 用 rusqlite 0.40（bundled 构建），即引擎自己的 sqlite
栈，从而只有一个 `libsqlite3-sys` 参与链接，引擎存储与 AI 存储共用同一个
sqlite。迁移为手工实现，每个迁移一个事务并在 `ai_migrations` 表中留一行
簿记；文件命名保持 sqlx 形状（`0001_init.sql`），以便将来迁移引擎。API 是
同步的（互斥锁保护的连接）；构造上就没有 `query!` 宏，也没有构建期
`DATABASE_URL`。

## 后台客户端与 sidecar 循环

后台客户端（`client/desktop/src/ai/`）服务非聊天补全：双协议（openai
`POST {base}/chat/completions` 带 bearer 密钥；anthropic `POST
{base}/v1/messages` 带 `x-api-key` 与 `anthropic-version: 2023-06-01`、
`max_tokens` 1024），链上每条目一次尝试，可重试失败（连接与传输错误、5xx、
429、畸形的 2xx 信封）推进到下一条目，`Retry-After` 按 delta 秒数执行并封顶
两秒，鉴权（401、403）与配额（402、各提供方的余额耗尽报文）对本次调用终局。
错误串只到状态级：响应体绝不流入。A2 之前没有特性消费该客户端；聊天面板的
模型走 sidecar。

sidecar 循环是面板的引擎：宿主生成 `node <sidecar>/index.cjs`（Node 22 或
更新），init 握手交付 cli 路径、数据目录、权限策略，以及**带内**携带密钥的
模型配置（生成环境是 `PATH` 与 `ASTRA_DATA_DIR` 加 Node 的 Windows 平台
最小项的白名单，再无其它，因此任何 `ASTRA_LLM_*` 变量都进不去），轮次事件
以 JSONL 流回，受门控的工具在用户作答的审批卡片上挂起，每次工具调用都经
`safe_exec` 汇入 `astra-cli`。读取自动执行；`save_draft` 与 `discard_draft`
先询问；一切未知一律失败关闭。

## 手动 LLM 冒烟清单

面向作者，对着真实端点执行（连接信息：一把 OpenAI 或 Anthropic 密钥，以及
该账号上的模型名；端点非提供方自营时附兼容 base url）。所有命令在仓库根目录
的 Git Bash 中执行。按设计不存在自动化网络测试；本清单就是实测证明。

1. **准备一次。**

   ```
   npm --prefix sidecar ci
   npm --prefix sidecar run build
   cargo build --release --bin astra-cli
   npm --prefix sidecar run smoke
   ```

   最后一条命令驱动构建出的 bundle 走真实 pi 路径连向不可达端点，必须干净地
   报出失败。

2. **经应用实测聊天。** 导出连接信息，然后运行桌面宿主：

   ```
   export ASTRA_LLM_CHAT=openai:gpt-4o-mini
   export ASTRA_LLM_API_KEY_OPENAI=sk-<your key>
   # 可选，用于兼容端点：
   export ASTRA_LLM_BASE_URL_OPENAI=https://<host>/v1
   cargo run -p astra-desktop
   ```

   （Anthropic 则为：`ASTRA_LLM_CHAT=anthropic:<model>` 与
   `ASTRA_LLM_API_KEY_ANTHROPIC=sk-ant-<your key>`，可选
   `ASTRA_LLM_BASE_URL_ANTHROPIC=https://<host>`。）

   从状态栏打开 Agent 面板，发送 `Say hello in one sentence`，观察流式回复。
   再发送 `Save a draft to bob@example.com about Friday` 并批准卡片；面板显示
   工具结果。

3. **核验账本。** 一个相关性 id 必须贯穿该轮次的各行（此处为 Windows 路径；
   macOS 为 `~/Library/Application Support/com.kenizhou.astra`，Linux 为
   `~/.local/share/com.kenizhou.astra`）：

   ```
   sqlite3 "$APPDATA/com.kenizhou.astra/ai.sqlite" \
     'select correlation_id, use_case, tier, model, outcome from llm_calls order by id desc limit 5;'
   sqlite3 "$APPDATA/com.kenizhou.astra/ai.sqlite" \
     'select correlation_id, verb, outcome from audit_log order by id desc limit 8;'
   ```

4. **单独运行 sidecar（无应用）。** 通过真实 pi harness 手工驱动 bundle（每个
   信封一行；`sleep` 在轮次流式输出期间保持 stdin 打开）。密钥从文件读入，
   shell 历史里留下的那行因此绝不含密钥本身：

   ```
   key=$(cat /path/to/keyfile)
   ( printf '%s\n' \
     '{"type":"init","harness":"pi","cli_path":"<绝对路径>/target/release/astra-cli","data_dir":"<绝对数据目录>","model_configs":{"chat":{"api":"openai-completions","base_url":"https://api.openai.com/v1","model":"gpt-4o-mini","api_key":"'"$key"'"}},"policy":{"list_folders":{"level":"auto"},"list_messages":{"level":"auto"},"read_message":{"level":"auto"},"save_draft":{"level":"confirm"},"discard_draft":{"level":"confirm"}}}' \
     '{"type":"prompt","id":"p-1","session_id":"agent:main","text":"Say hello in one sentence.","correlation_id":"corr-smoke"}' \
     ; sleep 20 ) | node sidecar/dist/index.cjs
   ```

   期望依次出现 `ready`、流式的 `text_delta` 事件、携带 `corr-smoke` 的
   `llm_call` 信封、以及 `done`。

5. **清理。** `unset ASTRA_LLM_CHAT ASTRA_LLM_API_KEY_OPENAI
   ASTRA_LLM_BASE_URL_OPENAI key`（用过 anthropic 拼法则一并 unset）。

**密钥的去向。** 从导出的环境变量进入宿主进程，只在启动时读一次；然后经
stdio 上的 init 信封带内进入 sidecar 内存中的模型注册；再无别处。它绝不进入
sidecar 的生成环境（上述白名单），绝不进入 sidecar 生成的任何子进程
（`safe_exec` 清洗为 `PATH`、`ASTRA_*` 透传，以及注入的数据目录与相关性
id），绝不落入任何账本行或日志行，错误消息只到状态级，响应体无法把它回显。

## 评测框架：推迟至 A1

规则手册要求每次 AI 行为变更配一次评测运行。SPA0 未交付评测框架；它与 A1
的第一批行为化提示词（规则、分类）一同落地。在那之前，产品中唯一的 AI 行为
是 sidecar 的技能指引与上述注册表语义，二者都由离线测试钉住，对它们的任何
变更只经评审而无评测网。这是记录在案的缺口，不是沉默。

## 已知缺口

- **单一模型注册。** pi harness 只注册交付的第一个层级的模型；一个聊天
  模型，无按会话切换。
- **无流式客户端。** 后台客户端只有补全；流式随 A2 的聊天界面到来（sidecar
  自己的文本今天已流式）。
- **DLP 仅允许。** `DlpPolicy::resolve` 对所有策略都答允许；redact 与
  block 随 A1 落地。类型先在，调用方按最终形态编写。
- **CLI 第二写入方接缝。** `astra-cli` 与运行中的应用打开同一个
  `astra.sqlite`（WAL、忙超时、引擎的 outbox 租约保证写入方互不破坏），但
  应用不会被推送：它在下一次同步或刷新时才得知 CLI 的草稿。
- **pi 锁定版本的 npm audit。** `@earendil-works/pi-coding-agent` 0.84.2
  （kylins 对齐锁定）的传递依赖树中有 1 条 moderate、2 条 high 公告。
  sidecar 由宿主生成，只与配置的模型端点通信。
- **`--replacing` 之前的重复草稿。** `drafts save` 无替换标志，同一稿件的
  第二次保存会在第一份之侧再存一份。
- **畸形 init 静默丢弃。** 出现但畸形的 init 行被跳过并只在 stderr 警告；
  宿主观察到的是就绪超时，而非类型化错误。
- **`discard_draft` 门控在 confirm。** 删除草稿在 confirm 级询问一次；因
  Astra 除草稿外不交付删除类动词，kylins 的更硬确认被裁掉。
- **单会话相关性归属。** 中继唯一的全局相关性持有者，以及 `any_correlation`
  对无会话请求与工具结果的归属，都是单会话约束；当 A2 带来并发聊天界面时，
  它们将换成按提示词的盖章。
