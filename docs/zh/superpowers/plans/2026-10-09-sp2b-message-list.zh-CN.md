# SP2b 消息列表实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-09-sp2b-message-list.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** MailboxList 快照长出消息行（平铺与线程）、加 RowRef 上的
MarkRead 与 SetFlagged、以虚拟化与动作把列表面板做实、对 harness 种子邮件
活体验证。

**架构：** astra-viewmodel 增量长快照；读取经 engine mail_window /
mail_on_threads 走既有行缓存与代际守卫；写入经 engine edit_mail 丢弃结果、
重同步；两个新 serde Intent；SPA 列表面板用 @tanstack/react-virtual 做实。

**技术栈：** 既有 Rust workspace；client/web 加
@tanstack/react-virtual（kylins 版本）。

**Spec：** `docs/superpowers/specs/2026-10-09-sp2b-message-list-design.md`

## 全局约束

- 沿 SP1/SP2a 家规：SPDX、500 行上限、deny-everything lints、提交前 gate、
  conventional commits、无破折号、英式英语、zh 镜像同提交、只允许增量线上
  变更（既有夹具必须原样通过）。
- 移植源根 MC = D:/Projects/mailclient/opensource/mail-calendar；kylins 根
  KY = D:/Projects/mailclient/kylins。
- 前序计划的适配规则 AR1-AR7 适用。

---

### 任务 1：viewmodel 行与快照增长

**文件：**
- 修改：`crates/astra-viewmodel/src/{view.rs, view_tests.rs, folders.rs}`；
  若触上限则新建 `crates/astra-viewmodel/src/rows.rs`（+测试）
- 移植自：MC `mailcal-viewmodel/src/view.rs` 的消息行类型。

**接口：**
- 产出：`MessageRowView`（行键：message 或 thread id、发件人显示名与地址、
  主题、发送日期、未读、旗标、has_attachments、线程模式下的 thread_count、
  account id）带 serde（Option/默认字段 skip 或 default）；`MailboxListSnapshot`
  增加 `rows: Vec<MessageRowView>`、`selection: Option<RowKey>`、
  `visible_limit`、`view_mode`——全部 `#[serde(default)]` 使旧夹具可解析。

- [ ] 步骤 1：测试先行——旧夹具（无新字段）往返；带行的新夹具钉死确切
  线上形状；view_mode 以 snake_case 序列化。
- [ ] 步骤 2：移植行投影类型（MC 行形状裁到这些字段）；日期按 MC 的行日期
  类型镜像。
- [ ] 步骤 3：测试/clippy/fmt/gate 绿；提交
  `feat: grow the mailbox snapshot with message rows`。

### 任务 2：RowRef、新 Intent、列表读取

**文件：**
- 修改：`crates/astra-app/src/{reference.rs, protocol/intent.rs,
  protocol_tests.rs, dispatch.rs, snapshot.rs, row_cache.rs,
  accounts.rs（若需）}` + 测试；`client/web/src/services/ipc.ts`（+测试）
- 移植自：MC `mailcal-app/src/{reference.rs（RowRef/MessageRef/ThreadRef）,
  dispatch.rs（MarkRead/SetFlagged 臂）, snapshot.rs（窗口化/列表投影）}`。

**接口：**
- 产出：`RowRef::{Message, Thread}` 带 from_parts；`MessageRef`、
  `ThreadRef`；`Intent::MarkRead { row: RowRef, read: bool }`、
  `Intent::SetFlagged { row: RowRef, flagged: bool }`；按选中文件夹填充快照
  行（平铺：engine mail_window；线程：engine mail_on_threads）；ShowMore 增长；
  SetViewMode 重投影并重置；selection 字段初始由文件夹选择驱动。
- IPC：ipc.ts 的 Intent 联合类型增加两个变体（线上测试按 Rust 测试钉死
  `{"mark_read":{"row":{"message":{...}},"read":true}}` 风格字面量）。

- [ ] 步骤 1：protocol_tests 先行：两个变体确切线上往返；RowRef from_parts
  拒绝残缺。
