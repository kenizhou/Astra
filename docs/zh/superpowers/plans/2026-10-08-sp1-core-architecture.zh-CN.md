# SP1 核心架构实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-08-sp1-core-architecture.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 构建 Astra 核心架构（engine pin 的 workspace、Intent/Surface 协议 v1、Tauri 宿主端口、最小 React 壳、插件骨架、Stalwart harness），并用垂直切片证明：添加 IMAP 账户、同步、看到文件夹树、重启存活。

**架构：** 把 mail-calendar 的核心 crate（`mailcal-app`/`viewmodel`/`account` 模式）移植为 `astra-*` crate，由 Tauri host 直接消费——host 扮演 `mailcal-bindings`（runtime 拥有者、keyring 凭据存储、去抖 observer 桥、`dispatch`/拉取命令）。React SPA 是被 `surface_changed` 事件驱动的薄渲染器。同步轮次、快照与 durable outbox 纪律都在 `astra-app`。

**技术栈：** Rust（edition 2021，workspace lints 镜像 mail-calendar）、九个 engine crate 按 rev `68eab00e` git pin、Tauri 2 + keyring 作 host、React 19 + Vite 7 + Tailwind v4 + Zustand 作 SPA、Docker Stalwart 作活体 harness。

**Spec：** `docs/superpowers/specs/2026-10-08-sp1-core-architecture-design.md`

## 全局约束

- 每个 `.rs`、`.ts`、`.tsx` 文件以 `// SPDX-License-Identifier: GPL-3.0-only` 开头（gate 强制）。
- 源码文件不超过 500 行；协议家族按 mail-calendar 的方式拆分。
- Intent 与 Surface 可 serde 序列化：外部标签枚举、snake_case、线上省略 `None`。
- 引擎只经 `engine-api` 消费（`astra-account` 内可用 provider crate），在
  `[workspace.dependencies]` 锁 `rev 68eab00e6a9a9fe5a200218e156dbaae1ba00caa`。
- SP1 无产品 SQLite：引擎 SQLite + `preferences.toml` + OS keyring（spec D4）。
- 每个产品操作经 `dispatch`；没有 Tauri 命令直接改引擎或产品状态。
- 测试为内联同目录文件（`<module>_tests.rs`）；app 级测试用
  `Engine::open_in_memory`。
- 英式英语；破折号不作标点；`docs/zh/` 以外纯英文。
- 每次提交前 `cargo run -p xtask -- gate` 绿；conventional commits。
- 移植源根：`D:/Projects/mailclient/opensource/mail-calendar`（下称 MC）。
  kylins 根：`D:/Projects/mailclient/kylins`（下称 KY）。

## 适配规则（每个移植任务都适用）

- **AR1**：`mailcal` -> `astra` 重命名（crate 名、模块路径、标识符、文档注释）；删除历史性注释（注释里不提移植、阶段、上游）。
- **AR2**：移除全部 `uniffi` 属性与 `mailcal-bindings` 转换层；对跨 Tauri IPC 的值（Intent、Surface、快照、status 枚举、setup 类型）加 `serde::Serialize`/`Deserialize` 派生。线上风格：默认外部标签，可选字段加 `#[serde(skip_serializing_if = "Option::is_none")]`。
- **AR3**：引擎类型（id、行、draft）从 `engine-api` 再导出导入，绝不重定义。
- **AR4**：日期用 `time` crate，与引擎一致。
- **AR5**：逐字镜像 MC 根 `Cargo.toml` 的 `[workspace.lints]` 块（unsafe forbid、warnings/missing_docs/unreachable_pub deny、clippy pedantic deny 及其 allows）；修到干净。
- **AR6**：`[workspace.dependencies]` 取 MC 表中版本、去掉 uniffi，加 `tauri = "2"`、`tauri-plugin-single-instance`、`tauri-plugin-opener`、`tauri-plugin-dialog`、`tauri-plugin-notification`、`keyring`（最新 3.x）——仅 host 使用。

---

### 任务 1：引擎接线与 workspace lints

**文件：**
- 修改：`Cargo.toml`（成员、workspace 依赖、lints）
- 新建：`.cargo/config.toml`

**接口：**
- 产出：`[workspace.dependencies]` 下九个 git pin 的引擎依赖（`engine-api`、`engine-core`、`engine-provider`、带 `tls-native-erts` 的 `engine-tls`、`provider-imap`、`provider-caldav`、`provider-graph`、`provider-google`、`provider-jmap`）；MC 的 lint 块；共享 `target-dir`。

