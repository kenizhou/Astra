# 悬停动作计划：邮件行上的快捷动作

规格：docs/superpowers/specs/2026-11-12-hover-actions-design.md。分支
`hover-actions` 自文档提交从 main 分出。每个任务：先红、实现、电池、一个
约定式提交。一个契约变更：list-selection.md 新增悬停动作节连同其 zh 镜像
在同一提交，capabilities.md 连同其 zh 镜像移动，porting-backlog 条目解决，
并落一个变更日志碎片。

### 任务 1：悬停动作（D1，D2）

**文件：** client/web/src/components/message-list/MessageRow.tsx（尾部槽位
既有加旗与已读/未读按钮旁的归档 + 回收站图标按钮；在仅被悬停的行上派发
intents.archiveMessage(row.key) / intents.moveToTrash(row.key)，带
stopPropagation；在 hovered || selected || focus-within 时显现；在草稿行
抑制邮件动作按钮；保持加旗行持久与钉住的行高；每按钮 aria-label +
focus-visible 环）；MessageRow.test.tsx 或 MessageList.test.tsx（红：悬停
显现、focus-within、消息键与线程键上精确的归档/回收站载荷、点击不选中/
打开、草稿抑制、加旗持久不变）。

- [ ] 测试先行；随后按钮与显现。提交
      `feat: quick actions on message-row hover`。
- [ ] 行数警戒：MessageRow.tsx 现 234 行；按钮加显现远低于 500（仅在越过
      时拆一个小兄弟模块）。ipc.ts 不动；无新 intent。既有加旗/已读-未读
      悬停断言保持绿。

### 任务 2：契约 + 收尾（D3）

**文件：** docs/list-selection.md（悬停动作节：单行目标、显现规则、草稿
抑制）+ docs/zh/list-selection.zh-CN.md；docs/capabilities.md（一个新的
Mail 区行「邮件行悬停上的快捷动作」，✅）+ docs/zh/capabilities.zh-CN.md；
docs/porting-backlog.md（悬停动作条目解决）+ 其 zh 镜像（若存在）；
docs/changelog/unreleased/hover-actions.md（Platforms: desktop，Bump:
minor）。

- [ ] 文档 + zh + 行 + backlog + 碎片。提交
      `docs: the hover actions contract`。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。无_live_模型证明（纯 UI）。
- [ ] 分支推送；PR（预期 squash）；CI 观察（ci-ok、desktop、sidecar）；
      squash 合并、删分支、同步 main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的集 + 目标 + D2 的显现（T1）；D3 的契约 + 行 + backlog +
  碎片 + 电池 + PR（T2）。
- 风险：按钮点击绝不选中/打开该行（stopPropagation）；目标是仅被悬停的
  行，绝非多选；草稿行抑制邮件四动作；加旗行持久与钉住的行高保留；
  focus-within 显现不得使行本身可聚焦（ribbon/面板/菜单仍是键盘路径）；
  滑动通道必须保持畅通（按钮在被平移的内容内）。
- 不评测、不提升 pipeline（纯 UI）。无撤销（与行菜单和 ribbon 一致；
  滑动拥有撤销）。