# 分类设计

**状态。** 2026-11-14 定稿。切片自此提交从 main 分出。

**范围。** 没有办法用颜色分类标记邮件。本 kylins 附加新增手动分类：定义带色分类、
 assigning 到会话、在行上看到色条、按分类过滤、管理定义。这是本地覆盖层，锁定
kylins 参考的红线（thread_categories.rs）：分类是本地 v1，无引擎写、无同步、无协议
概念；EAS 与 IMAP 同等服务，因为表是协议无关的 shell 状态。新 astra-categories
store（定义 + 每 (account,thread) 分配）、行 wire 上的 categories 字段（仅 id）、折
入列表投影的覆盖层、及管理器/菜单/徽章/过滤 UI。无引擎改动；不评测/不提升 pipeline
（手动分类非 AI）。保真移植，无参考偏差。

## D1：分类 store

新 crate crates/astra-categories（astra-reminders 形态：Mutex 下 rusqlite、内嵌迁
移、带 5s busy timeout 的 open、open_in_memory）。两表：categories（id TEXT 主键
[slug]、name TEXT NOT NULL UNIQUE COLLATE NOCASE、color TEXT NOT NULL [#RRGGBB]、
created_at）与 thread_categories（account_id、thread_id、category_id、created_at、
主键 (account_id,thread_id,category_id)、外键 category_id 引用 categories ON DELETE
CASCADE），category_id 索引。关键：rusqlite 默认不启用外键；在 open 与 open_in_memory
两者设 PRAGMA foreign_keys = ON（astra-crypto-store 先例），测试须证明级联触发。
CategoryId newtype（slug 跨入 astra-viewmodel）。方法（镜像参考）：create_category
（校验名 <=64 去空白 + 色 #RRGGBB，slug 带 -2/-3 冲突后缀，类型化 NOCASE「已存在」
拒绝）、update_category（原地改名/改色，分配以 id 为键故存活）、delete_category
（事务计数 + 级联，返回移除数）、assign_category（探存在 + INSERT OR IGNORE，幂等，
仅插入时 Ok(true)）、unassign_category（幂等）、list_categories（rowid/创建序）、
assignments_for_threads（绑定 IN，ORDER BY thread_id,rowid，账户域）。设备本地；CLI
不共享（snooze/reminders 先例）。

## D2：核心动词 + 覆盖层 + wire

核心 install_categories_store（install_reminder_store 形态，
Mutex<Option<Arc<Store>>> 槽；lib.rs 在上限，回收注释行）与新 categories.rs 六个核
心动词。assign/unassign 取 RowRef（Thread -> 直接 thread_id；Message -> 一次引擎读
解析 thread_id 或 key，thread_key 约定 list_rows.rs:309-314，是读非写故红线成立）；
assign/unassign/delete 以 invalidate_list_cache + bump_generation + rebuild_snapshot
收尾（create/update 无需重建，wire 仅带 id）。

新 categories_overlay.rs 在投影后折叠分配（纯投影不动）：每行解析其 thread key
（RowKey::Thread -> thread_id；RowKey::Message -> 源行的 thread_key），按账户分组，
每页每账户一次 assignments_for_threads 查询，设 row.categories。折于三投影调用点
（snapshot_rows folder_rows + unified_rows、search run_search）。覆盖层绝不缓存
（mail_row_cache 仅缓存引擎行；折叠每次重建新鲜运行）。

wire：MessageRowView 增 categories: Vec<CategoryId>（serde default +
skip_serializing_if empty，thread_count 惯用法；仅 id，名/色 web 侧从定义快照解析）。
astra-viewmodel 依赖 astra-categories（无环）。三个 list_rows 构造器初始化
categories: Vec::new()。

## D3：宿主命令 + web 面

新 commands/categories.rs（六命令，commands/reminders.rs 形态，核心前畸形 ref 拒
绝；commands.rs 在上限回收一行）。AppPort 经自有模块的 CategoriesPort supertrait
（app_state/categories_port.rs，trait AppPort: CategoriesPort）增六方法，故每 impl
的分类块在自有子文件（E0119 禁拆单一 trait 的 impl，不同 trait 自由 impl），使
app_state.rs（500）与 commands_fakes/port.rs（498）几乎不动。Boot 在 assemble_state
打开 categories.sqlite + install_categories_store（无定时器，drop 序不动）。

web：services/categories.ts（CategoryDef wire 类型 + 六包装 + 调色板，绝不在
ipc.ts）；features/categories/categoriesStore.ts（zustand 定义快照 + loadCategories
+ 修订号，MessageList 挂一个加载器）；CategoriesDialog 管理器（名 + 20 色板 radiogroup、
删除确认带「从每个会话移除」文案、本地-only 文案置顶、snoozeStore 开合形态、
useDialogFocus）；Categorize 子菜单（categoryMenu.ts 即 blockSenderMenu 模式，Outlook
切换：当每个目标都带该分类则全移除，否则加到缺失者；ContextMenu 子渲染器小扩展加
checked 槽 + 色板 + keepOpenOnSelect）；MessageRow 前导色条（首分类色，title 列所有
名，已删定义解析为空）；分类过滤（listFilter 谓词 + listFilterStore categoryFilter +
ListToolbar tablist 外 select picker，AND 到活动 tab，文件夹切换重置，定义删除清空，
无定义禁用）。

## D4：契约

新 docs/categories.md（本地-only 红线、store、覆盖层、面、记录缺口；SetKeywords 陷
阱理由：引擎路径经 outbox 写服务器同步关键字，违反本地性，非能力）+ zh；capabilities
行；变更日志碎片；porting-backlog 已交付列表移动。zh 镜像同提交移动。

## 已知缺口，记录而非静默

- 分类（AI 自动归类）是独立后续（触发 eval/pipeline-bump 规则）；条件格式是独立后续
  （读分类但自有契约）。
- 本地-only：无同步、无跨设备漫游、无协议概念；定义全局（非每账户），创建序固定
  （无重排）。
- 无 CLI/agent 动词（本地视图态）；无自定义取色器（仅色板，后端接受任意 #RRGGBB）。
- 批量 assign 逐行循环（行菜单妥协）；多账户批量逐行解析。

## 测试先行

每个行为都带着它的红落地。store 测试（CRUD、slug 冲突后缀、NOCASE 唯一、类型化校验
拒绝、改名/改色保留分配、删除级联 + 移除数证明 FK ON、幂等 assign/unassign、未知分类
拒绝、assignments rowid 序 + 空页 + 账户域）。核心覆盖层测试基于 fake provider
（assign 投影到 folder/unified/search 行，平铺与会话两模式含独消息情形、unassign 移
除、删除级离开行、改名保留 id、引擎不动[零 edit_mail]、无 store 答空）。命令测试
（路由 + 畸形 ref 拒绝）。web 测试（服务包装、定义 store、切换语义、管理器 CRUD + 删
除确认、徽章首色/tooltip/已删定义、过滤 AND + 重置 + 删除清空；act() 测试中段）。一
个门控活 harness 实证（categories_proof.rs：提交笔记、经核心创建 + assign、断言
mailbox_list 行带 id[真实读上的覆盖层]、断言引擎不动[关键字不变、无 outbox op、笔记
在 INBOX 未移动]、删除分类、断言行 categories 空、撤回笔记）。离线电池是主证明；活
实证加真实读上覆盖层 + 引擎不动断言。