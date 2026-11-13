# 垃圾邮件阻止设计

**状态。** 2026-11-14 定稿。切片自此提交从 main 分出。

**范围。** 没有办法让某发件人静音。本 kylins 附加新增垃圾邮件阻止：阻止某发件人，
使其邮件不再触发新邮件通知。这是仅通知抑制，锁定 kylins 参考的红线：阻止列表驱动
新邮件抑制，别无其他；把邮件归为垃圾仍是 Junk 动词的事（可选举报腿复用已交付的
垃圾举报）。被阻止发件人的邮件仍落入 Inbox。阻止列表是一个偏好字段；抑制是既有新
邮件队列门中的一个确定性腿（非 AI  incoming pass，故无 eval/pipeline bump）。无
引擎改动；无新 store；不评测/不提升 pipeline。

## D1：阻止列表偏好 + 抑制门

Preferences 上的 blocked_senders: BTreeSet<String> 字段（sender_names/signatures
模式，空则 skip_serializing_if），地址规范化小写/去空白/从显示形式提升 addr-spec
（VIP normaliseAddress 惯用法）。BlockSender / UnblockSender SettingsIntent 臂走
共享 set_setting setter。新邮件队列在 arm 刷新阻止集（如该 pass 已读 sync 设置），
qualifies 增「发件人未被阻止」腿，故被阻止发件人既不触发 OS 通知也不触发历史行
（kylins「阻止 id 在历史前消费」语义）。阻止列表读失败绝不抑制通知（过滤器失败开
放，kylins 不变量）。被阻止发件人甚至胜过 VIP 静音绕过（参考顺序）。

## D2：web 阻止面

行菜单「Block sender」项（精简，紧邻 vipItem；rowMenu.ts 接近上限，故对话框自成
组件）。独立 BlockSenderDialog：阻止地址，加可选「Also report this conversation
as junk」复选框默认开，复用已交付垃圾举报动词（举报失败绝不撤销阻止）。设置管理段
列出被阻止发件人并可解除（Astra 无「Mail」设置类别；段落入既有类别，记录的细节）。
阻止列表设备本地（偏好文件）；web 经设置 wire 读（无新 ipc.ts 类型，字段走既有设
置快照）。

## D3：契约

新 docs/junk-blocking.md（仅抑制红线、阻止列表偏好、门腿、可选举报腿、失败开放不
变量、记录缺口）+ zh；capabilities 行；变更日志碎片；porting-backlog 已交付列表移
动。zh 镜像同提交移动。

## 已知缺口，记录而非静默

- 仅抑制：被阻止发件人邮件仍落 Inbox；自动归档到 Junk 是命名非目标（参考红线；归
  档仍是 Junk 动词的事）。若 owner 要自动归档，那是独立、更侵入的切片。
- 阻止列表设备本地（偏好文件），不漫游；CLI 不共享（snooze/reminders 先例）；无新
  CLI 动词（VIP/pin 先例，偏好上的纯视图态）。
- 设置管理段落入既有类别（Astra 无「Mail」类别），表面细节。

## 测试先行

每个行为都带着它的红落地。核心测试（设置 intent 往返、地址规范化、队列分类抑制被
阻止发件人的 notice、未被阻止发件人仍合格、阻止列表读失败开放、被阻止胜过 VIP 顺
序）。web 测试（阻止对话框阻止 + 可选举报腿、举报失败绝不撤销阻止、管理段解除）。
一个门控活 harness 实证（junk_blocking_proof.rs：阻止发件人、经 harness 提交路径
从其送达邮件、断言队列无 notice；解除、再送达、断言 notice 返回；撤回副本）。无活
模型证明；离线电池即证明。