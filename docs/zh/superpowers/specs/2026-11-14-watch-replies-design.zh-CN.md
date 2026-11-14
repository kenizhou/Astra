# 关注回复设计

**状态。** 2026-11-14 定稿。切片自此提交从 main 分出。

**范围。** 没有办法在你关心的会话收到回复时被告知。本 kylins 附加新增关注回复：
关注一个会话，首条回复触发「Reply received」通知并清除关注（一次性）。关注绝不
移动邮件、不拥有引擎状态（锁定 kylins 参考红线，watch_replies.rs）。设备本地
(account, thread) 成员 store、一个经引擎解析 thread 的 pass 后到达钩子、经桥的回复
通知、watch_replies_enabled 偏好（默认开）。无引擎改动；不评测/不提升 pipeline。

## 关键：衍生 thread 合并时重新命名

衍生 thread 以合并分量中最小的 owned Message-ID 命名（vendor/engine store-sqlite
derived_ops/threading.rs）；当回复带着较小 Message-ID 到达，引擎把每个存储成员重新
命名到新名。以旧 thread id 为键的关注，对着回复新解析的 thread id 匹配，会漏（随机
Message-ID 下回复约半数排在前）。故关注带一个代表性 member_key（设置时捕获的稳定
provider key，reminder 的 provider_key 类别，是链接非引擎状态），检查时两侧都经引
擎解析：批次的 notice key 与每个关注的 member key，每账户一次 mail_by_keys，解析
对解析匹配。成员行在重新命名中存活（其 thread_id 随分量移动），故两侧总是命名当前
真相。此 member-key 重解析入 v1，非后续（无它特性在 plain IMAP 约半数首次回复失败）。
重新命名回归测试（回复 Message-ID 排在原消息之前）钉住它。

## D1：关注 store

新 crate crates/astra-watches（astra-reminders 形态：Mutex 下 rusqlite、内嵌迁移、
带 5s busy timeout 的 open、open_in_memory）。一表：watch_replies（account_id、
thread_id、member_key、created_at、主键 (account_id,thread_id)）。member_key 是设置
时捕获的被关注会话的一个代表性 provider key。方法：watch（幂等 upsert，答是否插
入）、unwatch（DELETE，答是否移除；一次性消费走此同一方法）、list（rowid 序，供未
来设置面）。无 due 逻辑、无定时器（到达 lane 是唯一触发）。畸形身份行跳过，绝不致
命。设备本地；CLI 不共享。

## D2：核心关注动词 + pass 后检查

核心 install_watch_store（install_reminder_store 形态，Mutex<Option<Arc<Store>>>
槽；lib.rs 在上限，回收注释行）与新 watch_replies.rs：watch_thread(RowRef) /
unwatch_thread(RowRef)（如 categories.rs resolve_thread_key 解析 thread key，加
member-key 捕获：thread 行读 mail_on_threads 取最新成员，message 行用自身 key）、
list_watches、check_watched_replies(batch) -> Vec<NewMailNotice>（async：
watch_replies_enabled 偏好门[关 -> 空，不消费]，按账户分组批次 key，每账户一次
mail_by_keys 取批次 key + 关注 member key，各映射到 thread_key，解析对解析匹配，每
命中从首条回复 notice 构建 NewMailNotice，经 unwatch 消费，任何读/store 失败开放
[记录，绝不搁浅 pass 或基础 notice]）。

watch_replies_enabled 偏好（默认开，notifications_enabled 模式：偏好字段 + state
投影 + persist 腿，SetWatchRepliesEnabled SettingsIntent 臂[settings/mod.rs 在上限
回收]，SettingsBlock wire 字段）。

## D3：宿主钩子 + 回复通知 + 面

