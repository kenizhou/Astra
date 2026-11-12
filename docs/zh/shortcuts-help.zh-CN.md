# 快捷键帮助：契约

**范围。** 桌面外壳里只读的键盘快捷键参考浮层，由 `?` 打开。命令面板
之后的 kylins 增补；它列出应用已接线的快捷键，按其生效位置分组。约束
web 客户端的外壳。纯 UI：不新增 wire 类型、不新增 intent，并复用设置里
的快捷键清单作为唯一事实源。

## 面

一个 store 驱动的浮层，不用库，镜像面板的形态：

- wrapper 读 `isOpen`，关闭时返回 null，每次打开挂载一个全新的面。面是
  一个只读对话框（`role="dialog" aria-modal aria-label="Keyboard
  shortcuts"`，`tabIndex={-1}` 配共享焦点钩子，使焦点打开时进入、关闭时
  恢复）。
- 行即设置的 `ShortcutsDetail` 清单，按其 `context` 以首次出现顺序分组
  （Everywhere、Command palette、Search island、Settings、Folder drawer、
  Menu bar、Composer、Calendar event），每行动作与其键帽并列。清单被
  导出并复用，绝不复制。
- 对话框上的 Escape 与背景 mousedown 命中自身目标时关闭。头部提示读作
  「Press ? to reopen. Esc closes.」。浮层以最大高度滚动。
- 浮层处于 `--z-modal` 层并挂载在外壳浮层尾部面板之后，故两者同时打开
  时它赢得平局。它不用面板保留的 spring 缓动；朴素的面板才是合规选择。

## 绑定

**`?`** 切换浮层。opener 是一个 document keydown，匹配逻辑
`event.key === '?'` 且无 ctrl/meta/alt，尊重 `event.defaultPrevented`，在
`event.repeat` 时退出，并拒绝可编辑目标（焦点元素是 input、textarea 或
contenteditable）。它不测试 `shiftKey`：标准布局上 `?` 是 Shift+/，故
物理事件携带 shift 而逻辑键已是 `?`。面板输入框聚焦时，可编辑守卫已挡住
`?`。这是外壳第一个裸字符 document 绑定，故守卫由测试钉住。该绑定列入
设置的快捷键清单（`?` 行，context Everywhere），每个新快捷键都必须在
同一变更里加入该清单。

## 已知缺口，记录而非静默

- 快捷键引擎（可重映射绑定、序列、持久化覆盖、mac/win 显示变体）仍在
  外；清单仍是如今的静态单串 Ctrl 记法。
- 无浮层内搜索或过滤、无最近使用、除关闭外无任何交互。
- 一个打开帮助的面板命令是自然的后续，为保持本切片有界而被推迟。
- 焦点纪律是初始焦点加恢复（面板自己的），而非完整陷阱。浮层仅限外壳，
  如面板。
- Escape 在最上层浮层自己的对话框上应答；既有的 document 级 Escape
  监听器（设置、文件夹抽屉）不检查 `defaultPrevented`，故 Escape 可能
  关闭其下叠放的一层。面板行为相同；仅最上层应答的 Escape 是后续切片
  的共享浮层约定，而非本切片。

## 矩阵

| 规则 | 状态 | 位置 |
|---|---|---|
| 按 context 分组、复用清单的只读浮层 | ✅ | `client/web/src/features/shortcuts-help/ShortcutsHelp.tsx` |
| `?` 打开并切换；可编辑目标守卫；Ctrl+K 与 Ctrl+Shift+P 不受影响 | ✅ | `useShortcutsHelpOpener.ts` |
| 打开焦点进入、关闭恢复；Escape 与背景关闭 | ✅ | `ShortcutsHelp.tsx`、`useDialogFocus.ts` |
| 打开时位于其他模态之上（`--z-modal`，挂在面板之后） | ✅ | `ShortcutsHelp.tsx`、`AppShell.tsx` |
| 设置快捷键清单里的 `?` 行 | ✅ | `ShortcutsDetail.tsx` |
| 快捷键引擎、浮层内搜索、一个打开帮助的面板命令 | N/A | 被推迟，见上述缺口 |