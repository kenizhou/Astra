# 固定搜索设计

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** 搜索岛保留近期查询的会话历史；没有办法把查询留待后用。本 kylins 附加
新增固定（已保存）搜索：把活查询以名字保存、列出已保存搜索、运行其一、重命名、
删除。它是 quick-steps store 形态（zustand + localStorage、防御性解析、记录而不
抛出的持久化）指向已交付的 Intent::Search。纯 client/web：无引擎、无宿主、无
viewmodel、无新 wire 类型。设备本地且有上限。不评测/不提升 pipeline。

## D1：已保存搜索 store

新 client/web/src/features/pinned-searches/pinnedSearchesStore.ts（克隆
quickStepsStore.ts）：基于 localStorage 键 astra.pinnedSearches 的 zustand store，
{ id, name, query } 条目的有上限列表（20），防御性解析（畸形条目丢弃，绝不抛
出），存储失败记录而不抛出。CRUD：save(name, query)、rename(id, name)、remove(id)
及列表。仅保存查询串，绝不保存 scope：search.md 规则 7 钉住 scope 为会话态绝不
持久化，故已保存搜索是查询，而非收窄。空查询的保存被拒（无可固定之物）。允许重
名（id 是身份）；相同 query+name 的保存幂等。

## D2：面板 + 保存入口

新 PinnedSearchesPanel.tsx（列出/运行/重命名/删除，空状态）挂载于 TitleBar.tsx，
紧邻通知历史铃铛模式。运行已保存搜索打开搜索岛（openSearchIsland）并派发
intents.search(query)；scope 以默认打开，恰如键入搜索（已保存查询不携带 scope）。
AmbientIsland.tsx 上的「Save search」入口在有活查询时以名字捕获活查询（字段为空
时不出现）。commands.ts 中可选的palette 命令（打开面板）。不改 ipc.ts（在上
限）；特性自有其类型。

## D3：契约

docs/search.md 新增「Saved searches」段（仅查询持久化、scope 不保存规则、上限、
会话历史区分：岛的近期历史是会话态，固定搜索是其持久化兄弟）。docs/capabilities.md
新增一行（命名仅查询 scope 与会话历史区分）。落变更日志碎片。
docs/porting-backlog.md 把「pinned searches」移入已交付列表。zh 镜像同提交移动。

## 已知缺口，记录而非静默

- 仅查询：scope 不保存（search.md 规则 7），故固定搜索以默认 scope 重跑，而非保
  存时的收窄。
- 设备本地：localStorage，无跨设备同步。
- 已保存查询对当前同步深度重跑，故结果可能与保存时不同（horizon 注记已陈述深度
  策略）。

## 测试先行

每个行为都带着它的红落地。store 测试（解析丢弃畸形条目、上限、save/rename/remove
CRUD、空查询拒绝、幂等保存、持久化失败记录而不抛出）镜像 quickStepsStore.test.ts。
面板组件测试（运行以已保存查询派发 intents.search、重命名/删除、空状态）镜像
QuickStepsManager.test.tsx。岛测试：「Save search」捕获活查询、为空时不出现。
vitest 陷阱适用：测试中段的 store 变更包于 act()，整体 mock 外层函数。离线电池即
证明（纯 web 特性；无活 harness 证明）。