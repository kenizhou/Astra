# 稍后提醒设计

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** 没有办法把邮件搁置并让它稍后返回。本 kylins 附加新增稍后提醒
（snooze）：选一个唤醒时间，邮件移入 Snooze 文件夹，到唤醒时间它返回原文件夹。
引擎没有 snooze/调度机制，故这是宿主/核心特性：一个设备本地 snooze store、一个
宿主唤醒定时器、两次移动。无引擎改动；不评测/不提升 pipeline。

两个事实决定设计（已对引擎核实）：
- IMAP 移动在目标文件夹合成新 provider key（mail_edit.rs MailEditReceipt：目标
  副本在下次同步时对账）。故移回在同步后按 Message-ID 头重解析邮件；绝不复用
  snooze 时存储的 key。
- 无 Snooze MailboxRole（role.rs）。Snooze 为每账户创建一个「Snooze」文件夹
  （FolderIntent::Create）并移入。

## D1：snooze store

新 crate crates/astra-snooze（复制 astra-ai 的 AiStore：Mutex 下的 rusqlite、内
嵌迁移、带 5s busy timeout 的 open、测试用 open_in_memory）。每封被 snooze 的邮
件一行：id、account_id、message_id（RFC 5322 Message-ID 头，稳定身份）、
provider_key（snooze 时的 key，仅诊断，移动后失效）、wake_at（unix 秒）、
origin_folder（要返回的文件夹 key）、subject、sender、created_at；wake_at 上的
索引。身份跨边界 newtype（AccountId / MessageIdHeader）。方法：record、due(now)
（wake_at <= now）、remove、list（供未来 Snoozed 视图）。store 设备本地；CLI 不
共享（宿主定时器特性）。

## D2：核心 snooze + wake

核心 install_snooze_store（install_analytics_store 形态，
Mutex<Option<Arc<Store>>> 槽；无 AppState 字段，drop-order 陷阱）与两个核心方法：

- snooze_message(account, row, wake_at)：确保账户的 Snooze 文件夹存在（缺则创
  建，解析其 key），读邮件的 Message-ID + 原文件夹，记录条目，然后派发
  MoveMessages 移入 Snooze 文件夹（mail_writes 移动路径）。会话行被拒（移动仅命
  名邮件）。
- wake_due_snoozes(now)：对每个到期条目，同步 Snooze 文件夹（使移动副本对账），
  按 Message-ID 重解析邮件（扫描 engine.messages 过滤到 Snooze 文件夹；
  subject/sender 作回退），派发 MoveMessages 移回 origin_folder（原文件夹不存在
  则 Inbox），移除条目。移动重发布 Surface::MailboxList，故 web 经既有
  surface_changed 循环得知唤醒（无新事件）。

## D3：宿主定时器 + 面 + 契约

宿主 snooze_timer.rs 复制 calendar_timer.rs：SnoozeTimer<H: SnoozeHost> 状态机
（wait/due/wake 缝）、~30-60s tick、调试 interval/limit 覆盖（ASTRA_SNOOZE_*）、
在 setup_host 中紧邻 calendar_timer::spawn 生成，持有 Arc<dyn AppPort> +
Arc<SnoozeStore>（drop-order 安全形态）。每 tick 从 store 读到期条目并经 AppPort
调 wake_due_snoozes(now)；定时器在 store 之上无状态，故重启重读到期条目（应用关
闭期间到期的唤醒在下次启动时触发）。

面：行菜单「Snooze…」入口（单封邮件，sweepItem 形态）打开新 features/snooze/
picker（snoozeStore.ts + SnoozeDialog.tsx，sweep 对话框模式，useDialogFocus），
提供 later today / tomorrow / next week 作为计算时刻。intent 构造器 + wire 类型
走新 services/snooze.ts（绝不在 ipc.ts，在上限）；新 desktop 命令在
commands/snooze.rs 子模块（commands.rs 在上限），在 lib.rs 注册。

契约：新 docs/snooze.md（store、Message-ID 重解析、Snooze 文件夹、唤醒定时器、
关闭应用的唤醒精度）+ zh；docs/list-selection.md 交叉引用；capabilities 行；变
更日志碎片。zh 镜像同提交移动。

## 已知缺口，记录而非静默

- 设备本地：snooze store 不跨设备漫游。
- 应用关闭时的唤醒精度：定时器仅在进程存活时运行；关闭期间到期的唤醒在下次启动
  时触发，而非精确时刻（桌面宿主定时器固有）。
- 自定义唤醒时间是快速后续（原生 datetime-local 复用已存在）。
- 专用 Snoozed 视图/过滤是后续（Snooze 文件夹经窗格可达）。
- 会话 snooze、再 snooze/撤销、CLI/agent snooze 动词是后续。

## 测试先行

每个行为都带着它的红落地。store 测试（record/due/remove 往返、due 查询边界、迁移
声明、畸形行容忍）基于 open_in_memory。假时钟定时器测试（calendar_timer_tests
模式：假 SnoozeHost 记录 Wait/Wake、脚本 now/due、驱动 step()；无真实睡眠）。核
心集成测试（snooze 记录 + 移出；wake 按 Message-ID 重解析 + 移回 + 移除条目；
key 复用回归测试钉住存储 key 不用于移回）。一个门控活 harness 实证
（snooze_proof.rs：创建 Snooze 文件夹、snooze 播种笔记、以未来 now 强制唤醒、刷
新、断言笔记回到 INBOX 且条目消失、撤回文件夹）。wake 体以 now 为参数，故实证
无需真实等待即强制唤醒。