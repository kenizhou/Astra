> 非规范性译本，仅供审阅；以英文版为准。

# CLI：Astra Mail 契约

**范围。** `astra-cli` 二进制：AI sidecar 执行的、agent 优先的工具调用入口，
它的动词、`--json` 信封、变更动词上的 `--preview`、每次调用的审计，以及调用方
用于分发的退出码。对每一个驱动该 CLI 的界面（今天的 sidecar `safeExec`；明天
任何其它通过 shell 调用它的程序）都有约束。规则手册的 CLI 一节陈述常设政策；
本文档是已交付动词所遵循的契约。

**原则。** *CLI 是 agent 被允许对邮件做的事，仅此而已。* 每个动词都是读取或
起草；发送始终是人的动作；每一次调用，包括被拒绝的调用，都会在审计账本中留下
一行，绑定其轮次的相关性 id。未经审计的调用根本不会被应答：进程宁可直接失败，
也不为账本不知道发生过的事情打印信封。

## 动词

今天交付十五个：九个邮件、草稿与规则动词，加六个 `ai` 动词。语法是两个位置词（`verb
subverb`）加标志，且标志顺序自由：sidecar 应拼写为
`astra-cli [--json] [--correlation-id <id>] <verb> <subverb> [--flag <value> ...]`，
始终传 `--json` 与轮次的 `--correlation-id`，并把两个位置词之后的一切都当作
具名标志。九个走存储的动词（`rules` 与 `ai`）改为以裸位置词接受其余操作数，
即其界面钉住的语法。一次调用在文本中、在审计行里都不含糊。

| 动词 | 读取 | 应答的 `data` |
|---|---|---|
| `accounts list` | 数据目录中的账户索引（`accounts.json`） | `accounts`：按索引顺序的 id |
| `folders list --account <id>` | 引擎存储的 mailbox 对象 | `folders`：id、名称、角色、父级、`unread_count`，按侧栏的规范顺序，树被展平，每行携带父级因此嵌套可推导 |
| `messages list --account <id> [--folder <key>] [--limit <n>]` | 引擎存储的窗口化列表读取 | `messages`：key、发件人、主题、日期、未读、草稿、有无附件、预览 |
| `messages read --account <id> --key <k> [--key <k>...]` | 引擎存储的整封邮件读取与缓存正文 | `messages`：信封加 `body.plain` 与 `body.html_present` |
| `drafts save --account <id> --from <a> --to <a,...> [--cc <a,...>] --subject <s> (--plain <text> 或 --file <path> 或 stdin)` | 经引擎的 `put_draft` 写入 | `message_id`、`op`、`state`（`stored` 或 `queued`），已存储时附 `key` |
| `drafts discard --account <id> --key <k>` | 经引擎的 `delete_draft` 写入 | `key`、`op`、`state`（`removed` 或 `queued`） |
| `rules list` | 审计账本旁的规则存储 | `rules`：id、名称、条件、线词形式的 `actions`、enabled、`created_at`；存储的载荷超出封闭集时该动词响亮失败并点名规则 |
| `rules create --name <n> --condition <c> --action <word> [--action <word>...]` | 向规则存储写入一条规则，启用 | `rule`（完整的存储规则）与 `state: created`；`--preview` 打印存储将接收的确切规则 |
| `rules delete --id <id>` | 删除一条存储规则 | `id`、`state: deleted`；`--preview` 点名确切的删除；无规则携带的 id 是点名它的运行时错误 |
| `ai cold <account> <key> [<key>...]` | 结果存储中已存的冷判定 | `messages`：每 key，最新 `cold` 行的 `verdict`，附判定它的 `correlation` 与 `recorded_at`；无扫描审判过该邮件时为 `null` |
| `ai results <account> <key> [<key>...]` | 结果存储中已存的阶段行 | `messages`：每 key，全部行最新优先，含 `stage`、`rule_id`、`verdict`、`category`、`correlation`、`recorded_at` |
| `ai drafts <account> <key> [<key>...]` | 审计账本旁的草稿池行（迁移 0004） | `messages`：每 key，全部池行最新优先，含 `correlation`、完整的 `reply`（模型生成的文本，即该动词存在的目的，与 `messages read` 的整正文纪律相同）、`confidence`、`skipped`、`created_at`；未被生成触碰过的邮件应答空列表 |
| `ai summary <account> <key> [<key>...]` | 审计账本旁的摘要缓存行（迁移 0005） | `messages`：每 key，缓存的 `summary`，附缓存它的 `body_hash` 与 `created_at`；无缓存时为 `null`。该行是否仍然命中是应用在服务时的正文哈希检查，本读取对此不做断言 |
| `ai style <account>` | 已存的风格画像（0004 的姊妹表） | `style`：画像的 `note` 与 `built_at`；从未构建过时为 `null` |
| `ai style rebuild` | 无：该拼写被解析后拒绝 | 以 `3` 退出并附模型规则的拒绝消息（见下）；`--preview` 走非变更动词共有的用法错误，因为没有可描述的动作 |

