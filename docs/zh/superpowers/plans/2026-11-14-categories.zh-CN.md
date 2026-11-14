# 分类计划

规格：docs/superpowers/specs/2026-11-14-categories-design.md。分支 `categories`
自规格提交从 main 分出。每个任务：先红、实现、电池、一个约定式提交。一个契约变更：
新 docs/categories.md 连同其 zh 镜像在同一提交，capabilities 行连同其 zh 镜像移动，
落变更日志碎片，porting-backlog.md 把 categories 移入已交付列表。无引擎改动、无 AI
bump。保真本地覆盖层移植，无参考偏差。

### 任务 1：store + wire + 核心覆盖层（D1，D2）

**文件：** 新 crate crates/astra-categories（Cargo.toml + src/lib.rs + src/store.rs
+ src/migrations/0001_init.sql，astra-reminders 形态；categories + thread_categories
表，FK ON DELETE CASCADE，两 open 均 PRAGMA foreign_keys ON；CategoryId newtype；
create/update/delete/assign/unassign/list_categories/assignments_for_threads，
slug_for/valid_color/validate）+ store 测试；crates/astra-viewmodel/src/rows.rs
（MessageRowView.categories Vec<CategoryId> serde default + skip_serializing_if
empty）+ Cargo.toml 依赖；crates/astra-app/src/categories.rs（install_categories_store
+ 六核心动词，assign/unassign/delete 重建）+ categories_overlay.rs（投影后折叠）+
三个 list_rows 构造器初始化 categories + 折叠调用点（snapshot_rows folder_rows/
unified_rows、search run_search）+ 覆盖层测试；App 槽（lib.rs 在上限，回收注释行）。

- [ ] 在钩子处确认：astra-reminders store 形态、FK pragma 先例
      （astra-crypto-store/store.rs:137）、thread_key 约定（list_rows.rs:309-314）、
      投影调用点（snapshot_rows.rs:33/70、search.rs:189）、重建（snapshot.rs）、
      install_reminder_store。测试先行（store：CRUD、slug 后缀、NOCASE 唯一、类型化
      拒绝、改名保留分配、删除级联 + 计数证明 FK ON、幂等 assign/unassign、未知分类
      拒绝、assignments rowid 序 + 空 + 账户域；覆盖层：assign 投影 folder/unified/
      search 平铺 + 会话两模式含独消息、unassign 移除、删除级联、改名保留 id、引擎
      不动零 edit_mail、无 store 空）；随后 store + wire + 核心。提交
      `feat: the category store and overlay`。
- [ ] astra-categories/astra-app/astra-viewmodel 不豁免上限（lib.rs 在上限回收，
      list_rows +3 安全）。newtype（CategoryId 跨边界）。FK pragma 承重（无它级联静
      默无效）。无引擎写。既有测试保持绿。

### 任务 2：宿主命令 + AppPort + boot（D3 宿主）

**文件：** 新 client/desktop/src/commands/categories.rs（六命令，
commands/reminders.rs 形态，畸形 ref 拒绝；commands.rs 在上限回收一行）在 lib.rs 注
册；新 app_state/categories_port.rs（CategoriesPort supertrait，trait AppPort:
CategoriesPort）+ 每 impl 分类块在自有子文件（app_state/port.rs、
commands_fakes/port.rs 接近上限、showcase/app.rs）；boot 安装（boot.rs
assemble_state 打开 categories.sqlite + install_categories_store、
CATEGORIES_STORE_FILE 常量，无定时器）。

- [ ] 测试先行（命令路由 + 畸形 ref 拒绝基于 fakes）；随后命令 + supertrait + boot。
      提交 `feat: the category host commands`。
- [ ] client/desktop 不豁免上限：supertrait 拆分使 app_state.rs（500）与
      commands_fakes/port.rs（498）几乎不动（E0119：不同 trait 自由在不同于文件
      impl）。commands.rs 回收 1 行。drop 序不动（无定时器、无 AppState 字段）。既有
      测试保持绿。

### 任务 3：web 面 + 契约 + 活实证（D3 web，D4）

**文件：** 新 client/web/src/services/categories.ts（CategoryDef wire + 六包装 + 调
色板，绝不在 ipc.ts）；features/categories/categoriesStore.ts（zustand 定义快照 +
loadCategories + 修订号，MessageList 一个加载器）+ CategoriesDialog.tsx（管理器
CRUD，名 + 20 色板，删除确认，本地-only 文案，useDialogFocus）+ categoryMenu.ts
（Categorize 子菜单，Outlook 切换）+ 测试；ContextMenu.tsx 子渲染器小扩展（checked
槽 + 色板 + keepOpenOnSelect）；MessageRow.tsx 前导色条；分类过滤（listFilter 谓词 +
listFilterStore categoryFilter + ListToolbar select picker，AND 到 tab，文件夹切换
重置，定义删除清空）；surfaces.ts MessageRowView.categories；docs/categories.md +
zh；docs/capabilities.md 行 + zh；docs/changelog/unreleased/categories.md
（Platforms: desktop，Bump: minor）；docs/porting-backlog.md + zh；
crates/astra-app/src/tests_live_harness/categories_proof.rs（提交笔记、经核心创建 +
assign、断言行带 id 真实读上、引擎不动、删除 -> 行空、撤回）+ tests_live_harness.rs
注册。

- [ ] 测试先行（服务包装、定义 store、切换语义、管理器 CRUD + 删除确认、徽章首色/
      tooltip/已删定义、过滤 AND + 重置 + 删除清空；离线跳过的门控活实证）；随后 web +
      文档。提交 `feat: the category surface and contract`。
- [ ] client/web 不豁免：rowMenu.ts 495（+3 -> 498 紧，categoryMenu 构建器模式）；
      ipc.ts 在上限（services/categories.ts）；ContextMenu.tsx 311（+20 安全）。
      vitest 陷阱：act() 测试中段，整体 mock 服务。注释现在时；无 em/en 破折号；英式
      英语；源文件以换行结尾。docs/zh 镜像同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；cargo
      clippy --workspace --all-targets -- -D warnings；cargo test --workspace；npm
      --prefix sidecar run build && npm --prefix sidecar test；npx tsc --noEmit &&
      npx vitest run（client/web）；cargo run -p xtask -- gate。若 plugin 测试以
      AlreadyGranted 失败，清理陈旧 /tmp/astra-desktop-plugin-* scratch 目录
      （Windows pid 回收 flake）再重跑。留意磁盘空间（增量缓存膨胀则清理）。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后 up +
      env；cargo test -p astra-app tests_live_harness::categories_proof --
      --test-threads=1），随后 unset。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步 main；ledger
      清理。

## 自审笔记

- 规格覆盖：D1 的 store + D2 的 wire/核心/覆盖层（T1）；D3 的宿主命令（T2）；D3 的
  web + D4 的契约 + 活实证 + 电池 + PR（T3）。
- 风险：FK pragma 默认 OFF（级联静默无效；两 open 启用 + 测试）；平铺模式 thread-key
      解析须精确匹配 thread_key()（独消息以自身 provider key 为键；测两模式）；上限
      压力（app_state.rs 500 / commands_fakes/port.rs 498 -> supertrait 拆分；
      lib.rs/commands.rs 回收；rowMenu.ts 495 +3）；本地-only 红线（无引擎写；
      SetKeywords 陷阱是本地性非能力）。分类/条件格式/CLI 动词/自定义色/重排（记录
      缺口）。
- 不评测/不提升 pipeline（手动分类非 AI）。无参考偏差。活实证教训：先 reset
  harness；若电池 flake 清理陈旧 plugin scratch 目录；确保末尾换行。