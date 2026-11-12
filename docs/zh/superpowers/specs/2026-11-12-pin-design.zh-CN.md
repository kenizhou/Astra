# 置顶设计：把置顶行浮到顶部

**状态。** 2026-11-12 定稿。切片自此提交从 main 分出。

**范围。** 下一个 kylins 增补：置顶，把所选行浮到列表顶部。kylins 的
置顶完全是客户端的（一个持久化 id 集加一个渲染时的纯分区）；Astra 在
列表唯一的渲染咽喉点移植该形态。它是视图状态，如密度或过滤标签页：无
引擎写入、无新 intent、无 wire 类型、无 ipc.ts 改动，故不评测、不提升
pipeline。置顶集按行身份（account + key）键控，相对 kylins 的全局裸 id
集是一个刻意的逐账户改进。

## D1：置顶集与叠加

一个 `pinnedRowsStore`（zustand + localStorage，notificationHistoryStore /
quickStepsStore 惯用法：防御性解析、持久化记录而绝不抛出、加载时水合）。
集合持有行身份（`rowKeyId`：`message:account:key` 或
`thread:account:thread_id`），故置顶免费地按账户作用域。store 提供
`toggle(key)`（乐观）与 `isPinned(key)`。

一个纯而稳定的 `partitionPinnedFirst(rows, pinned)` 把置顶行浮到顶部，
**在每个分区内保留引擎顺序**（Astra 的引擎顺序是带平局裁决的全序；它
不复制 kylins 的日期重排）。叠加在 `MessageList` 的 `shownRows` 咽喉点
施加（过滤之后、日期头之前），故它传播到渲染项、Shift 范围与全选，而
不触碰快照、引擎顺序、选择携带（基于键）或 ShowMore（它比较原始行
计数）。搜索分支保持自己的顺序（叠加仅限文件夹/统一列表，kylins 的
先例）。

## D2：开关与指示器

一个行菜单**置顶 / 取消置顶**条目（标签按行的置顶状态翻转，在「在新
窗口打开」之后，经菜单的 forEach 可批量），以及行上一个**持久**的置顶
指示器（尾部图标簇中旁于回形针的一个 `PinIcon`，绝非仅悬停，带一个
「Pinned」title/aria-label）。行直接订阅置顶 store（不经虚拟器
prop-drilling）。`PinIcon` 加入 icons.tsx（已安装包中的 Hugeicons
Pin02Icon）。

## D3：契约

docs/list-selection.md 约束列表面；本切片在其中加入一段置顶节（逐账户
置顶集、保留引擎顺序的稳定分区、仅文件夹叠加、窗口化诚实性），并镜像
到 zh。一个新的 Mail 区 capabilities 行落地，带其 zh 镜像。
porting-backlog 的「置顶」条目随切片解决。一个变更日志碎片落地。

## 已知缺口，记录而非隐藏

- 置顶仅在已加载快照窗口内浮动；超出可见上限的行上的置顶在 ShowMore
  加载它之前不可见（kylins 相同的限制；不抓取）。
- 叠加仅限文件夹/统一列表；搜索结果保持其顺序。
- 扁平模式消息置顶与线程模式会话置顶是不同身份；两者之间的映射需要
  扁平行上的线程 id（引擎工作），一个命名的后续。置顶一个线程行即置顶
  所显示的整个会话。
- v1 无「已置顶」过滤标签页（便宜但为保持切片紧凑而推迟）。
- 置顶是设备本地的（localStorage），不跨设备同步。

## 测试先行

每个行为都带着它的红落地。store 测试：toggle/isPinned、逐账户键控、
防御性解析（畸形丢弃）、localStorage 持久化。分区测试：置顶行浮到最前、
每个分区内保留引擎顺序、非变异、空置顶集为恒等。列表测试（一个新的
MessageList.pin.test.tsx，既有文件接近上限）：置顶行最先渲染并带指示器、
叠加保持选择与 ShowMore 完好、搜索模式不受影响。菜单测试（一个新的
rowMenu.pin.test.ts）：置顶/取消置顶标签翻转、开关写入 store、批量置顶。
fake timers 下任何测试中途 store 变更都包 act()。无_live_模型证明（纯
UI）；离线电池即证明。