约束每个动词的规则：

- **账户 id 来自索引。** 命名账户的动词在 id 不在账户索引中时以运行时错误失败。
  CLI 永不打开 OS 钥匙串（每个账户的配置，即凭据，存放处），因此它永远不会知道
  密码，也永远不会拨号某个账户。
- **按文件夹过滤的列表使用应用自己的列表策略。** `--folder` 在一个超伸的窗口
  （`limit` 乘四、下限 500，即 `astra-app/src/window.rs` 使用的数字）上读取窗口化
  列表，并把该文件夹的行切到 `limit`，因此当账户最新的邮件存放在别处时，一个
  文件夹的页面不会短缺。
- **正文来自存储的缓存。** `messages read` 从缓存的源中提取正文；一封从未在
  应用中预热过正文的邮件使该动词以运行时错误失败，而不是通过 CLI 并不持有的
  网络去抓取。未消毒的 HTML 备选只报告其存在（`html_present`）而绝不输出：
  agent 读纯文本形式，渲染 HTML 是宿主的职责。
- **预览打印动作本身，来自执行它的同一个构建器。** `drafts save --preview`
  打印引擎调用将携带的那份草稿（地址、主题与正文长度，绝非正文文本）；
  `drafts discard --preview` 打印确切的删除。预览不写入任何内容；审计行的
  outcome 为 `preview`。
- **正文只来自一个来源。** `--plain`、`--file` 或 stdin；同时给两个是用法错误。
- **走存储的动词不经引擎应答。** `rules` 与 `ai` 读写 `ai.sqlite` 中审计账本旁
  的规则表、结果表与回复起草表（草稿池与风格画像、摘要缓存），即每次调用都已
  打开的那一个文件：不读邮件、不拨提供商，因此这些动词对冷数据目录的可用性与
  邮件动词对已同步目录的可用性相同。
- **CLI 绝不拨模型。** `ai cold` 与 `ai results` 返回扫描记录下的内容，绝不是
  实时调用：判定在此存在仅因应用的收件扫描审判过该邮件，未审判的邮件应答
  `null`，绝不猜测。`ai drafts`、`ai summary` 与 `ai style` 把同样的诚实延伸到
  回复起草表：池行、缓存行或画像行的存在仅因应用的起草驱动器或拉取记录过它。
  生成接缝是同样的诚实：`rules create` 接受显式的条件句子、不做任何生成，因为
  后台客户端在桌面宿主里，CLI 不持有模型连接且永远不会有。agent 用自己的模型
  自行撰写条件，动词照单存储（接缝记录于 `docs/ai-rules.md`）。
  `ai style rebuild` 把该规则带到它的拒绝边缘：重建是一次模型推断，因此该拼写
  被解析为下文的拒绝，而不是假装推断。
- **封闭动作集刻意拼写两遍。** CLI 的 `CLOSED_ACTIONS` 与应用层的
  `Action::ALL` 在两个互不依赖的 crate 里携带同样的线词；两者各有自己的测试
  钉住，任一处变更都要落进两处。重复的句子与其代价见 `docs/ai-rules.md`。

## 信封与退出码

每次调用在 stdout 打印一个 JSON 信封。`--json` 固定单行形式；不带它时终端得到
缩进形式、管道得到单行形式，因此两类读者解析的值始终相同。

```json
{"schema": 1, "ok": true, "data": { ... }}
{"schema": 1, "ok": false, "error": {"code": "usage", "message": "..."}}
```

- `schema` 固定为 `1`。它只作为契约变更移动，sidecar 的解析器与字节固定的测试
  随同一变更移动。
- `error.code` 是 `usage`、`runtime`、`refused` 之一。
- 退出状态跟随代码：`0` 成功、`1` 运行时、`2` 用法、`3` 拒绝。

**发送类动词先解析后被拒绝。** `send` 与 `queue`（以及之后被划入发送类的任何
词）解析到足以知道被请求了什么，然后以 `3` 退出，消息点明人发规则：由人决定并
发送，应用来发送。拒绝位于参数解析器中，因此任何发送类拼写都永远到不了引擎。