- [ ] **步骤 1：** 按 MC `Cargo.toml` `[workspace.dependencies]` 的确切 `rev`、features 与逐 crate 拼写，把引擎 git 依赖加进根 `Cargo.toml`。
- [ ] **步骤 2：** 按 AR5 复制 MC 的 `[workspace.lints]` 块，各成员经 `[lints] workspace = true` 继承。
- [ ] **步骤 3：** 新建 `.cargo/config.toml`：共享构建目录 + 以注释形式收录 MC `.cargo/config.toml` 的 `[patch."https://github.com/allodia-eu/email-calendar-sync-engine"]` 本地检出模式。
- [ ] **步骤 4：** 建一次性 `crates/astra-viewmodel` 桩（只含 SPDX 头的空 lib.rs）作首个成员让 pin 真正解析；`cargo build -p engine-api` 成功；`cargo tree -p engine-api --depth 1 | head -20` 记入报告。
- [ ] **步骤 5：** gate 绿；提交 `build: pin the engine crates and adopt the mail-calendar lint set`。

### 任务 2：astra-viewmodel（快照类型）

**文件：**
- 新建：`crates/astra-viewmodel/{Cargo.toml, src/lib.rs, src/view.rs, src/folders.rs, src/view_tests.rs, src/folders_tests.rs}`
- 移植自：`MC/crates/mailcal-viewmodel/src/{view.rs, folders.rs}`，限 MailboxList surface。

**接口：**
- 消费：`engine-api`（id newtype、`MailboxListRow` 来源）。
- 产出：`MailboxListSnapshot { account_folders: Vec<AccountFolderRow> }`、`AccountFolderRow`（账户 id、显示名、嵌套文件夹、未读计数、角色、展开态），按 AR2 加 serde。

- [ ] **步骤 1：** 先写 `view_tests.rs`：快照经 `serde_json` 往返并断言确切线上形状（外部标签、snake_case、省略 `None`）。
- [ ] **步骤 2：** 按 AR1-AR4 移植 `view.rs`/`folders.rs`，只保留 `MailboxListSnapshot` 所需；日历/联系人/设置模块整体不取。
- [ ] **步骤 3：** `cargo test -p astra-viewmodel` 绿；clippy pedantic 干净；gate 绿；提交 `feat: port the mailbox list view model from mail-calendar`。

### 任务 3：astra-account 配置与设置

**文件：**
- 新建：`crates/astra-account/{Cargo.toml, src/lib.rs, src/config.rs, src/config_tests.rs, src/setup.rs, src/setup_tests.rs, src/credentials.rs, src/tls.rs}`
- 移植自：`MC/crates/mailcal-account/src/{config.rs, setup.rs, imap_credentials.rs, tls.rs}`。

**接口：**
- 产出：`AccountConfig`（TOML `[imap]`/`[smtp]` 节、脱敏 Debug 的 `Secret`、`load`/`load_str`/`to_toml`、`derived_account_id`、钉住的 `shape.id`）、`SetupCredential::{Password, Grant}`、带默认端口规则的 `build_config_toml`、基于注入 `CredentialSource` 的 `imap_credential_source`、带证书例外 pinning 类型的 TLS acceptor。

- [ ] **步骤 1：** 先写 `config_tests.rs`：TOML 往返、Debug 脱敏、派生 id 对用户名大小写稳定、拒绝空白用户名。
- [ ] **步骤 2：** 先写 `setup_tests.rs`：裸主机补标准端口、CalDAV 主机补 `https://`、密码凭据序列化进 TOML。
- [ ] **步骤 3：** 按适配规则移植四个模块；`to_toml` 仍是唯一序列化点。
- [ ] **步骤 4：** 测试、clippy、gate；提交 `feat: port account config, setup, credentials, and tls from mail-calendar`。

### 任务 4：astra-account IMAP 连接

**文件：**
- 新建：`crates/astra-account/src/{imap.rs, imap_tests.rs, reconnect.rs, pass_folders.rs}`
- 移植自：`MC/crates/mailcal-account/src/{imap.rs, reconnect.rs, pass_folders.rs}`。

**接口：**
- 产出：`ImapConnections`（每注册账户一个 `ImapAccount<TlsStream<TcpStream>>`，async-mutex 去重拨号）、`connect_mail_providers`（连接、LIST、绑定 `pass_syncs` 的文件夹、INBOX 优先）、`connect_imap_mailbox`、`ReconnectingImapProvider`（`FailureClass::Retryable` 时经注入 `Redial` 恰好重试一次）。

