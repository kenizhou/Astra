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

今天交付三十二个：十四个邮件、草稿与规则动词，十个 `ai` 动词，七个
`knowledge` 与 `snippets` 动词，以及 `completions` 生成器。语法是两个位置词
（`verb subverb`）加标志，且标志顺序自由：sidecar 应拼写为
`astra-cli [--json] [--correlation-id <id>] <verb> <subverb> [--flag <value> ...]`，
始终传 `--json` 与轮次的 `--correlation-id`，并把两个位置词之后的一切都当作
具名标志。`ai` 存储动词改为以裸位置词接受其余操作数，即其界面钉住的
语法；`knowledge` 与 `snippets` 动词像邮件动词一样经具名标志读写其存储。
`completions` 动词把 shell 词放在第二个位置词槽（`completions <shell>`），
取 bash、zsh、fish、pwsh 之一。一次调用在文本中、在审计行里都不含糊。

| 动词 | 读取 | 应答的 `data` |
|---|---|---|
| `accounts list` | 数据目录中的账户索引（`accounts.json`） | `accounts`：按索引顺序的 id |
| `folders list --account <id>` | 引擎存储的 mailbox 对象 | `folders`：id、名称、角色、父级、`unread_count`，按侧栏的规范顺序，树被展平，每行携带父级因此嵌套可推导 |
| `messages list --account <id> [--folder <key>] [--limit <n>]` | 引擎存储的窗口化列表读取 | `messages`：key、发件人、主题、日期、未读、草稿、有无附件、预览 |
| `messages read --account <id> --key <k> [--key <k>...]` | 引擎存储的整封邮件读取与缓存正文 | `messages`：信封加 `body.plain` 与 `body.html_present` |
| `messages search --query <q> [--account <id>] [--folder <key>] [--limit <n>] [--offset <n>]` | 引擎的搜索面，分页 | `rows`（账户、key、主题、发件人、日期、未读、旗标、有无附件；无正文）、`total`、`offset`、`has_more`、`windowed`。不带 `--account` 时搜索覆盖索引中的每个账户，但排除各账户自己的废纸篓；带账户则覆盖其整个邮箱；文件夹只在命名账户内收窄且必须伴随账户。页默认二十，最多五十 |
| `messages mark-read --account <id> --key <k> [--unread]` | 经引擎发件队列写入 `$seen` 关键字编辑 | `key`、`op`、`state`（`applied` 或 `queued`）；`--preview` 打印确切的编辑 |
| `messages set-flagged --account <id> --key <k> [--clear]` | 经引擎发件队列写入 `$flagged` 关键字编辑 | `key`、`op`、`state`；`--preview` 打印确切的编辑 |
| `messages archive --account <id> --key <k>` | 经引擎发件队列把邮件移入账户的归档 | `key`、`op`、`state`；`--preview` 打印确切的移动 |
| `messages trash --account <id> --key <k>` | 经引擎发件队列把邮件移入账户的废纸篓；可从那里恢复 | `key`、`op`、`state`；`--preview` 打印确切的移动 |
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
| `ai digest <account>` | 审计账本旁最新的已存摘要报（迁移 0010） | `digest`：文档完整的 `content`，附 `period_start`、`period_end`、`created_at` 与生成回合的 `correlation`；没有生成过时为 `null` |
| `ai report <account>` | 最新已存的分析报告（同一存储的 report 类别） | `report`：同样的字段挂在自己的词下；两个类别绝不互串 |
| `ai digest generate` | 无：该拼写被解析后拒绝 | 以 `3` 退出并附模型规则的拒绝消息（见下）；`--preview` 走非变更动词共有的用法错误，因为没有可描述的动作 |
| `ai report generate` | 无：该拼写被解析后拒绝 | 以 `3` 退出并附模型规则的拒绝消息（见下）；`--preview` 走非变更动词共有的用法错误，因为没有可描述的动作 |
| `knowledge list --account <id>` | 审计账本旁的知识存储（0007） | `entries`：id、kind、text、`source_account`、`source_key`、`created_at`，最新优先；条目文本整段携带，与 `ai drafts` 的整正文纪律相同 |
| `knowledge forget --id <id>` | 删除一条存储的知识条目，唯一出口 | `id`、`state: forgotten`；`--preview` 点名确切的遗忘；无条目携带的 id 是点名它的运行时错误 |
| `knowledge extract` | 无：该拼写被解析后拒绝 | 以 `3` 退出并附永不拨号规则的拒绝消息（见下）；`--preview` 走非变更动词共有的用法错误 |
| `snippets list` | 审计账本旁的片段库（0008） | `snippets`：id、title、body、`created_at`、`updated_at`，最近触碰优先；正文整段携带，插入放置的正是它 |
| `snippets create --title <t> (--body <text> 或 --body-file <path>)` | 向片段存储写入一个片段 | `snippet`（id、title、body、库的 stamps）与 `state: created`；`--preview` 打印存储将接收的确切片段；正文只来自一个来源，同时给两个是用法错误 |
| `snippets delete --id <id>` | 删除一条存储的片段 | `id`、`state: deleted`；`--preview` 点名确切的删除；无片段携带的 id 是点名它的运行时错误 |
| `snippets search` | 无：该拼写被解析后拒绝 | 以 `3` 退出并附永不拨号规则的拒绝消息（见下）；排序搜索在应用的编写器选择器里运行，agent 自行过滤 `snippets list` |
| `completions <shell>` | 无：CLI 自身的动词词汇，不读存储、不经引擎 | 默认原样输出补全脚本（`eval "$(astra-cli completions bash)"` 惯用法）；`--json` 下为 `shell` 与 `script`（脚本为 JSON 字符串）。未知或缺失的 shell 是用法错误 |

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
  `drafts discard --preview` 打印确切的删除。四个 `messages` 动作动词打印引擎
  调用将携带的确切 `MailEdit`，同一纪律：`mark-read` 与 `set-flagged` 展示
  关键字变更，`archive` 与 `trash` 展示解析出的目的地。预览不写入任何内容；
  审计行的 outcome 为 `preview`。
