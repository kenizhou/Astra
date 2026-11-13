# VIP 发件人设计

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** 没有办法把发件人标记为重要并一眼看到其邮件。本 kylins 附加新增 VIP 发
件人：把发件人标记为 VIP、在其行上看到徽章、把列表过滤到 VIP 邮件。它是
pin/quick-steps 的 web 形态（zustand + localStorage，纯视图态，无 intent），以发
件人地址为键。纯 client/web：无引擎、无宿主、无 viewmodel、无新 wire 类型。设备
本地。不评测/不提升 pipeline。

## D1：VIP 发件人 store

新 client/web/src/features/vip/vipSendersStore.ts（克隆 pinnedRowsStore.ts）：基
于 localStorage 键 astra.vipSenders 的 zustand store，裸发件人地址的 Set（小写、
去空白；跨账户，不同于 pin 的每账户 rowKeyId），防御性解析（畸形载荷降为空，绝不
抛出），存储失败记录而不抛出。toggle(address) / add / remove / has 及集合。地址
是裸 addr-spec（行的 sender_address），规范化使「Ada <ada@x.com>」与
「ada@x.com」一致。空/空白地址被拒。

## D2：过滤 tab + 行徽章 + 切换

列表过滤新增 'vip' tab：listFilter.ts 的 ListFilter 扩展 'vip'，filteredRows 答
sender_address 在 VIP 集中的行（集合在列表渲染 choke point 读取，pin 模式；过滤
保持客户端侧，绝不为 intent），filterCounts 增 vip 计数。tab 渲染于
ListToolbar.tsx，紧邻 All/Unread/Flagged/Attachments。MessageRow.tsx 对来自 VIP
发件人的行渲染 VIP 徽章（小标记，既有徽章惯用法）。行菜单新增 Add/Remove VIP 批
量切换（克隆 pinItem：基于本地集的纯视图态，无 intent，标签按目标态翻转，批量落
一个态）。不改 ipc.ts（在上限）；特性自有其类型。

## D3：契约

docs/list-selection.md 新增 VIP 发件人段（以发件人为键的集、跨账户范围、徽章、过
滤 tab、批量切换、设备本地持久化）。docs/capabilities.md 新增一行。落变更日志碎
片。docs/porting-backlog.md 把 VIP 移入已交付列表。zh 镜像同提交移动。

## 已知缺口，记录而非静默

- 设备本地：localStorage，无跨设备同步；VIP 集是每浏览器profile 而非每账户（标记
  为 VIP 的发件人跨账户皆 VIP）。
- 徽章与过滤仅读已加载行（列表过滤是快照上的客户端侧，如其他 tab）；无已加载行的
  VIP 发件人在其邮件加载前不显示。
- 无 VIP 邮件的通知或优先路由（后续）；本切片是标记、徽章、过滤。

## 测试先行

每个行为都带着它的红落地。store 测试（解析丢弃畸形、地址规范化、
toggle/add/remove/has、空白拒绝、持久化失败记录而不抛出）镜像
pinnedRowsStore.test.ts。过滤测试（vip tab 答 VIP 发件人的行、计数、空集）扩展
listFilter.test.ts。行菜单测试（批量切换落一个态、标签翻转）镜像 pin 切换测试。
MessageRow 徽章测试（徽章对 VIP 发件人成立，否则不出现）。vitest 陷阱适用：测试
中段 store 变更包于 act()，整体 mock 外层函数。离线电池即证明（纯 web 特性；无活
harness 证明）。