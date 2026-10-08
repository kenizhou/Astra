# SP1 核心架构设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-08-sp1-core-architecture-design.md`）。

日期：2026-10-08
状态：proposed（待审阅）
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 4、5、15 节）

## 1. 目的

SP1 构建核心架构，并用一个垂直切片证明它：添加 IMAP 账户、同步、在桌面壳里
看到文件夹树。后续一切（SP2+ 功能切片、SPA0 AI 基础）都长在本 spec 固定的
接缝里：Intent/Surface 协议、crate 边界、Tauri 宿主端口、IPC 契约、插件宿主
骨架。

## 2. 参考点（来自深度探索）

- 引擎：`allodia-eu/email-calendar-sync-engine` rev `68eab00e...`，九个 crate
  在 workspace 按 git pin（`engine-api`、`engine-core`、`engine-provider`、
  `engine-tls`、`provider-imap`、`provider-caldav`、`provider-graph`、
  `provider-google`、`provider-jmap`）；`engine-api::Engine` 是门面
  （`Arc<Engine>`，方法逐个对 `P: Provider` 泛型）；id 来自 `engine-core`
  （`object_id!` newtype），经 `engine-api` 再导出。
- mail-calendar 核心：`Intent` 派发（`dispatch(intent).await`，每变体一个
  match 臂，家族臂委托）、`Surfaced<T>` 先发布后发信元、
  `AppObserver::surface_changed`、带代际守卫与窗口化行缓存的快照重建、
  引擎持有的 durable outbox（`submit_mail` -> `queued_state` ->
  `SendOutcome`）、boot 序列（`Engine::open` -> `abandon_sync_leases` ->
  `App::new` -> `recover_outbox` -> `prime_snapshot` -> 后台连接与同步）。
- kylins 壳：无边框 Tauri 2 窗口 + 自定义 TitleBar、AppShell 布局、消费
  事件的 Zustand stores、作为 `invoke()` 薄包装的 services、typed 事件镜像、
  CSS 自定义属性主题管理器。

## 3. 决策

- **D1**：Tauri host 扮演 mail-calendar 的 `mailcal-bindings`：拥有 tokio
  runtime（多线程，workers = 核数减一；阻塞式宿主调用走专用大栈线程）并实现
  宿主服务端口。以 crate 方式直接消费 `astra-app`（Linux 客户端先例）。
- **D2**：Astra 的 `Intent` 与 `Surface` 带 serde 派生（mail-calendar 走
  UniFFI 记录过 FFI；Tauri IPC 需要 JSON）。IPC 契约：`dispatch(intent)`
  命令、携带 surface 名的 `surface_changed` 事件、每个 surface 一个拉取命令
  （SP1 只有 `mailbox_list`）。Observer 桥按 mail-calendar 去抖：250ms 窗口，
  MailboxList 前沿、SyncProgress 后沿。
- **D3**：同步轮次归 `astra-app`（逐账户 join_all -> `engine.sync_mail` ->
  `drain_outboxes` -> `rebuild_snapshot`）；host 只拥有 runtime 与后台任务
  spawn。无壳级调度器。
- **D4（Ruling R7）**：SP1 不引入产品 SQLite。持久化 = 引擎 SQLite +
  `preferences.toml` + OS keyring，即 mail-calendar 在此阶段的做法。sqlx
  产品库随 SPA0 在 AI 台账需要时到来。若错，代价是之后一次 schema 迁移，
  不是返工。
- **D5**：垂直切片按两路探索确定的最小集交付（第 5 至 7 节）。
- **D6**：插件宿主在 SP1 落骨架：manifest 类型、生命周期状态、版本化 hook
  API 面作为类型、权限模型声明。跨进程执行与 example-plugin e2e 落在 SP1
  末或 SP2，由计划按余量判断。

## 4. Crate 布局与依赖

```
crates/
  astra-app/         protocol（Intent、Surface、status）、surfaced.rs、
                     dispatch、snapshot + window + row_cache、accounts、
                     sync、boot
  astra-viewmodel/   MailboxListSnapshot、folder 行（叶子 crate：仅依赖
                     engine-api）
  astra-account/     AccountConfig（TOML）、setup -> build_config_toml、
                     IMAP 连接与绑定、重连包装、凭据源、TLS、证书例外类型
  astra-plugin/      manifest、生命周期、hook API v1 类型、权限
client/
  desktop/           Tauri host：runtime、端口（keyring 凭据存储、observer
                     桥）、命令、事件、boot
  web/               React SPA（第 7 节）
```

`astra-app` 依赖 `astra-viewmodel`、`astra-account`、`engine-api`、
`engine-provider`、`engine-core`。`client/desktop` 依赖 `astra-app` 加 Tauri
与 keyring crate。workspace 依赖表从 mail-calendar 集合起步、去掉 UniFFI
（核心 crate 用 tokio sync/time；futures；async-trait；serde/serde_json；
toml；thiserror；IMAP 的 rustls 栈；time；log），按需增长。

