# 通知历史计划：铃铛及其记录

规格：docs/superpowers/specs/2026-11-12-notification-history-design.md。
分支 `notification-history` 自文档提交从 main 分出。每个任务：先红、实现、
电池、一个约定式提交。一个契约变更：background-sync.md 新增历史子句连同其
zh 镜像在同一提交，capabilities.md 连同其 zh 镜像移动，porting-backlog 条目
解决，并落一个变更日志碎片。

### 任务 1：特性（D1，D2）

**文件：** crates/astra-app/src/notifications.rs（在 NewMailNotice 与
NewMailBatch 上派生 Serialize，增量）；client/desktop/src/notify.rs
（NotifyBridge::deliver 在启用/焦点门控之后发出携带批次的 `new-mail`
Tauri 事件，share.rs/mailto.rs 的 app.emit 模式；无头路径不发出；历史失败
绝不抑制 OS 通知）；notify_tests.rs（发出钉子）；client/web/src/events/
newMailEvents.ts（StrictMode 安全挂载，mailtoEvents.ts 模式）+
newMailEvents.test.ts；client/web/src/stores/notificationHistoryStore.ts
（zustand + localStorage，上限 100 最新在前，按 account+key 去重，
record/dismiss/clear）+ notificationHistoryStore.test.ts；client/web/src/
components/icons.tsx（BellIcon）；client/web/src/components/layout/
TitleBar.tsx（铃铛按钮）；client/web/src/features/notification-history/
NotificationHistoryPanel.tsx（弹出层：最新在前列出、点击经
intents.openMessage 打开、dismiss、clear all、空状态、Escape 关闭）+
NotificationHistoryPanel.test.tsx；App.tsx（挂载 newMailEvents）。

- [ ] 测试先行（Rust 发出钉子；web 事件挂载、store、面板红）；随后宿主
      发出、事件挂载、store、铃铛与面板。提交
      `feat: a history of new-mail notifications behind a bell`。
- [ ] 行数警戒：每个文件低于 500（面板约 150-200、store 约 100）。ipc.ts
      不动（无新 intent；openMessage 已存在）。无引擎改动。serde 派生是
      增量的。既有 notify/delivery 测试保持绿。

### 任务 2：契约 + 收尾（D3）

**文件：** docs/background-sync.md（历史子句：宿主发出被通知的批次，web
按设备本地记录、上限 100，铃铛读取它）+ docs/zh/background-sync.zh-CN.md；
docs/capabilities.md（通知行旁一个新的 Sync 区行，✅）+
docs/zh/capabilities.zh-CN.md；docs/porting-backlog.md（通知历史条目解决）
+ 其 zh 镜像；docs/changelog/unreleased/notification-history.md
（Platforms: desktop，Bump: minor）。

- [ ] 文档 + zh + 行 + backlog + 碎片。提交
      `docs: the notification history contract`。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。无_live_模型证明。
- [ ] 分支推送；PR（预期 squash）；CI 观察（ci-ok、desktop、sidecar）；
      squash 合并、删分支、同步 main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的记录路径 + D2 的铃铛/面板（T1）；D3 的契约 + 行 + backlog
  + 碎片 + 电池 + PR（T2）。
- 风险：发出仅在启用/焦点门控之后触发（被通知邮件的历史，而非新邮件
  日志），且绝不在无头路径上；历史失败不得抑制 OS 通知；serde 派生是增量
  的（无字段重命名）；store 按 account+key 去重并上限 100 最新在前；点击
  经既有 openMessage intent 打开（无新 intent，ipc.ts 不动）；面板键盘
  可达；既有 notify/delivery 测试保持绿。
- 不评测、不提升 pipeline（无面向模型改动）。无引擎改动。