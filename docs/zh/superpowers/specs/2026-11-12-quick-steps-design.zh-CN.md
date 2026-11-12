# 快捷步骤设计：一键邮件宏

**状态。** 2026-11-12 定稿。切片自此提交从 main 分出。

**范围。** 下一个 kylins 增补：快捷步骤，用户定义的宏，一键在所选行上
运行一个邮件动作。kylins 的快捷步骤是「一个归档动词加一个可选的先标
已读」，而非多动作序列引擎；Astra 在它已交付的动词之上移植该形态。
面侧：无新 wire 类型、无新 intent、无引擎/宿主/Rust 工作，故不评测、
不提升 pipeline。每个要素都已存在（逐行批量 intent、批量循环、移动
浮窗的资格规则、设备本地 store 惯用法、ribbon 原语、对话框惯用法）。

## D1：模型与 store

一个 `quickStepsStore`（zustand + localStorage，notificationHistoryStore
惯用法：防御性解析、持久化记录而绝不抛出、模块加载时水合）。一个快捷
步骤是 `{ id, name, action, target?, markRead? }`：

- `action` 是 `archive`、`trash`、`mark-read`、`mark-unread`、`flag`、
  `unflag`、`move` 之一。
- `target`（仅 move）是来自文件夹 store 的一个具体文件夹
  `{ account, folderKey }`；role 目标是命名的后续。
- `markRead` 是可选的先标已读修饰（kylins 的形态）。
- 固定上限五个步骤（kylins 的 MAX_QUICK_STEPS）；store 提供 upsert
  （增/改）、delete 与有序列表。畸形条目在解析时丢弃。

## D2：运行器

`runQuickStep(step, rows)` 把动作映射到既有 intent（rowSwipe 的
动作到-intent 表惯用法），并在行上循环它（批量工具栏的 forEach）：

- archive → `intents.archiveMessage`，trash → `intents.moveToTrash`，
  mark-read/unread → `intents.markRead`，flag/unflag →
  `intents.setFlagged`，move → `intents.moveMessages`（folders.ts 特性
  服务）。
- `markRead` 先运行，作用于合格行，因为被归档的行离开快照（kylins 的
  理由）。
- move 跳过会话（线程）行与外账户行，并带一个计数 toast（swipeToastStore
  惯用法）：`moveMessages` 无法命名一个会话（行菜单自己的规则），且一个
  具体文件夹属于一个账户。
- 运行器作用于 `listStore.selectedKeys`（批量选择），带 ribbon 所用的
  单锚点回退；无选择则不运行任何东西。

## D3：管理器与 ribbon

一个 `QuickStepsManager` 对话框（SweepDialog 外壳：store 门控、
useDialogFocus、Escape/背景关闭、--z-modal）列出已存步骤与一个创建/编辑
的内联表单：一个名字、动作选择、move 的文件夹选择（文件夹 store 之上的
普通 select，无树选择器）、先标已读复选框，以及删除。五上限在达到时带
原因禁用新增。

一个 `QuickStepsRibbonGroup`（RibbonPrimitives 惯用法）挂在 ReadRibbon：
每个已存步骤一个按名字标注的按钮，无选择时禁用，在选择上运行该步骤；
一个 Manage 按钮打开对话框。该组如其他 ribbon 组在紧凑模式下隐藏。

## D4：契约

docs/list-selection.md 约束列表动作；本切片在其中加入一段快捷步骤节
（模型、运行器的先标已读与 move-跳过规则、上限、设备本地持久化），并
镜像到 zh。一个新的 Mail 区 capabilities 行落地，带其 zh 镜像。
porting-backlog 的「快捷步骤」条目随切片解决。一个变更日志碎片落地。

## 已知缺口，记录而非隐藏

- snooze 动作不在内：它依赖未交付的 snooze 基础设施（隐藏/重现调度、
  宿主定时器）。唯一带未交付依赖的 kylins 动作。
- 键盘槽位（quickstep:1..5）不在内：Astra 尚无快捷键引擎（命令面板的
  命名缺口）。
- role 目标与多段序列不在内（kylins 也不做序列）；仅具体文件夹 move。
- 步骤的面板命令是一个便宜的可选后续，而非 v1。
- 步骤是设备本地的（localStorage），不跨设备同步。
- 无撤销（与批量工具栏一致；归档动词止于回收站）。

## 测试先行

每个行为都带着它的红落地。store 测试：upsert/delete、五上限、防御性解析
（畸形丢弃）、localStorage 持久化。运行器测试：每个动作在行上派发精确的
intent 载荷；先标已读在归档动词之前；move 跳过线程行与外账户行并带计数
toast；无选择则不运行任何东西。管理器测试：创建/编辑/删除、上限禁用
新增、move 的文件夹选择。ribbon 测试：每步骤一个按钮、无选择时禁用、
在选择上运行、Manage 打开对话框。fake timers 下任何测试中途 store 变更
都包 act()。无_live_模型证明（纯 UI）；离线电池即证明。