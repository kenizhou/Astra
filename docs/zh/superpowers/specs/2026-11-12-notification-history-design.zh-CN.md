# 通知历史设计：铃铛及其记录

**状态。** 2026-11-12 定稿。切片自此提交从 main 分出。

**范围。** 下一个 kylins 增补：新邮件通知的历史，置于外壳中的一个铃铛
之后。Astra 已交付整条通知管线（核心队列、投递驱动、到 OS 插件的宿主桥、
设置开关）；缺的恰是被通知内容的记录与一个读取它的面板。它对照 Astra 的
事件/store/标题栏惯用法移植 kylins 的通知历史形态。无引擎改动、无新
intent、无新 IPC 面（ipc.ts 保持其行数上限）。宿主把被通知的批次作为
Tauri 事件发出；web 按设备本地记录它并渲染一个铃铛弹出层。

## D1：记录路径（宿主发出，web 记录）

**宿主。** `NotifyBridge::deliver`（client/desktop/src/notify.rs）在窗口
模式下已持有 `AppHandle`，并按设置开关与窗口焦点门控。在门控通过后
（实际被通知的邮件，kylins 自己的纪律），它发出一个携带被排空批次的
`new-mail` Tauri 事件，与 share.rs 和 mailto.rs 相同的模式（`app.emit`）。
`NewMailNotice` 与 `NewMailBatch`（crates/astra-app/src/notifications.rs）
获得 `Serialize`（增量 serde，硬规则）；载荷是批次的 messages 加其
overflow 计数。无头 `LogNotifier` 路径不发出任何东西（无窗口，无历史）。
发出绝不抑制 OS 通知：历史失败被记录，而非抛出。

**Web。** 一个 `newMailEvents.ts` 挂载（仿 events/mailtoEvents.ts，
StrictMode 安全的 dispose），挂在 App.tsx，监听 `new-mail` 并记录进一个
`notificationHistoryStore`：一个持久化到 localStorage 的 zustand store
（appearance.ts 与滑动动作所用的设备本地惯用法），上限 100、最新在前。
一个条目携带消息 id（account + key，去重键）、账户、key、存在时的发件人、
主题与预览，以及到达时间。记录按 id 去重（被再次通知的消息移到最前，而非
第二行）。store 提供 record（一个批次）、dismiss（一个 id）与 clear。无新
IPC 面、命令或 intent。

**门控决定，记录在案。** 历史记录被通知的邮件（在启用/焦点门控之后），
与 kylins 一致：窗口聚焦或开关关闭时，不触发通知，也不记录任何东西。这是
通知历史，不是新邮件日志；诚实的新邮件与变更判断留在核心队列，绝不在
web 侧重新实现。

## D2：铃铛与面板

一个 `BellIcon` 加入 components/icons.tsx（今天没有）。一个铃铛按钮挂在
TitleBar（TITLE_ICON_CLASS 按钮惯用法），打开一个
`NotificationHistoryPanel` 弹出层。面板最新在前列出条目：发件人与主题
（两者皆无时为一行仅计数），到达时间，以及账户。点击一个条目经既有的
`intents.openMessage({ account, key })` 打开邮件（MessageRef 与通知的
account 和 key 一一对应）并关闭弹出层。每个条目有一个忽略动作；一个
「全部清除」清空历史；无记录时呈现一个空状态。v1 交付 Focus、Dismiss 与
Clear（kylins 的 Mark read / Archive 悬停动作被推迟，直到线程在已加载
列表中，那是它自己的诚实-v1 妥协）。

面板键盘可达（铃铛是真正的按钮，条目是按钮，Escape 关闭弹出层），并使用
常驻的 focus-visible 环与共享的弹出/焦点惯用法。

## D3：契约

docs/background-sync.md 陈述通知规则；本切片在其中加入一段简短的历史子句
（宿主发出被通知的批次，web 按设备本地记录、上限 100，铃铛读取它），并
镜像到 zh。一个新的 Sync 区 capabilities 行落在通知行旁，带其 zh 镜像。
porting-backlog 的「通知历史」条目随切片解决。一个变更日志碎片落地。

## 已知缺口，记录而非隐藏

- 历史仅记录被通知的邮件（门控之后）；聚焦的窗口或禁用的开关不记录任何
  东西。所有到达的新邮件日志是另一个特性。
- overflow 计数随事件而来，但 v1 记录显式的 messages；仅计数的 overflow
  条目被推迟。
- 从面板 Mark read / Archive 被推迟（kylins 仅在线程已加载时启用它们）；
  v1 是 Focus、Dismiss、Clear。
- 历史是设备本地的（localStorage），不跨设备同步，也不在宿主持久化；它
  随运行的 web 视图，如外观与滑动偏好。
- v1 无逐账户过滤芯片（kylins 有）；面板最新在前列出每个账户的通知。

## 测试先行

每个行为都带着它的红落地。Rust：notify_tests.rs 的 recorder-sink 模式钉住
deliver 在通知时发出携带批次的 `new-mail` 事件，且在开关关闭或窗口聚焦时
不发出任何东西（无头路径不发出）。Web：事件挂载测试（mailtoEvents.test.ts
的 Tauri 哨兵模式）把一个批次记录进 store；store 测试钉住 record/按-id
去重/上限-100/最新在前/dismiss/clear 与 localStorage 持久化；面板测试钉住
列表、点击经 mocked ipc 打开邮件（精确的 openMessage 载荷）、dismiss、
clear、空状态与 Escape 关闭。fake timers 下任何测试中途 store 变更都包
act()。无_live_模型证明；离线电池即证明。