Boot 安装关注 store（categories 模式：assemble_state 打开 watches.sqlite +
install_watch_store，无定时器、无 AppState 字段，drop 序不动）。WatchPort supertrait
（CategoriesPort 模式，trait AppPort: WatchPort，每 impl 块在自有子文件，E0119 安
全；app_state.rs 在上限）。新 commands/watch_replies.rs（watch_set/watch_clear/
watch_list，commands/reminders.rs 形态；commands.rs 在上限）。生产 HostDelivery::pass
在 drain 后运行 check_watched_replies（它是 async，持 notify Arc；同步 NewMailQueue
commit fold 不动），门控 notifications_enabled，经新 NotifyBridge::deliver_replies
投递答案。

deliver_replies（deliver_reminders 形态）：每命中一个 OS 通知，标题「Reply
received」，正文发件人/主题，无焦点抑制（显式用户请求，reminder 先例；Astra 无
DND），历史经新 reply 事件发射。notice 带回复自身 account/key/from/subject/preview
故点击打开不变；基础「New mail」notice 仍触发（两次通告，kylins 一致）。web
replyFlow/replyEvents（reminderFlow 克隆）记入同一 notificationHistoryStore。

web：services/watchReplies.ts（invoke 包装，绝不在 ipc.ts）、zustand 关注集 store
（从 watch_list 播种，切换时刷新）、watchMenu.ts 项（「Watch replies」/「Unwatch」，
单行，批量禁用带理由，blockSenderMenu 模式；rowMenu.ts 在上限）、General 设置
watch_replies_enabled 开关。

## D4：契约

新 docs/watch-replies.md（红线、一次性生命周期、member-key 重解析机制及原因[衍生
thread 重新命名]、升级偏差、记录缺口）+ zh；capabilities 行；变更日志碎片；
porting-backlog 已交付列表移动。zh 镜像同提交移动。

## 参考偏差，升级（AGENTS.md）

- 回复通知无焦点抑制 + drain 时消费：kylins 仅在基础 toast 触发时运行关注钩子（在
  DND/mute 门后）并在那里消费；本切片遵循已交付 reminder 契约（无焦点抑制，drain
  时消费故聚焦窗口仍诚实消费）。升级，非静默。

## 已知缺口，记录而非静默

- 溢出折叠：一个 pass 中超过 5 条合格到达里的回复折入溢出计数；其 key 不到达 drained
  批次，故检查漏它，关注保持武装直到后续 pass（队列携带 pass 完整新 key 集是升级路
  径）。
- 仅背景 lane：检查走 delivery driver（唯一生产 drain）；手动刷新到达在下次背景
  pass 检查（基础 new-mail notice 的相同暴露）。
- 陈旧成员链接：若代表性被关注消息被移动（IMAP 合成新 key）或删除，关注无法再解析；
  失败开放，关注保持武装但静默（snooze Message-ID 重解析是升级路径）。
- 设备本地、无漫游、无 CLI/agent 动词、无徽章、无设置/调试列表（可选后续）、无多选
  关注（kylins 一致）。

## 测试先行

每个行为都带着它的红落地。store 测试（一次性/幂等/list 序、账户域、迁移竞争）。核
心钩子测试基于 dispatch fakes（脚本带 Message-ID 的原消息、关注它、送达带
In-Reply-To 的回复；内存引擎衍生 thread 故钩子真实解析；重新命名回归：回复
Message-ID 排在原消息之前仍触发；偏好关 -> 无 notice + 关注未消费；未关注 -> 无；
消费后第二条回复 -> 无；解析失败开放；provider-threaded 腿）。notify-bridge 测试
（deliver_replies 尊重 enabled、无焦点抑制、reply 事件）。web 测试（菜单标签翻转 +
批量禁用、服务、设置开关、reply 事件挂载）。一个门控活 harness 实证
（watch_replies_proof.rs：提交原消息、经核心关注 thread、送达带 In-Reply-To 的回复
[选回复 Message-ID 排在前，证明真实 wire 上重新命名存活]、运行 pass + 检查、断言一
条「Reply received」notice 命名回复 + 关注行消失 + 第二条回复无 + 两消息仍在 INBOX
未移动、撤回）。无活模型证明；离线电池是主证明。