## 5. 协议 v1

- SP1 的 Intent 变体：`RefreshMail`、`SelectAccount(Option<AccountId>)`、
  `SetAccountExpanded { account, expanded }`、`SetFolderExpanded { folder,
  expanded }`、`SelectFolder { folder }`、`SetViewMode(ViewMode)`、
  `ShowMore`，加上家族形态（`Folders(FolderIntent)`），其中
  `FolderIntent::Create` 预留但不在 SP1 派发。全部 serde，外部标签枚举、
  snake_case、省略 `None`（与前端已镜像的引擎事件线上风格一致）。
- 组合引用在 `astra-app::reference`（`FolderRef { account, key }`），
  `from_parts` 在残缺时返回 `None`。
- SP1 的 Surface：`MailboxList`（推送快照）与 `SyncProgress`（仅信号，
  拉取）。`Surfaced<T>` 原样移植（先发布后发信；`get()` 克隆且永不发信）。
- 快照：`MailboxListSnapshot { account_folders: Vec<AccountFolderRow> }`，
  每账户带 `expanded`、未读计数、角色图标数据。
- `preferences.toml` 中的偏好：视图模式、文件夹展开状态、选中文件夹；
  boot 时加载，对应 Intent 时写入。

## 6. Tauri host 设计

- Builder：single-instance 最先、deep link（预留 `mailto:`）、opener、
  dialog、notification；kylins 几何的无边框主窗（1200x800，最小 800x600）。
  SP1 无托盘、无自启、无全局快捷键（随设置切片到来）。
- 命令：`dispatch`、`mailbox_list`、`sync_progress`，加账户设置：
  `detect_account_settings`（先以 astra-account 的手动 IMAP 推荐作桩；
  自动探测策略随 SP2 账户切片移植）、`add_account(config_toml)`、
  `imap_test_connection`。
- 事件：`surface_changed`（只有名字；客户端拉取）；保留名 `engine:*` 不用：
  app 是引擎的唯一消费者。
- 凭据存储端口：`AccountCredentialStore` 以 OS keyring 实现，作为必需构造
  参数注入、绝不作 setter；添加失败回滚注册。
- 退出：`RunEvent::Exit` 有界等待后停止同步。

## 7. React 壳（最小）

- `AppShell`（带拖拽区与窗口控件的 TitleBar、FolderPane 树、MessageList
  占位、StatusBar）、CSS 自定义属性主题包（明暗）、stores
  `accountStore`、`folderStore`、`uiStore`、`invoke()` 薄包装 services、
  `surface_changed` 的 typed 事件客户端、以首次 `dispatch(RefreshMail)`
  收尾的账户设置流程。
- kylins 的插件 slot-injection 运行时不在 SP1 接线；`astra-plugin` 类型
  仅编译并做单元测试。

## 8. 测试

- Rust：按规则书内联同目录测试文件；`Engine::open_in_memory` 驱动 app 级
  测试（dispatch、surfaced 语义、快照重建、偏好往返）。
- 活体：Stalwart harness（`scripts/dev/harness.sh`，本 SP 移植）承担验收
  运行：向种子服务器添加 IMAP 账户，观察 `MailboxList` 发布文件夹树。
- 前端：Vitest 组件测试覆盖 FolderPane 从固定快照夹具渲染、账户流程对
  mock 命令的行为。

## 9. 验收标准

1. `cargo run -p xtask -- gate` 全绿，含每个新文件的许可证头。
2. 对 Stalwart harness：向导添加 IMAP 账户后凭据入 OS keyring、经引擎
   同步、文件夹树在壳内渲染，且应用重启后仍在。
3. 每个产品操作都经 `dispatch` 流动：没有 Tauri 命令直接改引擎或产品
   状态。
4. 引擎只经 `engine-api` 消费（`astra-account` 内的 provider crate 除外），
   锁在 rev `68eab00e`。
5. `capabilities.md` 对应行随变更移至 partial 或 complete，changelog
   fragment 同变更落地。
6. `client/desktop` 在 Windows 构建；CI 绿。

## 10. 范围外（随 SP2+ 到来）

消息列表内容、阅读窗、撰写与草稿、发件箱与发送 surface、日历、联系人、
手动 IMAP 路径之外的自动探测策略、搜索、MCP、sqlx 产品库、AI sidecar。

## 11. 待决项

- 插件宿主的跨进程执行落在 SP1 末还是 SP2（D6；由计划按余量判断）。
- 启动画面与窗口主题深度（kylins 两者皆有；SP1 取普通窗口，除非计划发现
  成本低）。