**模型类动词以同样的方式拒绝。** `ai style rebuild` 需要通过一次模型补全从已发
邮件推断风格笔记；CLI 不持有模型连接且永远不会有，因此该拼写被解析为一条拒
绝，点明规则并指向重建运行之处：应用里。该动词不带 `--preview`，因为没有可
描述的动作。退出码 `3`、账本结果 `refused`，行点明完整的 `ai style rebuild`
拼写。

## 审计

每次调用，无论做了什么，都向数据目录旁的 AI 账本（`ai.sqlite`）追加一行：动词、
脱敏的参数预览、结果（`ok`、`error`、`usage`、`refused`、`preview`），以及轮次的
相关性 id，取自 `--correlation-id`，缺席时生成。参数预览只携带形状：账户与文件夹
id、数量、长度。地址、主题与正文文本绝不进入账本。解析失败的行只记录动词词，
当子动词属于 CLI 自身的词汇时与之相连：调用方键入的位置词绝不进入账本，而
相关性 id 已经把整行绑定到其轮次。账本无法打开或写入时调用不被应答：CLI 打印
运行时错误并以 `1` 退出。

## 存储在哪里

`ASTRA_DATA_DIR` 指定数据目录；未设置时，CLI 使用应用标识符下平台级的用户数据
目录，即桌面宿主经 Tauri 解析的同一目录族。其中 CLI 打开引擎存储
（`astra.sqlite`，与应用打开的是同一个文件）与 AI 账本（`ai.sqlite`，在其旁）。

**第二写入方的接缝，如实陈述。** CLI 与运行中的应用并行，打开同一个引擎存储。
存储是 WAL 模式的 SQLite，带五秒 busy timeout；CLI 的每次写入都经引擎自己的
`put_draft` 与 `delete_draft`，处于发件队列的租约与资源串行化之下，因此两个
写入方不会互相破坏。但应用不会被推送：它通过自己的发件箱界面与下一次同步或
刷新得知 CLI 写入的草稿，正如得知自己排队中的保存一样。CLI 的读取看到的是
最后一次同步的状态，绝不向服务器索取更多。与纯读者相比有一处偏离：引擎的
`open` 即便在读取路径上也会创建并迁移存储，因此 CLI 首次运行在一个应用从未
创建过的数据目录上时会创建 `astra.sqlite` 并带到本引擎的 schema。在应用旁这是
无害的，因为两侧运行同一引擎版本、因此同一套迁移，这也是读动词不需要预置
存储即可应答空结果的原因。这是被记录的已知交互，不是待关闭的缺口。

## 矩阵

| 规则 | 状态 | 位置 |
|---|---|---|
| 十五个动词与固定信封 | ✅ | `cli/src/verbs` |
| 每个变更动词（drafts 与 rules）上的 `--preview`，各共享一个构建器 | ✅ | `cli/src/verbs/{drafts,rules}.rs` |
| 每次调用带相关性 id 审计 | ✅ | `cli/src/audit.rs`、`cli/src/verbs/mod.rs` |
| 发送类动词先解析后拒绝，退出码 3 | ✅ | `cli/src/verbs/mod.rs` |
| 模型类动词（`ai style rebuild`）先解析后拒绝，退出码 3 | ✅ | `cli/src/verbs/ai.rs` |
| 退出码 0、1、2、3 | ✅ | `cli/src/envelope.rs` |
| 脱敏的参数预览（形状，绝非文本） | ✅ | `cli/src/audit.rs` |
| 作为桌面安装包的 `externalBin` 分发 | ✅ | `client/desktop/scripts/stage-cli.mjs` |
| 每个动词的 `--json`，版本化 schema | ✅ | schema 固定为 1 |
| CLI 自行生成规则或拨模型 | N/A | 生成接缝：agent 撰写，CLI 存储 |
| 拒绝之外的发送类动词（不交付） | N/A | 发送类就是拒绝本身 |
| 归档与标签动词 | ⬜ | 随其切片交付 |
| 建立在动词之上的虚拟权限工具 | ⬜ | sidecar 的桥，SPA0 后续任务 |

已知缺口，记录而非静默：

- CLI 从索引文件应答 `accounts list`，因为固定版本上的引擎存储不提供账户枚举，
  而应用在启动时从索引把账户列表保存在内存中。若引擎将来提供存储侧的账户列表，
  它将取代这一文件读取。
- `drafts save` 无法指名它要取代的副本：动词不接受 `--replacing`，因此在回复与
  转发切片带着其线程关联落地之前，同一份撰写的第二次保存会在第一份旁边再存
  一份草稿。
- 默认数据目录在 `tauri.conf.json` 与 CLI 的 `main.rs` 中都以标识符
  `com.kenizhou.astra` 拼写；品牌化时的环境变量统一属于桌面壳而非 CLI，随其
  移动。
