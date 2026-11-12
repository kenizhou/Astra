# 命令面板设计：第一个 kylins 增补

**状态。** 2026-11-12 定稿。切片自此提交从 main 分出。

**范围。** backlog 命名的 kylins 增补中的第一个
（docs/porting-backlog.md:70-75），如今在所有者「跑完队列」的常驻授权下
选取。一个键盘驱动的命令面板：列出应用各项动作的全局浮层，边输入边
过滤，运行所选动作。它移植 kylins 的 UI 形态（创始规则把栈/UI 的优势
给 kylins）；它是增补，不是 mail-calendar 的对齐债，新增一个 capabilities
行而非关闭某个部分行。纯 UI：不改动面向模型的面，故不附带评测运行，
也不提升 pipeline 版本。

## D1：面板面

一个自定义浮层，不引入新依赖（kylins 不用任何库；Astra 也没有）。
`client/web/src/features/command-palette/` 下三件：

- `CommandPalette.tsx`：浮层。一个文本输入框罩在命令的扁平列表上。
  对标签做大小写不敏感的子串过滤（kylins 自己「不做模糊评分，YAGNI」
  的纪律）；ArrowUp/Down 环绕；Enter 运行所选命令并关闭；Escape 与
  背景点击关闭；选择索引随列表收缩而钳制；每次打开重置查询与选择；
  `scrollIntoView({block:'nearest'})` 为 jsdom 加保护。无障碍镜像 kylins：
  `role="dialog" aria-modal`，输入框 `aria-controls`/`aria-activedescendant`，
  `ul role="listbox"` / `li role="option" aria-selected`，一个空状态。
  每行携带图标与标签；被禁用的命令展示其原因（ContextMenuItem 的
  诚实禁用惯用法），绝不静默空操作。
- `useCommandPalette.ts`：一个微型 zustand store（`isOpen`、`open`、
  `close`、`toggle`），加模块作用域的 `openCommandPalette`/
  `closeCommandPalette`/`toggleCommandPalette`，使非 React 代码也能切换。
- `commands.ts`：注册表，一份 `{ id, label, category, icon, run, disabled? }`
  列表，由既有 store getter 与 intent 构建。每个命令映射到 Astra 已交付
  的动作；面板不新增 wire 类型、不新增 intent（故 ipc.ts 不动，遵规则书
  陷阱）。

浮层挂载在 `AppShell` 的浮层尾部（SettingsScreen 旁）。其入场动效用
`--ease-spring`，设计系统为面板独占保留的令牌
（docs/design-system.md:39-41）。

## D2：打开方式与命令集

**Ctrl+Shift+P** 打开面板（与 kylins 一致）。Ctrl+K 仍归搜索岛
（AmbientIsland.tsx:113-126）；面板的 document keydown 尊重
`event.defaultPrevented`。该绑定在同一变更里落入静态
`ShortcutsDetail.tsx` 的 SHORTCUTS 表（其注释把新快捷键绑定到该清单）。

命令集，各映射到一个既有动作：

- **前往**：Mail / Calendar / People（`setShellView`）；Inbox / Sent /
  Drafts / Archive / Trash（在当前账户里按 role 找文件夹，再用 FolderTree
  所用的 `select` + `dispatch(intents.selectFolder)` 对）；统一收件箱
  （`selectUnified` + `selectAccount(null)`）。
- **撰写**：新邮件（`openNewMessageComposer`，与 ribbon 相同的无账户
  门控）；对已打开邮件的回复 / 全部回复 / 转发
  （`intents.replyTo`/`replyAllTo`/`forward`，带携带的附件），无邮件打开
  时禁用并给出原因。
- **邮件动作**作用于所选行：标为已读 / 未读、加旗 / 去旗、归档、删除
  （移入回收站），各为 ribbon 与行菜单已派发的 intent，无行选中时禁用
  并给出原因。
- **搜索邮件**：通过一个小的新接缝打开搜索岛（岛的展开态如今是局部的；
  一个模块作用域 opener 或一个 uiStore 标志，由实现者选择，保持最小）。
- **应用**：设置（`setSettingsOpen`）；添加账户（`setAccountSetupOpen`）；
  循环主题（经 `intents.setTheme(nextThemeMode(...))`，绝不单独用局部
  setter，以便持久化）；切换代理面板；切换 AI 文档面板；立即同步邮件
  （`intents.refreshMail`）。
- **视图**：扁平 / 线程化（`intents.setViewMode`）；阅读窗格 右 / 下 /
  关（`appearance.setReadingPosition`，设备本地）。

**发送绝不成为面板命令。** 发送纪律是确定性且仅人工的（规则书，
Security）；发送走撰写器自己的提交路径。永久删除与垃圾邮件被推迟
（它们各自带 confirm/门控）；撤销在 Astra 无对应物；逐账户切换、发件箱
导航、密度/过滤命令被推迟，以保持第一个切片有界。

## 已知缺口，记录而非隐藏

- 完整快捷键引擎（可重映射绑定、序列、持久化覆盖、kylins 的
  services/shortcuts 栈）不在内：面板交付一个固定绑定，形态为将来的
  keyMap 保持开放。
- 快捷键帮助浮层是单独的 backlog 项；Astra 的静态 ShortcutsDetail 已承载
  清单。
- 无最近使用、模糊评分或分组标题（kylins 自己也不交付这些）。
- 上述被推迟的命令（逐账户切换、发件箱、密度/过滤、垃圾邮件、永久
  删除）被命名，而非静默。

## 测试先行

每个行为都带着它的红落地。组件测试（vitest + @testing-library/react，
SettingsScreen.test.tsx 的浮层模式）：过滤收窄列表；ArrowUp/Down 环绕、
Enter 运行所选命令（经 mocked ipc 断言正确的 intent）；Escape 与背景
点击关闭；被禁用命令展示原因且不派发；重开时重置查询与选择；空状态
渲染。store 测试：open/close/toggle 与模块作用域助手。fake timers 下
任何测试中途的 zustand 变更都包 `act()`（规则书陷阱）。键盘 opener
测试：Ctrl+Shift+P 打开，Ctrl+K 仍打开岛。无_live_模型证明（纯 UI）；
离线电池即证明。