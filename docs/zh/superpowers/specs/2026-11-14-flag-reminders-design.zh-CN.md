# 标记提醒设计

**状态。** 2026-11-14 定稿。切片自此提交从 main 分出。

**范围。** 没有办法请求稍后提醒某封邮件。本 kylins 附加新增标记提醒：在邮件上选
一个唤醒时间，到唤醒时间通知用户。邮件不移动也不自动加标（既有 Flag 动词保持独
立）。它复用 snooze 宿主定时器形态，但 wake 更简（无移动、无 Message-ID 重解析：
邮件原地不动，故设置时记录的 provider key 仍是有效链接）。新 astra-reminders
store、宿主提醒定时器、提醒通知事件。无引擎改动；不评测/不提升 pipeline。

## 参考偏差，升级（AGENTS.md）

锁定的 kylins 参考（thread_flags.rs、followups.rs、Follow Up 族）保持到期标记持
久：行保留直到用户清除，徽章标记会话，一次性前端门控触发 toast 一次。本切片交付
较小的唤醒即删除生命周期（snooze 先例）：条目触发时移除。理由：无徽章面的持久行
是不可见的死重；使持久有意义的徽章面是第二切片。这是对参考生命周期的刻意偏差，
在此与 PR 中升级，非静默。kylins 保真项为命名后续：持久到期标记 + 徽章、Clear
reminder 动词、upsert 重设（再标记移动截止）、一次性 toast 门控。参考的红线被遵
守：标记绝不移动邮件、绝不触碰未读计数、不拥有引擎状态。

## D1：提醒 store

新 crate crates/astra-reminders（复制 astra-snooze 的形态，非其表：snooze 的
origin_folder 为 NOT NULL 且其 wake 移动邮件，两者此处皆不适用）。每提醒一行：
id、created_at、account_id、provider_key、wake_at（unix 秒，索引）、subject、
sender。无 origin_folder，v1 无 message_id（邮件不移动，故 provider key 保持有
效；subject/sender 在设置时捕获，故 wake 无需读引擎）。方法 record / due(now)
（wake_at <= now）/ list / remove，同迁移声明与畸形跳过纪律。身份 newtype
（AccountId、ProviderKey）。设备本地；CLI 不共享（宿主定时器特性，snooze 先例）。

## D2：核心 remind + wake

核心 install_reminder_store（install_snooze_store 形态，
Mutex<Option<Arc<Store>>> 槽；lib.rs 在上限，回收数行注释）与：

- remind_message(account, row, wake_at)：拒会话行（snooze 先例），从引擎自身副本
  读邮件 key + subject + sender，记录条目。无移动、无标记编辑。
- wake_due_reminders(now) -> Vec<NewMailNotice>：对每个到期条目构建 NewMailNotice
  （account、key、设置时捕获的 subject/sender，复用 new-mail 载荷使 wire 形状相
  同），移除条目，答 notices。无同步、无重解析、无移动、无面重发布。坏行跳过/移
  除，绝不搁浅批次。

## D3：宿主定时器 + 提醒通知

宿主 reminder_timer.rs 复制 snooze_timer.rs（ReminderHost wait/now/due/wake 缝、
60s tick、ASTRA_REMINDER_SECONDS/LIMIT 调试覆盖、spawn 持 Arcs、在 store 之上无
状态故重启重读到期条目；应用关闭期间到期的唤醒在下次启动触发）。与 snooze 的唯一
结构差异：提醒定时器需 NotifyBridge，而 assemble_state 无法构建（无 AppHandle），
故把 Arc<NotifyBridge> 传入 assemble_state（窗口调用方有 handle；headless 构建
logged()），与 delivery 共享。

新 NotifyBridge::deliver_reminders(batch, enabled) + REMINDER_EVENT = "reminder"
（notify.rs 有余量）：尊重 notifications_enabled，标题「Reminder」，每到期提醒一
个通知，无焦点抑制（显式用户请求在窗口聚焦时也触发；enabled 开关是唯一门控，
Astra 无 DND 设置），发射 NewMailBatch 载荷（overflow 0）。web 增
events/reminderFlow.ts + reminderEvents.ts（new-mail 对的近复制）记入同一
notificationHistoryStore（历史面板无需改：条目带 account + key，点击打开不变）。

## D4：面

行菜单「Remind me…」入口（单封邮件，snoozeItem 形态；会话禁用带理由）打开新
features/reminders/ picker（reminderStore.ts + ReminderDialog.tsx，SnoozeDialog
模式，useDialogFocus）。picker 复用 services/snooze.ts 的 snoozeChoices（纯导出
助手；标签对提醒读起来正确，kylins 参考也复用 snooze 预设数学）。wire 走新
services/reminders.ts（remindMessage 调 remind_message；绝不在 ipc.ts，在上限）。
新 commands/reminders.rs 子模块（commands.rs 在上限，回收一行注释），在 lib.rs 注
册。AppPort 增两方法（app_state.rs 接近上限回收注释行；无 AppState 字段，核心安
装）。

## D5：契约

新 docs/reminders.md（store、无移动 wake、提醒事件、无焦点抑制策略、升级的唤醒即
删除偏差、记录缺口）+ zh；capabilities 行紧邻 snooze；变更日志碎片；
porting-backlog 已交付列表移动。zh 镜像同提交移动。

## 已知缺口，记录而非静默

- kylins 保真生命周期（持久到期标记 + 徽章、Clear reminder、upsert 重设、一次性
  toast 门控）是升级偏差的后续。
- 自定义唤醒时间；会话/批量提醒；ribbon/palette 入口；CLI/agent 动词。
- 设备本地，无跨设备漫游；关闭应用的唤醒在下次启动触发。
- 邮件在 wake 前被移动/删除时的陈旧打开链接（v1 无 Message-ID 重解析；与 new-mail
  通知相同的暴露）。
- 大量提醒同时到期时无摘要折叠；无 DND 设置。

## 测试先行

每个行为都带着它的红落地。store 测试（record/due/remove 往返、due 边界、迁移声
明、畸形跳过、并发首开）基于 open_in_memory。核心测试（remind 从引擎副本记录；
无移动/标记编辑发射的回归；wake 答 notices + 移除行；无 store -> no-op；会话被
拒）。假时钟定时器测试（snooze_timer_tests drive() 模式）。notify 测试
（deliver_reminders 尊重 enabled 开关、忽略焦点、发射批次；通道失败绝不抑制 OS
通知）。命令 + web picker/store/事件测试。一个门控活 harness 实证
（reminder_proof.rs：提交笔记、remind_message(note, 1_000)、以未来 now 强制
wake、断言 notice 带笔记 subject/sender、条目消失、笔记仍在 INBOX 未移动；撤回笔
记）。wake 体以 now 为参数，故实证无需真实睡眠即强制 wake。