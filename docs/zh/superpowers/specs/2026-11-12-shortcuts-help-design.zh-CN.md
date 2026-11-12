# 快捷键帮助设计：键盘参考浮层

**状态。** 2026-11-12 定稿。切片自此提交从 main 分出。

**范围。** 命令面板之后的下一个 kylins 增补：一个只读的键盘快捷键参考
浮层，由 `?` 打开。它列出应用已接线的快捷键，按其生效位置分组，Escape
或背景点击关闭。它复用设置里的快捷键清单作为唯一事实源，并镜像面板的
微型 store 与 opener 模式。落地它即关闭命令面板命名的「快捷键帮助浮层」
缺口。纯 UI：无 wire 类型、无 intent、无面向模型改动，故不评测、不提升
pipeline。

## D1：浮层

`client/web/src/features/shortcuts-help/` 下一个 store 驱动的浮层：

- `ShortcutsHelp.tsx`：wrapper 读 `isOpen`，关闭时返回 null，每次打开
  挂载一个全新的面。面是一个只读对话框（`role="dialog" aria-modal
  aria-label="Keyboard shortcuts"`，`tabIndex={-1}` 配 `useDialogFocus`，
  使焦点打开时进入、关闭时恢复，Escape 可靠应答）。它按清单行的
  `context` 以首次出现顺序分组列出（Everywhere、Command palette、
  Search island、Settings、Folder drawer、Menu bar、Composer、
  Calendar event），每行动作与其键帽并列。头部提示读作「Press ? to
  reopen. Esc closes.」。对话框上的 Escape 与背景 mousedown 命中自身
  目标时关闭。浮层处于 `--z-modal` 并挂载在外壳浮层尾部面板之后，故
  两者同时打开时它赢得平局。它以最大高度滚动。它不用 `--ease-spring`
  或 `palette-pop-in`（设计系统为面板独占保留那些）；朴素的面板淡入
  或无动效才是合规选择。
- `useShortcutsHelp.ts`：面板 store 的逐字镜像（`isOpen`/`open`/`close`/
  `toggle` 加三个模块作用域动词）。

数据即设置的 `ShortcutsDetail.tsx` 清单：导出 `SHORTCUTS` 与
`ShortcutEntry`（表留在原处作为唯一源），并复用其键帽样式。无快捷键
列表的第二份副本。

## D2：`?` 绑定

`useShortcutsHelpOpener.ts` 镜像面板 opener，带上裸字符所需的差异：它
匹配逻辑 `event.key === '?'` 且无 ctrl/meta/alt，尊重
`event.defaultPrevented`，在 `event.repeat` 时退出，并拒绝可编辑目标
（焦点元素是 input、textarea 或 contenteditable，即
`closest('input, textarea, [contenteditable="true"]')` 惯用法）。它不测试
`shiftKey`（标准布局上 `?` 是 Shift+/，故物理事件携带 shift 而逻辑键已
是 `?`）。切换语义：第二个 `?` 关闭。面板输入框聚焦时，可编辑守卫已
挡住 `?`。这是 Astra 第一个裸字符 document 绑定，故守卫是新的，由测试
钉住。

`?` 行在同一变更里加入 `ShortcutsDetail` 清单（其头部注释把每个新快捷键
绑定到该表）：`{ keys: '?', action: 'Open the shortcuts help', context:
'Everywhere' }`。清单测试钉子由 16 行移为 17 行，并新增一个 `?` 断言。

## D3：关闭面板的缺口

命令面板把快捷键帮助浮层记录为一个被推迟的缺口。落地它在同一变更里
更新：面板的 capabilities 注记、`docs/command-palette.md` 的已知缺口与
矩阵（快捷键帮助行由被推迟移为已交付，指向 shortcuts-help.md），及其
zh 镜像。一个新的 Mail 区快捷键帮助 capabilities 行落在面板行旁，带其
zh 镜像。

## 已知缺口，记录而非隐藏

- 快捷键引擎（可重映射绑定、序列、持久化覆盖、mac/win 显示变体）仍在
  外；清单仍是如今的静态单串 Ctrl 记法。
- 无浮层内搜索或过滤、无最近使用、除关闭外无任何交互。
- 一个打开帮助的面板命令是自然的后续，但会拓宽本切片；被推迟，命名。
- 焦点纪律是初始焦点加恢复（面板自己的），而非完整陷阱。浮层仅限外壳，
  如面板（不在弹出撰写器或阅读窗）。

## 测试先行

每个行为都带着它的红落地。浮层测试（CommandPalette.test.tsx 模式）：
关闭渲染 null；对话框被命名且模态；行按 context 分组并复用清单；z 层
钉子（`--z-modal`，非背景层）；打开焦点进入、关闭恢复；Escape 关闭；
背景自身目标关闭而内部点击不关闭。store 测试：open/close/toggle 与模块
动词。opener 测试：`?` 打开、第二个 `?` 关闭；被认领的键
（defaultPrevented）不被理会；input、textarea 或 contenteditable 聚焦时
`?` 不触发；Ctrl+K 与 Ctrl+Shift+P 不受影响。清单钉子移为 17 并带 `?`
断言。fake timers 下任何测试中途 store 变更都包 `act()`。无_live_模型
证明（纯 UI）；离线电池即证明。