- [ ] **步骤 1：** 用假 `Provider`（模式取自 MC `imap.rs` 测试与 `tests_fakes`）先写 `imap_tests.rs`/重连测试：绑定顺序 INBOX 第一；可重试错误恰好触发一次 redial；不可重试直接传播。
- [ ] **步骤 2：** 按规则移植；必要时把假 provider 放 `src/fakes.rs` 以守 500 行。
- [ ] **步骤 3：** 测试、clippy、gate；提交 `feat: port imap connections, folder binding, and reconnect wrapper`。

### 任务 5：astra-app 协议 v1

**文件：**
- 新建：`crates/astra-app/{Cargo.toml, src/lib.rs, src/protocol.rs, src/protocol/{intent.rs, status.rs}, src/reference.rs, src/surfaced.rs, src/protocol_tests.rs, src/surfaced_tests.rs, src/reference_tests.rs}`
- 移植自：`MC/crates/mailcal-app/src/{protocol.rs, protocol/intent.rs（子集）, protocol/status.rs（子集）, reference.rs（子集）, surfaced.rs}`。

**接口：**
- 产出：`Intent` 恰含 `RefreshMail`、`SelectAccount(Option<AccountId>)`、`SetAccountExpanded{account, expanded}`、`SetFolderExpanded{folder: FolderRef, expanded}`、`SelectFolder{folder: FolderRef}`、`SetViewMode(ViewMode)`、`ShowMore`、`Folders(FolderIntent)`（仅声明 `Create`，`#[non_exhaustive]`）；`Surface::{MailboxList, SyncProgress}`；`FolderRef::from_parts(&str, String) -> Option<Self>`；`Surfaced<T>` 先发布后发信、`get()` 只克隆；全部按 AR2 加 serde。

- [ ] **步骤 1：** 先写 `protocol_tests.rs`：每个变体经 JSON 往返并断言确切线上名；`FolderRef::from_parts` 拒绝残缺两半。
- [ ] **步骤 2：** 先写 `surfaced_tests.rs`：发布在值可见后恰好发信一次；`get()` 永不发信；`resignal` 是唯一重发路径。
- [ ] **步骤 3：** 移植各文件；接口清单之外的 Intent/Surface/status 变体全部不取（随 SP2 切片回归）。
- [ ] **步骤 4：** 测试、clippy、gate；提交 `feat: port intent/surface protocol v1 with serde wire shapes`。

### 任务 6：astra-app 派发、快照、账户、同步、boot

**文件：**
- 新建：`crates/astra-app/src/{dispatch.rs, dispatch_tests.rs, snapshot.rs, window.rs, row_cache.rs, accounts.rs, sync.rs, sync_account.rs, preferences.rs, preferences_tests.rs, boot.rs}`
- 移植自：`MC/crates/mailcal-app/src/{dispatch.rs, snapshot.rs, window.rs, row_cache.rs, accounts.rs, sync.rs, sync_account.rs}`，加 `MC/crates/mailcal-account/src/preferences.rs` 与 `MC/crates/mailcal-bindings/src/boot.rs` 的 boot 序列。

**接口：**
- 产出：`App<P: Provider>::dispatch(&self, intent: Intent)`（async、每变体一臂、`Folders` 臂返回尚未实现错误）；`App::new(engine: Arc<Engine>, accounts, connector, observer, clock)`；带搜索代际守卫与窗口化 `visible_limit` + `ShowMore` 的 `rebuild_snapshot`；`add_new_account_deferred`（安装、重建、默认 3 个月深度、不同步）与 `sync_added_account`；`StreamTuning::new(200, 50)` 的 `sync_mail` 扇出；`preferences.toml`（视图模式、展开、选中文件夹）往返；`boot()` 序列：开引擎、`abandon_sync_leases`、构造 app、`recover_outbox`、`prime_snapshot`、spawn 连接+同步。

- [ ] **步骤 1：** 用 `Engine::open_in_memory` 与假 provider 先写 `dispatch_tests.rs`：`SelectFolder` 持久化进偏好并重建 `MailboxList`；`RefreshMail` 扇出同步并发布；`ShowMore` 增长窗口。
- [ ] **步骤 2：** 先写 `preferences_tests.rs`：默认值、往返、展开映射更新。
- [ ] **步骤 3：** 移植各模块；observer 是从 MC `protocol.rs` 移植的 `AppObserver` trait（`Send + Sync` 界限）；telemetry 钩子不取（SP1 无 telemetry）。
- [ ] **步骤 4：** 测试、clippy、gate；提交 `feat: port dispatch loop, snapshots, sync, preferences, and boot`。

