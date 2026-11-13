# 固定搜索计划

规格：docs/superpowers/specs/2026-11-13-pinned-searches-design.md。分支
`pinned-searches` 自规格提交从 main 分出。先红、实现、电池、一个约定式提交。一个
契约变更：docs/search.md 新增 Saved searches 段连同其 zh 镜像在同一提交，
capabilities 行连同其 zh 镜像移动，落变更日志碎片，porting-backlog.md 把 pinned
searches 移入已交付列表。纯 client/web；无引擎、宿主、viewmodel 或 ipc.ts 改动。

### 任务：已保存搜索 store + 面板 + 入口 + 契约（D1，D2，D3）

**文件：** 新 client/web/src/features/pinned-searches/pinnedSearchesStore.ts
（zustand + localStorage astra.pinnedSearches，上限 20，防御性解析，记录而不抛出
的持久化；克隆 quickStepsStore.ts）+ pinnedSearchesStore.test.ts；新
PinnedSearchesPanel.tsx（列出/运行/重命名/删除 + 空状态）+
PinnedSearchesPanel.test.tsx；AmbientIsland.tsx 上的「Save search」入口（捕获活查
询，为空时不出现）+ 岛测试；TitleBar.tsx 的面板挂载（紧邻通知历史铃铛）；
commands.ts 可选 palette 命令；docs/search.md（Saved searches 段）+ zh；
docs/capabilities.md（行）+ zh；docs/changelog/unreleased/pinned-searches.md
（Platforms: desktop，Bump: minor）；docs/porting-backlog.md（pinned searches 入
已交付列表）+ 如有镜像则 zh。

- [ ] 在钩子处确认：Intent::Search / intents.search（ipc.ts:273）、
      openSearchIsland（searchIsland.ts:17）、活查询回显（AmbientIsland.tsx）、
      quickStepsStore.ts 形态、TitleBar 铃铛挂载。测试先行（store：解析丢弃畸形、
      上限、CRUD、空查询拒绝、幂等保存、持久化失败记录；面板：运行以已保存查询派
      发 intents.search、重命名/删除、空状态；岛：Save search 捕获活查询、为空时
      不出现）；随后 store + 面板 + 入口 + 文档。提交 `feat: pinned searches`。
- [ ] client/web 不豁免上限：新文件在 features/pinned-searches/，不改 ipc.ts（在
      上限；特性自有类型）。运行已保存搜索打开岛 + 以默认 scope 派发查询（查询不
      携带 scope，search.md 规则 7）。仅保存查询串。vitest 陷阱：测试中段 store
      变更包于 act()，整体 mock 外层函数。注释现在时；无 em/en 破折号；英式英语。
      docs/zh 镜像同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；cargo
      clippy --workspace --all-targets -- -D warnings；cargo test --workspace；
      npm --prefix sidecar run build && npm --prefix sidecar test；npx tsc
      --noEmit && npx vitest run（client/web）；cargo run -p xtask -- gate。若
      plugin 测试以 AlreadyGranted 失败，清理陈旧 /tmp/astra-desktop-plugin-*
      scratch 目录（Windows pid 回收 flake）再重跑。
- [ ] 离线电池即证明（纯 web 特性；无活 harness 证明）。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步 main；
      ledger 清理。

## 自审笔记

- 规格覆盖：D1 的 store + D2 的面板/入口 + D3 的契约 + 电池 + PR（单任务）。
- 风险：scope 不保存规则（search.md 规则 7，固定搜索以默认 scope 重跑）；仅查询
  持久化；上限 + 防御性解析；ipc.ts 在上限（不改）；vitest act()/mock 陷阱。设备
  本地，无跨设备同步（记录缺口）。
- 不评测/不提升 pipeline（面向模型不变）。离线电池即证明。活实证教训：若电池在
  AlreadyGranted 上 flake，清理陈旧 plugin scratch 目录。