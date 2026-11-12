# 引用折叠计划：折叠被引用的原文

规格：docs/superpowers/specs/2026-11-12-quote-collapse-design.md。分支
`quote-collapse` 自文档提交从 main 分出。每个任务：先红、实现、电池、一个
约定式提交。一个契约变更：reading-zoom.md 新增引用折叠节连同其 zh 镜像在
同一提交，capabilities.md 连同其 zh 镜像移动，porting-backlog 条目解决，
并落一个变更日志碎片。

### 任务 1：变换与接线（D1，D2）

**文件：** client/web/src/components/reading-pane/collapseQuoted.ts（纯
变换：尾部 blockquote 链 + 其前的「On ... wrote:」归属 + 一个尾部
.gmail_quote，折叠进一个带固定常量标签的折叠 details/summary；两个安全
空操作；片段被包裹，绝不重新解析或重新编码）；collapseQuoted.test.ts
（纯变换红，精确输出钉子）；MessageHtml.tsx（在组合点于 wrapBodyDocument
之前对被门控的 html 调用 collapseQuoted；不在 wrapBodyDocument 之内，故
打印保持完整正文）；MessageHtml.test.tsx（组件红：srcdoc 携带 details、
opt-in 重写在其中的恢复 src、敌意片段逐字通过、打印不折叠）。

- [ ] 测试先行；随后变换与接线。提交
      `feat: collapse the quoted original in the reading pane`。
- [ ] 行数警戒：collapseQuoted.ts 很小（约 40 行）；MessageHtml.tsx 现 260
      远低于 500。无消毒器改动、无 CSP/sandbox 改动、无新 intent、ipc.ts
      不动。既有 wrapBodyDocument 与 enableRemoteImages 字节钉子保持绿。

### 任务 2：契约 + 收尾（D3）

**文件：** docs/reading-zoom.md（引用折叠节：检测、默认折叠的折叠、打印
豁免、逐字保证）+ docs/zh/reading-zoom.zh-CN.md；docs/capabilities.md
（阅读行旁一个新的 Mail 区行，✅）+ docs/zh/capabilities.zh-CN.md；
docs/porting-backlog.md（引用折叠条目解决）+ 其 zh 镜像；
docs/changelog/unreleased/quote-collapse.md（Platforms: desktop，Bump:
minor）。

- [ ] 文档 + zh + 行 + backlog + 碎片。提交
      `docs: the quote collapse contract`。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。无_live_模型证明（纯 UI）。
- [ ] 分支推送；PR（预期 squash）；CI 观察（ci-ok、desktop、sidecar）；
      squash 合并、删分支、同步 main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的变换 + D2 的接线（T1）；D3 的契约 + 行 + backlog + 碎片
  + 电池 + PR（T2）。
- 风险：折叠必须逐字包裹片段（无重新编码；敌意逐字测试钉住它）；注入
  标记是固定常量（无发件人派生属性）；远程图像占位符必须存活且 opt-in
  重写必须仍能在 details 内找到它们；打印必须保持完整正文（折叠留在
  wrapBodyDocument 之外）；CSP 与 sandbox 不移动；整篇引用与无引用空操作
  保持。
- 不评测、不提升 pipeline（纯 UI）。无消毒器改动（折叠是消毒后注入的
  应用自有标记）。