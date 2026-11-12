# 悬停动作设计：邮件行上的快捷动作

**状态。** 2026-11-12 定稿。切片自此提交从 main 分出。

**范围。** 下一个 kylins 增补：邮件行尾缘的快捷动作按钮，悬停时显现。
Astra 已在那里交付加旗与标为已读/未读；本切片以归档和回收站补全该集，
使该槽位键盘可达，并在草稿上抑制它。它移植 kylins 的悬停动作形态
（创始规则把栈/UI 的优势给 kylins）；是增补，不是 mail-calendar 的对齐
债。面侧：它在被悬停的行上派发既有 intent，不新增 wire 类型、不新增
intent，ipc.ts 不动。纯 UI，故不评测、不提升 pipeline。

## D1：动作集与目标

在 `MessageRow` 尾部槽位既有加旗与已读/未读按钮旁新增两个图标按钮：
**归档**（`intents.archiveMessage(row.key)`）与**回收站**
（`intents.moveToTrash(row.key)`），图标已导入。各在**仅被悬停的行**上
派发，带 `stopPropagation`，使按钮点击绝不选中或打开该行，恰如既有的
`markRead`/`toggleFlag` 处理器。标签与行菜单的名词一致（「Archive」、
「Delete」或「Move to Trash」）。

单行目标是刻意的：批量工具栏与行菜单已拥有多选批量（`resolveTargets`
的 Outlook 语义），ribbon 与面板作用于单个所选行。悬停按钮在视觉上属于
一行，故即便该行处于多选之中，它也仅作用于那一行。这与 kylins 及 Astra
自己的 ribbon 先例一致。

邮件动作按钮（归档、回收站、加旗、已读/未读）在 `draft` 行上被抑制
（`draft` prop 已存在），与 kylins 及草稿菜单一致，后者的动词是
打开/删除草稿，而非邮件四动作。加旗行的持久规则保留：加旗行无论悬停
与否都保持其加旗按钮可见。

## D2：显现与可达性

槽位在**悬停 或 选中 或 focus-within** 时显现。如今
`actionsVisible = hovered || selected`；本切片加入 focus-within，使已
选中某行的键盘用户能 Tab 进入按钮，且聚焦时保持显现（行根携带
`group`，槽位用 `group-focus-within` 显现，或等价的焦点捕获状态；由
实现者选择机制）。按钮是真正的 `<button>`，各带 `aria-label` 与常驻的
`focus-visible:ring-2 ring-ring`。行高保持钉住，使显现的按钮绝不撑高
该行（kylins 所防的悬停抖动）。

这不是完整的键盘行导航：行仍是 `role="option"` 而无 `tabIndex`，
ribbon、面板与行菜单仍是邮件动作的主要键盘路径。使行可用方向键聚焦
是更大的列表无障碍切片，被推迟。触摸用户（无悬停）保留滑动动作、行
菜单与批量工具栏；悬停槽位是鼠标与键盘的装饰，不会与滑动相撞（触摸
不产生悬停，且按钮位于被平移的内容内，避开滑动显现层）。

## D3：契约

`docs/list-selection.md` 约束每个交付邮件列表的面；本切片在其中加入一段
简短的悬停动作节（单行目标、显现规则、草稿抑制），并镜像到 zh。一个新的
Mail 区 capabilities 行落地（「邮件行悬停上的快捷动作」），带其 zh 镜像。
porting-backlog 的「悬停动作」条目随切片解决。一个变更日志碎片落地。

## 已知缺口，记录而非隐藏

- 多选批量悬停动作不在内：批量工具栏与行菜单拥有批量。
- 按钮集的用户自定义与主开关（kylins 的设置 KV 配置）是单独的切片。
- 悬停归档/回收站无撤销提示：行菜单与 ribbon 也不提供撤销（仅滑动
  提供）；与它们一致使各面行为相同。把滑动撤销复用于悬停是一个干净的
  后续。
- 暂停与置顶候选不在内（Astra 尚无这两个特性）。
- 完整键盘行导航（方向键可聚焦的行）是更大的列表无障碍切片。

## 测试先行

每个行为都带着它的红落地，基于 MessageRow/MessageList 测试模式（`row()`
fixture、mocked ipc dispatch、`within(listbox)` 作用域）。红：悬停显现
归档/回收站按钮（`fireEvent.mouseEnter`/`mouseLeave`）；focus-within 保持
显现；归档与回收站在消息键与线程键上派发精确的 intent 载荷；按钮点击
不选中或打开该行（`onSelect` 未被调用）；草稿行抑制邮件动作按钮；加旗行
持久不变；既有加旗与已读/未读悬停断言保持绿。fake timers 下任何测试中途
store 变更都包 `act()`。无_live_模型证明（纯 UI）；离线电池即证明。