- **动作动词止步于行级废纸篓。** `messages mark-read`、`set-flagged`、
  `archive` 与 `trash` 是全部动作家族：不交付永久删除动词（应用层的 delete
  保持不暴露），命名存储中不存在邮件的 key 是运行时错误而非排队的 op，因此
  助手无法对从未移动过的邮件宣称成功。归档与废纸篓目的地的解析方式与应用层
  相同（`astra-app/src/mail_writes.rs`）：先 SPECIAL-USE 角色，再约定文件夹名，
  归档再退到 `\All` 邮箱；两个 crate 互不依赖，因此解析拼写两遍并由两套测试
  钉住，即封闭动作集自身的先例。
- **动作动词应答持久 op。** 每次编辑先经引擎发件队列排队：提供商一腿接收时
  应答说 `applied`，op 等待应用排空时说 `queued`，即 drafts 动词自身的两态
  应答。离线时应答恒为 `queued`，且关键字变更自那一刻起在存储中可见。
- **正文只来自一个来源。** `--plain`、`--file` 或 stdin；同时给两个是用法错误。
- **走存储的动词不经引擎应答。** `rules`、`ai`、`knowledge` 与 `snippets`
  读写 `ai.sqlite` 中审计账本旁的规则表、结果表、回复起草表（草稿池与风格画像、
  摘要缓存）、文档表（摘要报与分析报告）、知识表与片段表，即每次调用都已
  打开的那一个文件：不读邮件、不拨提供商，因此这些动词对冷数据目录的可用性
  与邮件动词对已同步目录的可用性相同。
