> 非规范性译本，仅供审阅；以英文版为准。

# SP2k MCP 服务器设计

日期：2026-10-12
状态：proposed
父规范：`docs/superpowers/specs/2026-08-...-workflow-design`（第 5、15 节）
移植契约：实质移植 mail-calendar 的 `docs/mcp.md`，并与 Astra 自己的安全
规则手册对账（偏差记录于下）。

## 1. 目的

让用户自己机器上的 AI 助手经 Model Context Protocol 读取并操作其邮件：
仅桌面、默认关闭、在用户点名它可触及的账户之前什么也够不到；写入走用
户自己的门，读取走一条什么也不改的门。

## 2. 决策

- **D1（传输）**：双进程形态。MCP 客户端拉起一个 stdio 中继（JSON-RPC
  2.0，换行分隔；一个不识协议的字节管道），在 Windows 上经命名管道
  （其他桌面走 Unix 域套接字，同一代码路径）桥接到应用；应用侧的监听
  器持有存储、活动连接与凭据，绝不出现第二个写者。
- **D2（门禁）**：默认关闭，位于 Settings 的 Advanced。账户允许清单默认
  为空且空即什么也不暴露：未列出的账户连名字都不告知客户端。端点仅同
  登录 OS 用户可达，对端经核验。开启服务器与授予账户是两个决定。
- **D3（工具集）**：`list_accounts`、`list_folders`、`list_messages`、
  `search_messages`、`get_message`、`mark_read`、`set_flagged`、
  `archive_message`、`move_to_trash`、`create_draft`。snake_case、
  draft-2020-12 schema、`additionalProperties: false`、注解如实；
  `account` 与 `key` 始终同行（错账户路由类在边界上不可表达）。
  **对 mail-calendar 契约的三处偏差，均已记录**：完全没有
  `send_message`，连开关都没有，因为 Astra 的规则手册拒绝向代理开放
  发送类动词而由人决定并发送（升级说明随 PR）；没有 `mark_as_spam`，
  因为 Astra 的分发不携带垃圾邮件动词。`add_account`、
  `open_account_setup` 与 `permanently_delete` 因 mail-calendar 自身的
  理由保持裁剪。第三：mail-calendar 跨重启持久化启用标志与账户允许
清单；Astra 每次启动都重新关门（关且空），因为规则手册 fail
closed 而此端点是助手能获得的最广触达（升级说明随 PR）。
- **D4（两扇门）**：写入分发 UI 所分发的同一批 `Intent`（助手的归档发生
  在用户自己的列表里，可见）。读取经一条什么也不改的查询层：不重发布
  快照、不移动选择、不改写查询，`OpenMessage` 的服务端已读标记绝不触
  发。保障测试钉住这道分界，后来的改动无法合并回去。
- **D5（正文规则）**：正文只出自 `get_message`，一次一条；构造上即纯文
  本；以 `<untrusted-message-content>` 围栏加一行前言，自行闭合的围栏
  被中和；附件字节不过界（只有名字）；不存在不可逆原语。
- **D6（台账）**：每次工具调用以关联 id 落入审计台账，只有计数、id 与
  时长：工具名、结果、时长、查询的字符数。绝不是查询、地址、主题或正
  文。
- **D7（create_draft）**：草稿在应用自己的撰写器中为人打开，复用代理
  sidecar 的批准卡缝；什么都不发送。

## 3. 测试

- 单元（astra-app，引擎 fake）：查询层的保障（读取不发布、不标记、不
  移动；与说明存在原因的对照测试成对）；围栏与纯文本构造；允许清单的
  空即不暴露（list_accounts 什么都不答，连数量都没有）；工具 schema 字
  面量。
- 服务器（进程内）：JSON-RPC 帧、工具分发、错误信封、对端核验、台账
  行。
- E2E（门控）：中继二进制经真实管道桥接到监听器；客户端的
  `initialize`/`tools/list`/`tools/call` 往返；允许清单门禁实测；一次写
  入在应用自己的列表中可见。
- 实测（门控，harness）：经真实 IMAP 邮件的 `get_message` 正文；经播种
  语料的 `search_messages`。

## 4. 已知缺口

- 中继二进制先交付 Windows（命名管道）；Unix 套接字路径可编译但在有
  Unix 宿主 CI 之前未实测。
- 无 `send_message`、无垃圾邮件工具（上述记录的偏差）。
- 撰写器的草稿交接复用代理 sidecar 的缝；撰写器关闭时创建的草稿在下
  次聚焦时打开，而非立即（面板自身的时机）。
- 允许清单列账户而非文件夹：暴露的账户暴露应用为其读取的每个文件夹
  （mail-calendar 的行级门禁随其文件夹级 ACL 工作到来）。
- 归档目标依序解析：RFC 6154 的 `\Archive` 角色、惯例文件夹名（层级名
  的叶、大小写不敏感、覆盖 mail-calendar 所交付的语言），最后才是账户
  的 `\All` 邮箱（Gmail 形态：归档即移除 Inbox 标签，All Mail 是归档
  邮件的家）。垃圾箱同法解析，但无 All 兜底。该解析在此定案、在
  astra-app 实现一次，意图、代理工具与未来的滑动手势共享同一答案。
- 代理的新草稿以撰写请求快照的 reply 种类打开、不带线程头：快照词汇
  早于代理草稿、没有"新建"拼写，而下游撰写器以（此处缺席的）线程头
  键控线程。代理草稿的 `bcc` 出于同一原因不做预填（快照没有 Bcc 字段）；
  撰写器打开时如实显示将要发送的内容，且在人类按下发送之前什么也不
  发。两者待撰写请求契约长出代理形态时一并收窄。
- 服务器到 `App<P>` 的端口（astra-mcp 中的 trait 实现）按设计只做薄委
  托：其行为钉在它委托的各层（astra-app 中查询层与写入的测试、经假端
  口的桥与服务器测试），由证明任务的门控 e2e 经真实管道驱动。
- 应用不可达时中继以非零码退出并在 stderr 留下诊断，而不是回答
  mail-calendar shim 所答的 JSON-RPC "unavailable" 帧：该帧需要请求
  id，而读取 id 正是此中继拒绝持有的协议知识。客户端报告服务器失
  败；打开应用即修复。
- Unix 传输半边（0700/0600 权限、陈旧套接字认领、对端 uid 拒绝）已
  实现但无处可跑地钉住：本宿主是 Windows，cfg(unix) 测试随首个
  Unix CI 落地。
