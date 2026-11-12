# 快捷键帮助计划：键盘参考浮层

规格：docs/superpowers/specs/2026-11-12-shortcuts-help-design.md。分支
`shortcuts-help` 自文档提交从 main 分出（main 已携带命令面板，PR #71）。
每个任务：先红、实现、电池、一个约定式提交。一个契约变更：
capabilities.md 连同其 zh 镜像在同一提交移动，新增 docs/shortcuts-help.md
（+ zh）作为 Source，面板的被推迟缺口在同一变更关闭，并落一个变更日志
碎片。

### 任务 1：浮层特性（D1，D2）

**文件：** client/web/src/features/settings/ShortcutsDetail.tsx（导出
SHORTCUTS + ShortcutEntry，表留作唯一源；加 `?` 行 { keys: '?', action:
'Open the shortcuts help', context: 'Everywhere' }；可选导出 Kbd）；
ShortcutsDetail.test.tsx（钉子 16 到 17 行 + 一个 `?` 断言）；
client/web/src/features/shortcuts-help/useShortcutsHelp.ts（面板 store
镜像）+ useShortcutsHelp.test.ts；useShortcutsHelpOpener.ts（逻辑 `?`、
无 ctrl/meta/alt、尊重 defaultPrevented、event.repeat 退出、可编辑目标
守卫、切换）+ useShortcutsHelpOpener.test.tsx；ShortcutsHelp.tsx（按
context 分组的只读对话框、tabIndex=-1 上的 useDialogFocus、对话框上的
Escape、背景自身目标关闭、--z-modal、最大高度滚动、无 spring）+
ShortcutsHelp.test.tsx；components/layout/AppShell.tsx（opener 置于
useCommandPaletteOpener 旁；浮层挂在 CommandPalette 之后）。

- [ ] 测试先行（浮层 关闭/null、命名模态、分组行、z 层、焦点进出、
      Escape、背景；store；opener `?` 打开/切换、被认领的键、可编辑目标
      守卫、Ctrl+K 与 Ctrl+Shift+P 不受影响；清单钉子 17）；随后实现。
      提交 `feat: the shortcuts help overlay`。
- [ ] 行数警戒：每个文件低于 500（浮层约 120、opener 约 40）。无
      --ease-spring 或 palette-pop-in（面板独占）。无快捷键列表的第二份
      副本。ipc.ts 不动。

### 任务 2：契约 + 收尾（D3）

**文件：** docs/shortcuts-help.md（契约：面、`?` 绑定及其守卫、清单复用、
被推迟项）+ docs/zh/shortcuts-help.zh-CN.md；docs/capabilities.md（面板
行旁一个新的 Mail 区行，⬜ 到 ✅；面板行注记从其被推迟列表去掉
shortcuts-help）+ docs/zh/capabilities.zh-CN.md；docs/command-palette.md
（已知缺口与矩阵：shortcuts-help 由被推迟移为已交付，指向
shortcuts-help.md）+ docs/zh/command-palette.zh-CN.md；
docs/changelog/unreleased/shortcuts-help.md（Platforms: desktop，Bump:
minor）。

- [ ] 文档 + zh + 行 + 面板缺口关闭 + 碎片。提交
      `docs: the shortcuts help contract`。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。无_live_模型证明（纯 UI）。
- [ ] 分支推送；PR（预期 squash）；CI 观察（ci-ok、desktop、sidecar）；
      squash 合并、删分支、同步 main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的浮层 + D2 的 `?` 绑定（T1）；D3 的缺口关闭 + 契约 + 行
  + 碎片 + 电池 + PR（T2）。
- 风险：`?` opener 是 Astra 第一个裸字符 document 绑定，故可编辑目标
  守卫必须挡住 input/textarea/contenteditable（及面板输入框）里的输入；
  它不得测试 shiftKey；必须尊重 defaultPrevented 且不理会 Ctrl+K /
  Ctrl+Shift+P。浮层必须用 --z-modal 并挂在面板之后（z 平局）。清单保持
  唯一源（导出，而非副本）。面板的被推迟提及必须全部移动（capabilities
  注记、command-palette.md 缺口 + 矩阵、zh）。
- 不评测、不提升 pipeline（纯 UI）。无 spring 缓动（面板独占）。