- **CLI 绝不拨模型。** `ai cold` 与 `ai results` 返回扫描记录下的内容，绝不是
  实时调用：判定在此存在仅因应用的收件扫描审判过该邮件，未审判的邮件应答
  `null`，绝不猜测。`ai drafts`、`ai summary` 与 `ai style` 把同样的诚实延伸到
  回复起草表：池行、缓存行或画像行的存在仅因应用的起草驱动器或拉取记录过它。
  生成接缝是同样的诚实：`rules create` 接受显式的条件句子、不做任何生成，因为
  后台客户端在桌面宿主里，CLI 不持有模型连接且永远不会有。agent 用自己的模型
  自行撰写条件，动词照单存储（接缝记录于 `docs/ai-rules.md`）。
  `ai style rebuild` 把该规则带到它的拒绝边缘：重建是一次模型推断，因此该拼写
  被解析为下文的拒绝，而不是假装推断。A3 辅助拼写把它带得更远（永不拨号规则，
  契约见 `docs/ai-assist.md`）：`knowledge extract` 是对账户邮件的批次补全，
  `snippets search` 是对片段库的排序补全，两者都解析为点名运行之处的同一拒绝；
  `knowledge list` 与 `snippets list` 读取应用落下的内容，自动补全与翻译根本
  没有 CLI 拼写，它们是编写器与阅读窗格的手势。A4 文档拼写以同样方式延伸
  （契约见 `docs/ai-digest.md`）：`ai digest` 与 `ai report` 读取应用面板
  生成并存储的文档，`generate` 拼写解析为点名那一次窗口化调用运行之处的拒绝。
- **封闭动作集刻意拼写两遍。** CLI 的 `CLOSED_ACTIONS` 与应用层的
  `Action::ALL` 在两个互不依赖的 crate 里携带同样的线词；两者各有自己的测试
  钉住，任一处变更都要落进两处。重复的句子与其代价见 `docs/ai-rules.md`。
- **补全生成器读取词汇，而非 clap。** `completions <shell>` 从
  `Verb::name()`、子动词词与每动词标志描述符构建每个脚本，描述符是对 `Verb`
  的穷尽匹配，因此新动词不可能在没有补全词汇的情况下交付。每个脚本还补全
  生成器自身的 shell 参数，在 `completions` 动词的第二个槽位提供四个 shell
  词。创始规格中 clap_complete 的句子已被取代：解析器是手写且无 clap 的，因此脚本改为从
  现有词汇生成，即代码库以测试钉住的单一事实源纪律。标志描述符把解析臂的
  标志拼写第二遍，即封闭动作集自身的先例，由一个漂移测试把两处拼写钉在
  一起。该动词是非变更的（共享门拒绝 `--preview`）且不经引擎应答，即走存储
  动词的纪律，因此对冷数据目录可用。

## 信封与退出码

每次调用在 stdout 打印一个 JSON 信封，唯一的生成器例外见下文。`--json` 固定
单行形式；不带它时终端得到缩进形式、管道得到单行形式，因此两类读者解析的值
始终相同。

```json
{"schema": 1, "ok": true, "data": { ... }}
{"schema": 1, "ok": false, "error": {"code": "usage", "message": "..."}}
```

- `schema` 固定为 `1`。它只作为契约变更移动，sidecar 的解析器与字节固定的测试
  随同一变更移动。
- `error.code` 是 `usage`、`runtime`、`refused` 之一。
- 退出状态跟随代码：`0` 成功、`1` 运行时、`2` 用法、`3` 拒绝。

**唯一的生成器例外：`completions`。** 信封规则只有一个例外，即 sidecar 从不
调用的人类面向补全生成器。`completions <shell>` 默认原样输出脚本，即
`eval "$(astra-cli completions bash)"` 惯用法，并在 `--json` 下把它包进标准
信封（`data` 携带 `shell` 与 `script`，脚本为 JSON 字符串，schema 仍为 `1`）。
面向 agent 的统一信封完好无损，因为 sidecar 从不调用 completions；sidecar
驱动的每个动词都按上文原样打印信封。

**发送类动词先解析后被拒绝。** `send` 与 `queue`（以及之后被划入发送类的任何
词）解析到足以知道被请求了什么，然后以 `3` 退出，消息点明人发规则：由人决定并
发送，应用来发送。拒绝位于参数解析器中，因此任何发送类拼写都永远到不了引擎。