### 任务 7：astra-plugin 骨架

**文件：**
- 新建：`crates/astra-plugin/{Cargo.toml, src/lib.rs, src/manifest.rs, src/manifest_tests.rs, src/lifecycle.rs, src/hook_api.rs, src/permissions.rs}`

**接口：**
- 产出：`PluginManifest`（id、名称、版本化 hook API 等级、声明的权限）从 TOML 解析、Debug 无秘密；`Lifecycle::{Discovered, Loaded, Activated, Failed(String)}` 状态机转移；`HookApiV1::{ComposeTransform, MessageDecode}` 类型化请求/响应契约（serde）；`Permission::{ReadMailbox, TransformOutgoing, DecodeIncoming}` 及 fail-closed 默认表（一切需批准）。暂无进程执行（spec D6）。

- [ ] **步骤 1：** 先写 `manifest_tests.rs`：合法 TOML 解析；API 等级不匹配与缺权限声明被拒；未知字段报错。
- [ ] **步骤 2：** 先写生命周期转移测试（非法转移被拒）。
- [ ] **步骤 3：** 实现类型；权限表的 fail-closed 默认做单测。
- [ ] **步骤 4：** 测试、clippy、gate；提交 `feat: add plugin host skeleton (manifest, lifecycle, hook api v1 types)`。

### 任务 8：Stalwart harness

**文件：**
- 新建：`docker/harness/{compose.yaml, stalwart/config.toml, stalwart/seed/*}`、`scripts/dev/harness.sh`
- 移植自：`MC/docker/**` 与 `MC/scripts/dev/harness.sh`（读 MC 的 seeding 形态；Astra 需要两个种子 IMAP/SMTP 账户、各带一个自定义文件夹）。

**接口：**
- 产出：`scripts/dev/harness.sh up|down|reset` 在 `127.0.0.1:2143`（IMAP）与 `2025`（SMTP）起种子 Stalwart；脚本导出 `ASTRA_LIVE_HARNESS=1` 与 `ASTRA_HARNESS_IMAP_HOST/PORT/USER/PASSWORD`；`astra-app` 里 gate 门控的 `tests_live_harness.rs` 用任务 4 的 `ImapConnections` 连上并断言文件夹清单。

- [ ] **步骤 1：** 读 MC docker harness，移植最小 compose + 配置；种子两用户（`astra1@example.test`、`astra2@example.test`，密码 `astra-harness`），各一个自定义文件夹。
- [ ] **步骤 2：** 写带 `up|down|reset` 与打印导出变量的 `env` 子命令的 `harness.sh`。
- [ ] **步骤 3：** `bash scripts/dev/harness.sh up` 后 `docker ps` 健康；一次性脚本验证 IMAP LOGIN。
- [ ] **步骤 4：** 写门控测试（无 `ASTRA_LIVE_HARNESS=1` 即跳过），对 harness 跑绿；harness 关闭时 gate 仍绿；提交 `test: port the stalwart harness and add the gated live folder test`。

### 任务 9：Tauri host 脚手架与 runtime

**文件：**
- 新建：`client/desktop/{Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, src/main.rs, src/lib.rs, src/runtime.rs, src/runtime_tests.rs}`
- 移植自：`KY/kylins.client.backend/{tauri.conf.json, capabilities/default.json, src/shell/desktop.rs（builder 形态）}` 与 `MC/crates/mailcal-bindings/src/runtime.rs`。

**接口：**
- 产出：无边框主窗（1200x800、最小 800x600、`com.kenizhou.astra`、图标取 `client/desktop/icons/`）；插件顺序 single-instance、deep-link（预留 mailto）、opener、dialog、notification；`build_runtime()`（workers = 核数减一）加大栈阻塞调用辅助（移植 MC 的 `FfiRuntime::block_on`）；setup 中注册托管状态。

