> 非规范性译本，仅供审阅；以英文版为准。

# 分类：Astra Mail 契约

**范围。** 用颜色分类标记邮件：定义一个带颜色的分类，把它指派给会话，
在行上看到颜色条，按分类筛选，并管理这些定义。对每一个交付该特性的
表面都有约束力。这是一个本地叠加层，即锁定的 kylins 参考的红线
（`thread_categories.rs`）：分类是本地 v1，不写引擎、不同步、没有协议
概念。EAS 与 IMAP 账户被同等对待，因为这些表是与协议无关的壳状态。
不改引擎；不做评测或流水线版本提升（手动分类不是 AI）。缺口在文末
记录，绝不静默。

**原则。** *分类不拥有任何引擎状态。* 定义与指派存放在设备本地存储；
列表通过投影之后的折叠读回它们。给一个平铺消息行指派分类时，通过一次
引擎读取解析其线程键，绝不写入。引擎的关键字绝不被触碰：分类不是
关键字。

## 存储

新 crate `astra-categories` 持有定义与指派（提醒存储的形状：一个互斥锁
下的 rusqlite 连接、在事务中认领的嵌入式迁移、五秒忙超时、供测试用的
内存打开）。宿主在数据目录打开 `categories.sqlite` 并安装到核心（一个
`Mutex<Option<Arc<Store>>>` 槽；无 `AppState` 字段、无定时器，因此 drop
顺序保持原样）。两张表：

- `categories`：定义，全局（非按账户）。`id`（名字的 slug，主键）、
  `name`（唯一，`COLLATE NOCASE`）、`color`（`#RRGGBB`）、`created_at`。
  按创建顺序（rowid）列出；改名或改色绝不重排。
- `thread_categories`：每个 (账户, 线程, 分类) 一行。`account_id`、
  `thread_id`、`category_id`、`created_at`，主键
  (account_id, thread_id, category_id)，以及一个
  `FOREIGN KEY category_id REFERENCES categories ON DELETE CASCADE`，
  并在 `category_id` 上建索引。

**外键 pragma 是关键。** rusqlite 默认不启用外键；存储在 `open` 与
`open_in_memory` 两处都设置 `PRAGMA foreign_keys = ON`（加密存储的
先例）。没有它，删除级联会静默失效，悬空指派会残留。一个存储测试
证明级联确实触发。

`CategoryId` 新类型把 slug 带过边界进入行wire。方法与参考一致：
`create_category`（校验修剪后不超过 64 字符的名字与 `#RRGGBB` 颜色，
生成带 `-2`、`-3` 冲突后缀的 slug，类型化的 NOCASE“已存在”拒绝）、
`update_category`（原地改名改色；指派以 id 为键，因此得以保留）、
`delete_category`（事务内计数加级联，返回移除数量）、`assign_category`
（先探测存在再 `INSERT OR IGNORE`，幂等，当且仅当插入时 `Ok(true)`）、
`unassign_category`（幂等）、`list_categories`（rowid 顺序）、以及
`assignments_for_threads`（绑定 `IN` 查询，`ORDER BY thread_id, rowid`，
按账户作用域，返回线程 id 到分类 id 的映射）。存储是设备本地的：不跨
设备漫游，CLI 也不共享它（稍后处理/提醒的先例）。

## wire 与叠加层

`MessageRowView` 新增 `categories: Vec<CategoryId>`（serde 默认值，为空时
跳过，`thread_count` 的惯用法）：仅 id，名字与颜色在表面侧从定义快照
解析。三个 `list_rows` 构造器把它初始化为空；纯投影保持不动。

新的 `categories_overlay` 在投影之后折叠指派：对每一行解析其线程键
（线程行用其 id，消息行用其源行的 `thread_key`，即已解析的线程 id，
或从未成线程时消息自己的提供商标键），按账户分组，对页面上每个账户
运行一次 `assignments_for_threads` 查询，并设置 `row.categories`。折叠
发生在三个投影调用点（文件夹行、统一行、搜索结果）。叠加层绝不被
缓存：邮件行缓存只持有引擎行，折叠在每次重建时重新运行。