**模型类动词以同样的方式拒绝。** `ai style rebuild` 需要通过一次模型补全从已发
邮件推断风格笔记；CLI 不持有模型连接且永远不会有，因此该拼写被解析为一条拒
绝，点明规则并指向重建运行之处：应用的需求驱动档案读取，每日上限过去后的
下一个起草回合或风格读取会从已发邮件推断新笔记（没有显式的重建命令交付；
契约与对规格可变动词句子的记录偏差见 `docs/ai-replies.md`）。该动词不带
`--preview`，因为没有可描述的动作。退出码 `3`、账本结果 `refused`，行点明
完整的 `ai style rebuild` 拼写。`knowledge extract` 与 `snippets search`
以相同方式拒绝（A3 辅助契约见 `docs/ai-assist.md`）：抽取作为宿主的
`knowledge_extract` 命令在账户最近的邮件上运行，排序搜索作为编写器选择器的
搜索运行，agent 经 `knowledge list` 与 `snippets list` 读取落下的内容。
`ai digest generate` 与 `ai report generate` 同样以相同方式拒绝（A4 文档
契约见 `docs/ai-digest.md`）：生成作为面板的 Generate 按钮在账户窗口化
邮件上运行，agent 经 `ai digest` 与 `ai report` 读取落下的内容。

## 审计

每次调用，无论做了什么，都向数据目录旁的 AI 账本（`ai.sqlite`）追加一行：动词、
脱敏的参数预览、结果（`ok`、`error`、`usage`、`refused`、`preview`），以及轮次的
相关性 id，取自 `--correlation-id`，缺席时生成。参数预览只携带形状：账户与文件夹
id、数量、长度。地址、主题与正文文本绝不进入账本。解析失败的行只记录动词词，
当子动词属于 CLI 自身的词汇时与之相连：调用方键入的位置词绝不进入账本，而
相关性 id 已经把整行绑定到其轮次。账本无法打开或写入时调用不被应答：CLI 打印
运行时错误并以 `1` 退出。

## 权限门控映射

虚拟权限工具是工具面上受门控的条目，其执行悬停在人工审批卡片之后。
sidecar 的桥对策略表中缺席的任何工具一律失败关闭：缺席的行在那里意味着
needs-approval。新增一个工具会迫使本映射、宿主的策略表
（`client/desktop/src/agent/mod.rs` 的 `default_policy()`）与预览词汇
（`client/desktop/src/agent/permissions.rs` 的 `shaped_arg_preview`，
镜像 `cli/src/audit.rs`）为它作答，即 `docs/plugins.md` 为权限陈述的
穷尽匹配先例。

| 工具 | CLI 动词 | 级别 | 脱敏预览形状 |
|---|---|---|---|
| `list_folders` | `folders list` | `auto` | `account=<id>` |
| `list_messages` | `messages list` | `auto` | `account=<id> [folder=<key>] [limit=<n>]` |
| `read_message` | `messages read` | `auto` | `account=<id> keys=<n>` |
| `search_messages` | `messages search` | `auto` | `query_chars=<n> [account=<id>] [folder=<key>] [offset=<n>]` |
| `save_draft` | `drafts save` | `confirm` | `account=<id> from=<n> to=<n> cc=<n> subject_chars=<n> body_chars=<n>` |
| `discard_draft` | `drafts discard` | `confirm` | `account=<id> key=<id>` |
| `mark_read` | `messages mark-read` | `confirm` | `account=<id> key=<id> read=<bool>` |
| `set_flagged` | `messages set-flagged` | `confirm` | `account=<id> key=<id> flagged=<bool>` |
| `archive_message` | `messages archive` | `confirm` | `account=<id> key=<id>` |
| `move_to_trash` | `messages trash` | `confirm` | `account=<id> key=<id>` |

四个读取直接运行；六个写入先询问。预览形状是宿主对一次询问的
`shaped_arg_preview` 输出，即权限审计行携带的脱敏参数预览：只有形状与
存储 id，绝无地址、主题或正文文本，与 CLI 自己的审计行携带同一词汇。
可选字段只在询问的参数携带它们时出现；搜索预览不带 `limit`，因为工具
在 argv 上拼写 `--limit`，但其审批参数只持有查询、可选的账户与文件夹、
以及可选的 offset。三个测试钉住映射：`policy_tests.rs` 把策略表钉在
恰好这十行，`permissions_tests.rs` 钉住每一个预览形状，sidecar 的
`mailToolsMap.test.ts` 钉住十个名字与每一个动词和子动词。