- [ ] 步骤 2：dispatch_tests 先行（内存引擎 + 假件）：SelectFolder 重建行；
  ShowMore 增长；视图切换重投影并重置；MarkRead/SetFlagged 经 edit_mail
  假件写穿、快照以翻转后的行状态重发布。
- [ ] 步骤 3：实现（移植 MC 臂/投影；engine edit_mail 是写缝——先读锁定
  rev 上 edit_mail 的签名再适配 MC 调用）。
- [ ] 步骤 4：ipc.ts + 线上测试；全绿；提交
  `feat: add message list reads and read/flag intents`。

### 任务 3：前端列表面板

**文件：**
- 修改：`client/web/src/components/layout/{AppShell.tsx, MessageList.tsx}`
  （替换占位）、新建 `client/web/src/components/message-list/{MessageRow.tsx,
  Monogram.tsx, ListToolbar.tsx}`；`client/web/src/stores/{folderStore.ts
  或新 listStore.ts}`；`client/web/package.json`（+
  @tanstack/react-virtual，对齐 KY 版本）
- 移植自：KY `kylins.client.frontend` 的 MessageList/虚拟化模式；MC 行呈现
  语义。

- [ ] 步骤 1：测试先行：MessageList 从夹具渲染行（虚拟化窗口——断言首批
  N 行渲染、滚动或按钮经 KY 模式触发 ShowMore）；视图切换派发
  set_view_mode；已读/旗标按钮以确切 RowRef 载荷派发 mark_read/set_flagged；
  未读行加粗、旗标行有字形。
- [ ] 步骤 2：实现；文件夹选择驱动列表（folderStore 选中文件夹）；发件
  地址生成单字母头像。
- [ ] 步骤 3：vitest + build + gate 绿；提交
  `feat: render the message list pane with actions`。

### 任务 4：harness 种子与活体列表测试

**文件：**
- 修改：`docker/harness/**`（经 harness SMTP 向 astra1 INBOX 种三封邮件）、
  `scripts/dev/harness.sh`（幂等种子）、
  `crates/astra-app/src/tests_live_harness.rs`（+列表测试）
- 修改：`docs/capabilities.md` + zh 镜像；
  `docs/changelog/unreleased/message-list.md`

- [ ] 步骤 1：扩展种子脚本：经 127.0.0.1:2025 向 astra1@example.test 投递
  三封可区分的邮件（自签 TLS——投递客户端按门控 IMAP 测试同路接受 harness
  证书）；暖卷积下幂等。
- [ ] 步骤 2：门控列表测试：harness up -> 连接+同步 -> 断言 INBOX 三行
  （主题钉死）-> MarkRead 一封 -> RefreshMail -> 断言未读数下降且行转已读。
- [ ] 步骤 3：活体运行：harness up、门控测试绿、slice-check 仍 PASS；记录
  输出。
- [ ] 步骤 4：capabilities 行（message list -> partial；threading 因分组视图
  -> partial）+ zh 镜像 + fragment（Platforms: desktop、Bump: minor）。
- [ ] 步骤 5：全量验证；提交
  `feat: prove the message list live and record the capability`。

### 任务 5：收尾

- [ ] 步骤 1：全量验证绿；推 `sp2b-message-list` 分支；开 PR 链接 spec；
  验收标准证据。
- [ ] 步骤 2：按 SDD 技能清台账。

## 自审记录

- Spec 覆盖：D1（任务 1）、D2/D3（任务 2）、D4（任务 2 写路径）、D5（任务
  3）、活体验证（任务 4）、文档（任务 4.4）。
- 接口链：MessageRowView/RowKey（T1）-> RowRef/intent/读取（T2）-> ipc 线
  （T2）-> 列表面板（T3）-> 活体验证（T4）。
- 携带风险：锁定 rev 上 engine edit_mail/mail_on_threads 的签名必须先读再
  移植 MC 调用（任务 2 步骤 3 已指名）。
