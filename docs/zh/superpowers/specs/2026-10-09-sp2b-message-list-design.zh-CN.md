# SP2b 消息列表设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-09-sp2b-message-list-design.md`）。

日期：2026-10-09
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 5、15 节）

## 1. 目的

让消息列表面板成真：所选文件夹的行、平铺与线程两种视图、窗口化分页、
未读与旗标显示及切换、单字母头像。阅读（正文、阅读窗）是下一片；本片止于
「列表成为邮件列表」。

## 2. 决策

- **D1**：MailboxList 快照以增量方式长出消息列表：`rows`、`selection`、
  `visible_limit`、`view_mode` 加入 `account_folders`；既有夹具保持有效
  （新字段 serde 默认为空）。
- **D2**：从 mail-calendar 的消息动作按单条形态移植两个 Intent：
  `MarkRead { row, read }` 与 `SetFlagged { row, flagged }`，作用在新增于
  `astra-app::reference` 的 `RowRef` 枚举（`Message(MessageRef)` |
  `Thread(ThreadRef)`）上，带 `from_parts` 构造。批量选择与其余动作（删除、
  归档、移动）随各自切片到来。
- **D3**：列表读取沿 mail-calendar：平铺走 engine `mail_window`、线程走
  `mail_on_threads`，经 SP1 行缓存投影并沿用代际守卫；`ShowMore` 增长窗口
  （PAGE 已是 100）；`SetViewMode` 切换投影并重置窗口。
- **D4**：已读与旗标写入走 engine `edit_mail`，沿 mail-calendar 的「丢弃
  结果、重同步」姿态：变更结果不直接呈现；乐观行状态加重同步发布结果。
- **D5**：前端加 `@tanstack/react-virtual`（kylins 选型）做列表、发件地址
  单字母头像（kylins 模式）、未读加粗、旗标字形、相对日期、视图切换、
  ShowMore 入口；动作暂作用于单个选中行。

## 3. 测试

- 单元（astra-app，内存引擎 + 假件）：选文件夹重建行；ShowMore 增长；视图
  切换重投影并重置；MarkRead 与 SetFlagged 经 edit_mail 写穿且行重发布；
  同步时行缓存失效。
- 线上：带行的快照夹具双侧钉死（Rust json + TS）。
- 前端（vitest）：MessageList 从夹具渲染行（虚拟化窗口）、切换视图派发
  Intent、已读/旗标按钮以正确 RowRef 载荷派发。
- 活体（门控，harness）：harness 向 astra1 的 INBOX 种入三封邮件（经 harness
  SMTP 投递使路径真实）；RefreshMail 后列表测试断言三行、MarkRead 后转已读。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建与 vitest 全绿。
2. 选文件夹显示该文件夹的行；ShowMore 分页；平铺与线程模式都渲染（线程按
   线程根分组并带计数）。
3. MarkRead 与 SetFlagged 的行更新可见且经 RefreshMail 存活（门控活体测试
   验证引擎写入）。
4. `docs/capabilities.md` 行移动（message list 到 partial、threading 因分组
   视图到 partial），zh 镜像与 changelog fragment 同变更。
5. 无协议破坏：既有夹具与线上测试原样通过（仅增量）。
