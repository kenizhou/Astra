# 命令面板计划：第一个 kylins 增补

规格：docs/superpowers/specs/2026-11-12-command-palette-design.md。分支
`command-palette` 自文档提交从 main 分出。每个任务：先红、实现、电池、
一个约定式提交。一个契约变更：capabilities.md 连同其 zh 镜像在同一提交
移动，新增 docs/command-palette.md（+ zh）作为 Source，并落一个变更日志
碎片。

### 任务 1：面板特性（D1，D2）

**文件：** client/web/src/features/command-palette/CommandPalette.tsx
（浮层：子串过滤、环绕方向键、Enter 运行并关闭、Escape/背景关闭、
listbox/option 无障碍、空状态、打开即重置、jsdom 安全的 scrollIntoView、
诚实禁用行、--ease-spring 入场）；useCommandPalette.ts（微型 store +
模块作用域 open/close/toggle）；commands.ts（{ id, label, category, icon,
run, disabled? } 注册表，取自既有 store getter 与 intent；无新 wire 类型、
无新 intent）；components/layout/AppShell.tsx（挂到浮层尾部）；document
keydown opener（Ctrl+Shift+P，尊重 defaultPrevented；Ctrl+K 仍归岛）；
小的搜索岛 opener 接缝；features/settings/ShortcutsDetail.tsx（SHORTCUTS
表里的绑定行）；CommandPalette.test.tsx + useCommandPalette.test.ts（红）。

- [ ] 测试先行（过滤、键盘导航 + 执行经 mocked ipc 派发正确 intent、
      escape/背景关闭、禁用门控、打开即重置、空状态、Ctrl+Shift+P
      opener）；随后组件、store、注册表、挂载与接缝。提交
      `feat: the command palette`。
- [ ] 行数警戒：每个文件低于 500；若 commands.ts 越过，按类别把注册表
      拆到子模块。ipc.ts 不动。任何地方都无发送命令。文件夹名绝不在
      大小写意义下相撞（CommandPalette.tsx 旁 useCommandPalette.ts 安全）。

### 任务 2：契约 + 收尾

**文件：** docs/command-palette.md（契约：面、绑定、命令集、发送拒绝、
被推迟的命令）+ docs/zh/command-palette.zh-CN.md；docs/capabilities.md
（主表里一个新的 Mail 区行，随完整交付 ⬜ 到 ✅）+
docs/zh/capabilities.zh-CN.md；docs/changelog/unreleased/command-palette.md
（Platforms: desktop，Bump: minor）。

- [ ] 文档 + zh + 行 + 碎片。提交 `docs: the command palette contract`。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。无_live_模型证明（纯 UI）。
- [ ] 分支推送；PR（预期 squash）；CI 观察（ci-ok、desktop、sidecar）；
      squash 合并、删分支、同步 main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的面 + D2 的绑定与命令集（T1）；契约 + 行 + 碎片 + 电池
  + PR（T2）。
- 风险：面板不得从搜索岛偷走 Ctrl+K（opener 尊重 defaultPrevented 且只
  绑 Ctrl+Shift+P）；邮件动作与撰写命令在目标缺失时必须禁用并给原因，
  绝不向空派发；主题命令必须经设置 intent 持久化，而非局部 setter；
  发送绝不出现；搜索接缝不得破坏岛自己的 Ctrl+K。
- 不评测、不提升 pipeline（纯 UI，无面向模型改动）。发送拒绝记录于契约。