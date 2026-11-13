# 标记提醒计划

规格：docs/superpowers/specs/2026-11-14-flag-reminders-design.md。分支
`flag-reminders` 自规格提交从 main 分出。每个任务：先红、实现、电池、一个约定式
提交。一个契约变更：新 docs/reminders.md 连同其 zh 镜像在同一提交，capabilities
行连同其 zh 镜像移动，落变更日志碎片，porting-backlog.md 把 flag reminders 移入
已交付列表。无引擎改动。唤醒即删除生命周期是升级的参考偏差（规格陈述；PR 升级）。

### 任务 1：提醒 store + 核心 remind/wake（D1，D2）

**文件：** 新 crate crates/astra-reminders（Cargo.toml + src/lib.rs +
src/store.rs + src/migrations/0001_init.sql，复制 astra-snooze 形态；提醒表
id/created_at/account_id/provider_key/wake_at/subject/sender，wake_at 索引；
record/due/list/remove，迁移声明 + 畸形跳过）+ store 测试；
crates/astra-app/src/reminders.rs（install_reminder_store、remind_message：拒会
话 + 读引擎副本 + 记录，无移动/标记；wake_due_reminders(now) ->
Vec<NewMailNotice>：due -> notices -> remove）+ 集成测试；App 槽（lib.rs 在上限，
回收约 4 行注释）。

- [ ] 在钩子处确认：astra-snooze store 形态（store.rs）、install_snooze_store 槽
      （snooze.rs:69，lib.rs:359）、引擎副本读（snooze.rs:100-126）、NewMailNotice
      （notifications.rs:43-65）。测试先行（store：record/due/remove 往返、due 边
      界、迁移声明、畸形跳过、并发首开；核心：remind 从引擎副本记录、无移动/标记
      编辑发射的回归、wake 答 notices + 移除行、无 store -> no-op、会话被拒）；随
      后 store + 核心。提交 `feat: the reminder store and wake core`。
- [ ] astra-reminders 不豁免上限（仅四个加密 crate 豁免）；文件保持 500 内。
      astra-app 不豁免（新子模块；lib.rs 在上限，回收注释行，不超 500）。跨边界
      newtype。既有测试保持绿。

### 任务 2：定时器 + notify + 面 + 契约 + 活实证（D3，D4，D5）

**文件：** 新 client/desktop/src/reminder_timer.rs（复制 snooze_timer.rs：
ReminderHost 缝、60s tick、ASTRA_REMINDER_* 覆盖、spawn 持 Arcs）+ 假时钟测试；
notify.rs deliver_reminders + REMINDER_EVENT（无焦点抑制）；boot 接线（把
Arc<NotifyBridge> 传入 assemble_state、打开 reminders.sqlite、
install_reminder_store、紧邻 snooze 生成；headless logged()）；AppPort 两方法
（app_state.rs 接近上限回收注释行、port.rs、fakes、showcase）；新
commands/reminders.rs（commands.rs 在上限回收一行注释）在 lib.rs 注册；新
client/web/src/features/reminders/（reminderStore.ts + ReminderDialog.tsx，
SnoozeDialog 模式，复用 snoozeChoices）+ 测试；新 services/reminders.ts
（remindMessage，绝不在 ipc.ts）；行菜单 remindItem（rowMenu.ts）；
events/reminderFlow.ts + reminderEvents.ts + App.tsx 挂载喂 notificationHistoryStore；
docs/reminders.md + zh；docs/capabilities.md 行 + zh；
docs/changelog/unreleased/flag-reminders.md（Platforms: desktop，Bump: minor）；
docs/porting-backlog.md + zh；crates/astra-app/src/tests_live_harness/
reminder_proof.rs（提交笔记、remind、以未来 now 强制 wake、断言 notice
subject/sender + 条目消失 + 笔记仍在 INBOX 未移动、撤回）+ tests_live_harness.rs
注册。

- [ ] 测试先行（假时钟定时器测试；notify deliver_reminders 测试；picker/store/事
      件测试；离线跳过的门控活实证）；随后定时器 + notify + 面 + 文档。提交
      `feat: the flag reminder surface and contract`。
- [ ] client/desktop/web 不豁免：commands.rs 在上限（commands/reminders.rs 子模
      块，回收一行注释）；app_state.rs 499（回收约 7 行注释，无 AppState 字段，核
      心安装）；commands_fakes/port.rs 491（fake 注释保持一行）；ipc.ts 在上限
      （services/reminders.ts）。提醒事件不继承焦点抑制。vitest 陷阱：act() 测试
      中段，整体 mock 外层函数。注释现在时；无 em/en 破折号；英式英语；源文件以换
      行结尾（CI rustfmt 比本地严）。docs/zh 镜像同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；cargo
      clippy --workspace --all-targets -- -D warnings；cargo test --workspace；
      npm --prefix sidecar run build && npm --prefix sidecar test；npx tsc
      --noEmit && npx vitest run（client/web）；cargo run -p xtask -- gate。若
      plugin 测试以 AlreadyGranted 失败，清理陈旧 /tmp/astra-desktop-plugin-*
      scratch 目录（Windows pid 回收 flake）再重跑。留意磁盘空间（增量缓存膨胀则
      清理）。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后 up +
      env；cargo test -p astra-app tests_live_harness::reminder_proof --
      --test-threads=1），随后 unset。
- [ ] 分支推送；PR（预期 squash）在正文升级唤醒即删除参考偏差；CI 观察；squash 合
      并、删分支、同步 main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的 store + D2 的核心（T1）；D3 的定时器/notify + D4 的面 + D5 的契
  约 + 活实证 + 电池 + PR（T2）。
- 风险：升级的唤醒即删除参考偏差（kylins 保持持久行；记录 + 升级，kylins 保真后续
  命名）；三文件处 499-500（共回收约 12 行注释）；boot 接线（NotifyBridge 传入
  assemble_state）；无焦点抑制策略；无移动/无重解析 wake（provider key 保持有
  效）。设备本地、关闭应用唤醒精度、陈旧打开链接（记录缺口）。
- 不评测/不提升 pipeline（面向模型不变）。活实证教训：先 reset harness；wake 体以
  now 为参数故实证无需睡眠即强制 wake；若电池 flake 清理陈旧 plugin scratch 目录；
  确保末尾换行（CI rustfmt）。