- [ ] **步骤 1：** 脚手架 crate；`tauri.conf.json` 的 `frontendDist` 指向 `../web/dist`（任务 12 前先建空 `client/web/dist/.gitkeep` 兜底，任务 12 移除）。
- [ ] **步骤 2：** 移植 runtime 模块；测试：`build_runtime()` 返回的 runtime worker 数符合本机公式。
- [ ] **步骤 3：** Windows 上 `cargo build -p astra-desktop` 成功；加入 workspace 成员但排除出 `default-members`；同一变更里给 `.github/workflows/ci.yml` 与 `nightly.yml` 两侧 ubuntu 腿加 Tauri 系统依赖安装步骤（`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev`），因为 workspace 级 cargo 命令从此会编译 desktop crate 而 ubuntu runner 缺这些库（Ruling R8）。
- [ ] **步骤 4：** gate 绿；提交 `feat: scaffold the tauri host with runtime and window`。

### 任务 10：Tauri host 端口与命令

**文件：**
- 新建：`client/desktop/src/{app_state.rs, credential_store.rs, observer.rs, observer_tests.rs, commands.rs, commands_tests.rs, boot.rs, exit.rs}`
- 移植自：`MC/crates/mailcal-bindings/src/{credential_store.rs（trait 形态）, observer.rs（DebouncedObserver）, app_accounts.rs（add_account 流程）, boot.rs（序列）}`；keyring 实现为新写。

**接口：**
- 产出：基于 `keyring` crate 的 `AccountCredentialStore`（服务名 `astra`、按账户条目、persist 失败回滚）；`dispatch(intent: Intent)` 命令（从 webview serde 进来、runtime 上 spawn）；`mailbox_list() -> MailboxListSnapshot` 与 `sync_progress()` 拉取命令；经 `DebouncedObserver`（250ms、MailboxList 前沿、SyncProgress 后沿）发 `surface_changed` 事件；`add_account(config_toml)`（prepare、dial、persist、`add_new_account_deferred`、spawn `sync_added_account`）；`imap_test_connection`；五秒有界停同步的退出钩子。

- [ ] **步骤 1：** 用同一 trait 缝后面的假 `App` 先写 `commands_tests.rs`：`dispatch` 接受每个 Intent 变体的 JSON；`mailbox_list` 返回已发布快照；不可解析 intent 得类型化错误、绝不 panic。
- [ ] **步骤 2：** 先写 `observer_tests.rs`：窗口内 N 次发布合并为一次前沿 MailboxList 信号；SyncProgress 信号后沿。
- [ ] **步骤 3：** 实现；托管状态是单个 `AppState { app, runtime, credential_store }`。
- [ ] **步骤 4：** 测试、clippy（仅 tauri crate 允许按记录裁定收窄 pedantic）、gate；提交 `feat: add host ports, ipc commands, and boot wiring`。

### 任务 11：React SPA 脚手架

**文件：**
- 新建：`client/web/{package.json, vite.config.ts, tsconfig.json, index.html, src/main.tsx, src/App.tsx, src/styles/{globals.css, theme.css}, src/vite-env.d.ts}`
- 移植自：`KY/kylins.client.frontend/{package.json（依赖版本）, vite.config.ts, src/styles/theme.css（token 集）}`。

**接口：**
- 产出：Vite 7 + React 19 + TS strict + 经 `@tailwindcss/vite` 的 Tailwind v4、端口 5173 strict、`@` 别名；CSS 自定义属性主题 token、`.light`/`.dark` 明暗；每个 `.ts`/`.tsx` 带 SPDX 头；`npm run build` 产出 `dist/`。

- [ ] **步骤 1：** 按 kylins 版本脚手架（react 19、vite 7、tailwindcss 4、zustand、`@tauri-apps/api` 2；dev 依赖 typescript、vitest、@testing-library/react、jsdom 供任务 12 用）。
- [ ] **步骤 2：** 移植主题 token 集（颜色、表面、`--header-h`、z 层级），裁到壳所需。
- [ ] **步骤 3：** `npm run build` 绿；移除任务 9 的 `.gitkeep`；gate 绿（license-headers 开始扫 `.ts`/`.tsx`）；提交 `feat: scaffold the react spa with theme tokens`。

### 任务 12：React 壳组件

**文件：**
- 新建：`client/web/src/components/layout/{AppShell.tsx, TitleBar.tsx, StatusBar.tsx}`、`client/web/src/components/folder-pane/{FolderPane.tsx, FolderTree.tsx}`、`client/web/src/components/ui/WindowTitleControls.tsx`、`client/web/src/stores/{accountStore.ts, folderStore.ts, uiStore.ts}`、`client/web/src/services/{ipc.ts, surfaces.ts}`、`client/web/src/events/{surfaceEvents.ts, surfaceEvents.test.ts}`、`client/web/src/features/account-setup/{AccountSetupFlow.tsx, AccountSetupFlow.test.tsx}`
- 移植自：`KY/kylins.client.frontend/src/components/layout/{AppShell.tsx, TitleBar.tsx}`、`ui/WindowTitleBar.tsx`（控件）、stores 与 services 模式、`events/engineEvents.ts`（类型化模式）。

