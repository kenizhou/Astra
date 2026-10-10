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

今天交付六个，每个对应一个已存在的界面。语法是两个位置词（`verb
subverb`）加标志，且标志顺序自由：sidecar 应拼写为
`astra-cli [--json] [--correlation-id <id>] <verb> <subverb> [--flag <value> ...]`，
始终传 `--json` 与轮次的 `--correlation-id`，并把两个位置词之后的一切都当作
具名标志。一次调用在文本中、在审计行里都不含糊。

| 动词 | 读取 | 应答的 `data` |
|---|---|---|
| `accounts list` | 数据目录中的账户索引（`accounts.json`） | `accounts`：按索引顺序的 id |
| `folders list --account <id>` | 引擎存储的 mailbox 对象 | `folders`：id、名称、角色、父级、`unread_count`，按侧栏的规范顺序，树被展平，每行携带父级因此嵌套可推导 |
| `messages list --account <id> [--folder <key>] [--limit <n>]` | 引擎存储的窗口化列表读取 | `messages`：key、发件人、主题、日期、未读、草稿、有无附件、预览 |
| `messages read --account <id> --key <k> [--key <k>...]` | 引擎存储的整封邮件读取与缓存正文 | `messages`：信封加 `body.plain` 与 `body.html_present` |
| `drafts save --account <id> --from <a> --to <a,...> [--cc <a,...>] --subject <s> (--plain <text> 或 --file <path> 或 stdin)` | 经引擎的 `put_draft` 写入 | `message_id`、`op`、`state`（`stored` 或 `queued`），已存储时附 `key` |
| `drafts discard --account <id> --key <k>` | 经引擎的 `delete_draft` 写入 | `key`、`op`、`state`（`removed` 或 `queued`） |

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
| 六个动词与固定信封 | ✅ | `cli/src/verbs` |
| 两个变更动词上的 `--preview`，共享一个构建器 | ✅ | `cli/src/verbs/drafts.rs` |
| 每次调用带相关性 id 审计 | ✅ | `cli/src/audit.rs`、`cli/src/verbs/mod.rs` |
| 发送类动词先解析后拒绝，退出码 3 | ✅ | `cli/src/verbs/mod.rs` |
| 退出码 0、1、2、3 | ✅ | `cli/src/envelope.rs` |
| 脱敏的参数预览（形状，绝非文本） | ✅ | `cli/src/audit.rs` |
| 作为桌面安装包的 `externalBin` 分发 | ✅ | `client/desktop/scripts/stage-cli.mjs` |
| 每个动词的 `--json`，版本化 schema | ✅ | schema 固定为 1 |
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