已知缺口，记录而非静默：`confirm_preview` 级别与审批 `preview` 载荷在
协议与桥中端到端存在，但尚无物生产它们。`runGated` 不向
`requestApproval` 传预览，sidecar 从不拼写 `--preview`，
`default_policy()` 不用 `ConfirmPreview`，审批卡片渲染的是询问的原始参数而
非动作预览。把 `--preview` 的输出接上卡片是更深的切片
（confirm_preview 集群），与本表记录的脱敏参数预览不同。

## 存储在哪里

`ASTRA_DATA_DIR` 指定数据目录；未设置时，CLI 使用应用标识符下平台级的用户数据
目录，即桌面宿主经 Tauri 解析的同一目录族。其中 CLI 打开引擎存储
（`astra.sqlite`，与应用打开的是同一个文件）与 AI 账本（`ai.sqlite`，在其旁）。

**第二写入方的接缝，如实陈述。** CLI 与运行中的应用并行，打开同一个引擎存储。
存储是 WAL 模式的 SQLite，带五秒 busy timeout；CLI 的每次写入都经引擎自己的
`put_draft`、`delete_draft` 与 `edit_mail`，处于发件队列的租约与资源串行化之下，
因此两个写入方不会互相破坏。但应用不会被推送：它通过自己的发件箱界面与下一次
同步或刷新得知 CLI 写入的草稿或编辑，正如得知自己排队中的保存一样。CLI 的读取
看到的是最后一次同步的状态，绝不向服务器索取更多。与纯读者相比有一处偏离：引擎的
`open` 即便在读取路径上也会创建并迁移存储，因此 CLI 首次运行在一个应用从未
创建过的数据目录上时会创建 `astra.sqlite` 并带到本引擎的 schema。在应用旁这是
无害的，因为两侧运行同一引擎版本、因此同一套迁移，这也是读动词不需要预置
存储即可应答空结果的原因。这是被记录的已知交互，不是待关闭的缺口。

## 矩阵

| 规则 | 状态 | 位置 |
|---|---|---|
| 三十二个动词与固定信封 | ✅ | `cli/src/verbs` |
| 每个变更动词（messages 动作、drafts、rules、knowledge 与 snippets）上的 `--preview`，各共享一个构建器 | ✅ | `cli/src/verbs/{messages,drafts,rules,knowledge,snippets}.rs` 与 `message_writes.rs` |
| 每次调用带相关性 id 审计 | ✅ | `cli/src/audit.rs`、`cli/src/verbs/mod.rs` |
| 发送类动词先解析后拒绝，退出码 3 | ✅ | `cli/src/verbs/mod.rs` |
| 模型类动词（`ai style rebuild`、`ai digest generate`、`ai report generate`、`knowledge extract`、`snippets search`）先解析后拒绝，退出码 3 | ✅ | `cli/src/verbs/{ai,knowledge,snippets}.rs` |
| 退出码 0、1、2、3 | ✅ | `cli/src/envelope.rs` |
| 脱敏的参数预览（形状，绝非文本） | ✅ | `cli/src/audit.rs` |
| 作为桌面安装包的 `externalBin` 分发 | ✅ | `client/desktop/scripts/stage-cli.mjs` |
| 每个动词的 `--json`，版本化 schema | ✅ | schema 固定为 1 |
| CLI 自行生成规则或拨模型 | N/A | 生成接缝：agent 撰写，CLI 存储 |
| 拒绝之外的发送类动词（不交付） | N/A | 发送类就是拒绝本身 |
| 邮件动作动词（mark-read、set-flagged、archive、trash）与 MCP 搜索契约的作用域 | ✅ | `cli/src/verbs/{message_writes,messages}.rs` |
| 永久删除动词（永不交付；行级废纸篓即上限） | N/A | 应用层的 delete 保持不暴露 |
| 建立在动词之上的虚拟权限工具 | ✅ | 上文的权限门控映射：十个工具钉住动词、级别与预览形状 |
| 来自动词词汇的 shell 补全（`completions <shell>`：bash、zsh、fish、pwsh） | ✅ | `cli/src/verbs/completions.rs`，信封规则的唯一生成器例外 |

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