**接口：**
- 产出：`ipc.ts` 暴露 `dispatch(intent)`、`mailboxList()`、`syncProgress()`、`addAccount(toml)`、`imapTestConnection(cfg)` 类型化包装；`surfaceEvents.ts` 监听 `surface_changed` 并分发到 stores；FolderPane 从测试 `__fixtures__` JSON 渲染 `MailboxListSnapshot`（账户、嵌套、未读徽标、角色）；账户设置流程收集 host/port/user/password、测连接、`addAccount` 后 `dispatch({ RefreshMail: null })`；窗口控件接 `getCurrentWindow()`。

- [ ] **步骤 1：** 先写 `surfaceEvents.test.tsx` 与 `AccountSetupFlow.test.tsx`（mock ipc）：`surface_changed` MailboxList 事件触发 `mailboxList` 拉取进 `folderStore`；流程在 `imapTestConnection` 失败时拒绝、成功时调 `addAccount` + `RefreshMail`。
- [ ] **步骤 2：** 实现组件与 stores；拖拽区与双击最大化按 kylins TitleBar。
- [ ] **步骤 3：** `npm run build` 与 `npx vitest run` 绿；gate 绿；提交 `feat: add the desktop shell (titlebar, folder pane, status bar) and account setup flow`。

### 任务 13：垂直切片验收与文档

**文件：**
- 新建：`scripts/dev/slice-check.sh`、`docs/changelog/unreleased/sp1-vertical-slice.md`
- 修改：`docs/capabilities.md`（+zh 镜像）、受影响文档的 zh 镜像、`.github/workflows/ci.yml`（desktop job）

- [ ] **步骤 1：** 写 `slice-check.sh`：起 harness、被测 app 以 `--headless-slice` 旗标启动（真实 `main.rs` 分支，不是测试专用复制），走 `add_account` + `dispatch(RefreshMail)` + 轮询 `mailbox_list`，打印 JSON 文件夹树，断言 INBOX 与自定义文件夹出现，重启二进制后断言树凭 `preferences.toml` + 引擎库持久存在。
- [ ] **步骤 2：** 跑绿；输出记入报告。
- [ ] **步骤 3：** 更新 `capabilities.md`：账户行（向导 partial：仅手动 IMAP）、folder pane 行（树、展开持久）到 partial；CLI/AI 行不动。镜像到 zh。加 changelog fragment（`Platforms: desktop`、`Bump: minor`）。
- [ ] **步骤 4：** CI：加 `desktop` job（windows-latest 上 `cargo clippy -p astra-desktop` + `cargo build -p astra-desktop`；ubuntu 保持核心 only 的 default-members 集）；本 SP 不改 nightly。
- [ ] **步骤 5：** 全量 gate + `cargo test --workspace` + 前端构建/测试；提交 `feat: prove the vertical slice end to end and update capabilities`。

### 任务 14：SP1 收尾

- [ ] **步骤 1：** `cargo run -p xtask -- gate`、`cargo test --workspace`、`cargo build -p astra-desktop`、`cd client/web && npm run build && npx vitest run` 全绿；工作树干净。
- [ ] **步骤 2：** 推 `sp1-core-architecture` 分支，对 main 开 PR 并链接 spec；PR 上 CI 绿。
- [ ] **步骤 3：** 按 SDD 技能清台账与工作区；对 spec 第 9 节六条验收标准逐条给出证据。

---

## 自审记录

- Spec 覆盖：D1（任务 9-10）、D2（任务 5、10）、D3（任务 6）、D5（任务 2-6、9-12）、D6（任务 7）、harness 与验收（任务 8、13）、CI 范围（任务 13.4）。D4 无需任务：它是一个「不做」。
- 接口一致性：任务 5 定义的 `FolderRef` 被任务 6、12 消费；任务 2 定义的 `MailboxListSnapshot` 流经任务 6、10、12；任务 4 的 `ImapConnections` 喂任务 6、8、10。
- 已知风险记为待决：tauri crate 内的 pedantic clippy 可能需要范围化放宽（任务 10 若采取则记录裁定）。
