# 快捷步骤计划：一键邮件宏

规格：docs/superpowers/specs/2026-11-12-quick-steps-design.md。分支
`quick-steps` 自文档提交从 main 分出。每个任务：先红、实现、电池、一个
约定式提交。一个契约变更：list-selection.md 新增快捷步骤节连同其 zh 镜像
在同一提交，capabilities.md 连同其 zh 镜像移动，porting-backlog 条目解决，
并落一个变更日志碎片。

### 任务 1：特性（D1，D2，D3）

**文件：** client/web/src/features/quick-steps/quickStepsStore.ts（模型
{ id, name, action, target?, markRead? }，动作集 archive/trash/mark-read/
mark-unread/flag/unflag/move，具体文件夹 move 目标，五上限，upsert/delete，
localStorage 持久化 + 防御性解析，notificationHistoryStore 惯用法）+
quickStepsStore.test.ts；runQuickStep.ts（rowSwipe 惯用法的动作到-intent
表、forEach 批量循环、先标已读、move 跳过线程 + 外账户行并带计数
swipeToastStore toast、作用于 listStore.selectedKeys 带锚点回退）+
runQuickStep.test.ts；QuickStepsManager.tsx（对话框：列表 + 内联
名字/动作/文件夹选择/先标已读 表单、编辑/删除、上限禁用新增、SweepDialog
外壳）+ QuickStepsManager.test.tsx；QuickStepsRibbonGroup.tsx（每步骤一个
按钮、无选择时禁用、在选择上运行、一个 Manage 按钮；RibbonPrimitives
惯用法）+ QuickStepsRibbonGroup.test.tsx；ReadRibbon.tsx（挂载该组，紧凑
模式隐藏）；一个管理器 store 标志或自挂载（kylins 模式）。

- [ ] 测试先行（store CRUD/上限/解析/持久化；运行器精确载荷 + 先标已读 +
      move 跳过 + 无选择；管理器 创建/编辑/删除/上限；ribbon 按钮/禁用/
      运行/Manage）；随后 store、运行器、管理器与 ribbon。提交
      `feat: quick steps, one-click mail macros`。
- [ ] 行数警戒：每个文件低于 500（store 约 150、运行器约 120、管理器约
      250、ribbon 约 80）。ipc.ts 不动（moveMessages 已住在 folders.ts）。
      无引擎/宿主/Rust 工作。无 snooze 动作、无键盘槽位、无 role 目标、
      无序列。既有 ribbon 测试保持绿。

### 任务 2：契约 + 收尾（D4）

**文件：** docs/list-selection.md（快捷步骤节：模型、运行器的先标已读与
move-跳过规则、上限、设备本地持久化）+ docs/zh/list-selection.zh-CN.md；
docs/capabilities.md（一个新的 Mail 区行，✅）+ docs/zh/capabilities.zh-CN.md；
docs/porting-backlog.md（快捷步骤条目解决）+ 其 zh 镜像；
docs/changelog/unreleased/quick-steps.md（Platforms: desktop，Bump: minor）。

- [ ] 文档 + zh + 行 + backlog + 碎片。提交
      `docs: the quick steps contract`。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。无_live_模型证明（纯 UI）。
- [ ] 分支推送；PR（预期 squash）；CI 观察（ci-ok、desktop、sidecar）；
      squash 合并、删分支、同步 main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的 store + D2 的运行器 + D3 的管理器/ribbon（T1）；D4 的
  契约 + 行 + backlog + 碎片 + 电池 + PR（T2）。
- 风险：先标已读在归档动词之前运行（被归档的行离开快照）；move 跳过
  线程行（moveMessages 无法命名会话）与外账户行并带计数 toast；运行器
  作用于选择带锚点回退，无选择则不运行任何东西；五上限禁用新增；store
  解析丢弃畸形条目；ribbon 组在紧凑模式隐藏；ipc.ts 不动。
- 不评测、不提升 pipeline（纯 UI）。无 snooze（未交付依赖）、无键盘槽位
  （无快捷键引擎）、无 role 目标、无序列、无撤销（与批量工具栏一致）。