## 核心动词

`install_categories_store` 安装存储。六个动词：`create_category`、
`update_category`、`delete_category`、`assign_category`、
`unassign_category`、`list_categories`。指派与取消指派接受一个
`RowRef`：线程行直接以其 id 为键；消息行通过一次引擎读取
（`mail_by_keys`）解析其线程键，即投影分组所用的同一 `thread_key`
约定，这是一次读取，因此红线得以保持。指派、取消指派、删除以
`invalidate_list_cache`、`bump_generation`、`rebuild_snapshot` 收尾，
使行携带或脱落该 id；创建与更新无需重建（wire 仅携带 id）。未安装
存储的应用不叠加任何内容并拒绝写入；列表返回空。

## 宿主命令

`commands/categories.rs` 子模块承载六个命令（`commands/reminders.rs`
的形状：一个内部 `*_of` 主体加一个薄 `#[tauri::command]` 包装；
`commands.rs` 已到上限）。畸形的行引用在到达核心之前被拒绝。
`AppPort` 通过一个位于自身模块的 `CategoriesPort` 超_trait 获得这六个
方法（`app_state/categories_port.rs`，`trait AppPort: CategoriesPort`），
因此每个实现的分类块都位于自身的子文件（一个 trait 的实现不能跨模块
拆分；不同 trait 可自由实现），使 `app_state.rs` 与假端口几乎不动。
启动时在 `assemble_state` 打开 `categories.sqlite` 并安装；无定时器，
drop 顺序不动。

## 表面

wire 走 `services/categories.ts`（`CategoryDef` 类型、六个包装、以及
二十色调色板；绝不走已到上限的 `ipc.ts`）。`categoriesStore` 持有定义
快照、一个在 `MessageList` 中挂载一次的 `loadCategories` 加载器、以及
一个修订号提升。管理器对话框（`CategoriesDialog`）提供名字与调色板
单选组、改名改色编辑、以及带“从每个会话移除”措辞与数量的删除确认；
本地-only 的说明置于开头。行菜单的 Categorize 子菜单
（`categoryMenu.ts`，`blockSenderMenu` 的模式）把定义列为带勾选的色板，
采用 Outlook 切换：当每个目标都携带该分类时从全部移除，否则给缺失者
添加；一个小的 `ContextMenu` 子渲染器扩展画出勾选槽与色板，并在多次
选择间保持菜单打开（`keepOpenOnSelect`）。`MessageRow` 上的前导颜色条
显示第一个分类的颜色，提示框命名该行携带的每个分类；被删除的定义
解析为空。分类筛选（一个 `listFilter` 谓词、一个 `listFilterStore` 的
`categoryFilter`、以及一个位于 tablist 之外的 `ListToolbar` 选择器）
与活动标签做 AND，在文件夹切换时重置，在其定义被删除时清空，并在
没有定义时禁用。

## 拒绝 SetKeywords 陷阱

引擎的 `SetKeywords` 路径通过发件箱写入服务器同步的关键字。用它来做
分类会违反本地性，而非能力：引擎能写关键字，但分类绝不能离开设备、
绝不同步、绝不能成为协议概念。本地叠加层才是忠实的移植；关键字路径
是本切片刻意避开的陷阱。

## 已知缺口

- **归类**（AI 自动分类）是单独的后续（它触发评测与流水线版本提升
  规则）；**条件格式**是单独的后续（它们读取分类，但有自己的契约）。
- **仅本地**：不同步、不跨设备漫游、没有协议概念；定义是全局的
  （非按账户），创建顺序固定（不可重排）。
- **无 CLI/agent 动词**（本地视图状态）；**无自定义颜色选择器**
  （仅调色板；后端接受任意 `#RRGGBB`）。
- **批量指派按行循环**（行菜单的折中）；多账户批量按行解析。