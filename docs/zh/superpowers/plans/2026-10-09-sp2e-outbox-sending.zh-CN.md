# SP2e 发件箱与发送实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-09-sp2e-outbox-sending.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 端到端交付发送：经引擎 durable outbox 的 SubmitRichMail（带 SMTP
provider 绑定）、发件箱族与 Sending surface、兑现 SP2d 前置的离开即冲写关闭、
撰写器 Send 按钮与发件箱面板、跨 harness 的活体投递验证。

**架构：** astra-app 增提交臂、基于引擎 op 的发件箱族（取消/重试/确认语义）、
Sending 快照与门控 then_close forget；astra-account 在连接时绑定 SMTP 提交
provider；SPA 从撰写器发送并管理队列。

**技术栈：** 既有 workspace；锁定 rev 的引擎 submit_mail/outbox op；MC 的
sending 契约文档作语义源。

**Spec：** `docs/superpowers/specs/2026-10-09-sp2e-outbox-sending-design.md`

## 全局约束

- 沿前序家规：SPDX、500 行上限、deny-everything lints、提交前 gate（直接
  退出码，绝不接管道）、conventional commits、无破折号、英式英语、zh 镜像
  同提交、只允许增量线上变更。
- 移植根：MC = D:/Projects/mailclient/opensource/mail-calendar（mailcal-app
  outbox_ops/send.rs、protocol/intent_outbox.rs、bindings send 路径、
  docs/sending.md）；引擎经
  D:/Projects/mailclient/opensource/email-calendar-sync-engine 的
  `git show 68eab00:<path>`。
- AR1-AR7 适用。

---

### 任务 1：提交臂、发件箱族、Sending surface、门控 then_close

**文件：**
- 修改：`crates/astra-app/src/{protocol/intent.rs（加 intent_outbox.rs 同级）,
  protocol.rs, dispatch.rs, drafts.rs（门控 forget）}` + 测试；新建：
  `crates/astra-app/src/{outbox.rs, outbox_tests.rs}`；修改
  `crates/astra-viewmodel/src/`（+sending 快照与测试）
- 移植自：MC `mailcal-app/src/{outbox_ops.rs, mail_ops/send.rs, protocol/
  intent_outbox.rs}`；68eab00 的引擎 submit/outbox op 面。

**接口：**
- 产出：`Intent::SubmitRichMail { from: AccountId, to, cc, bcc, subject,
  document: ComposerDocument, blobs: Vec<ComposerBlob> }`；
  `OutboxIntent::{Show, Cancel{queued: QueuedRef}, SendNow{queued:
  QueuedRef}, ConfirmSent, ConfirmNotSent}` + `Intent::Outbox(
  OutboxIntent)`；reference.rs 的 `QueuedRef { account, op: PendingOpId }`；
  `Surface::Sending`（事件 `sending`、不去抖）+ viewmodel 的
  `SendingSnapshot { rows: Vec<QueuedRow { queued: QueuedKey, state,
  subject, arrived_at }> }`；提交经 astra-composer 渲染并调引擎
  submit_mail 后 drain；发件箱臂映射引擎 op（先读 68eab00 的
  cancel_pending_op / retry_pending_op_now / 确认 op 签名）；drafts.rs：
  then_close 的 forget 移入保存门下（close_composition 形态），SP2d 竞态
  测试扩展到 then_close 路径。

- [ ] 步骤 1：protocol 线上测试先行（submit + 五个发件箱变体 + QueuedRef）。
- [ ] 步骤 2：viewmodel sending 快照线上测试。
- [ ] 步骤 3：outbox_tests 先行（内存引擎 + 带 SMTP 腿的假件——扩展假件）：
  提交排队；drain 投递；SendNow 立即重试；Cancel 撤回；未确认呈现提示行；
  ConfirmSent/ConfirmNotSent 解除；Sending 每转换发布；then_close 竞态
  扩展。
- [ ] 步骤 4：实现；全绿；提交
  `feat: add submit, the outbox family, and the sending surface`。

### 任务 2：连接时的 SMTP provider 绑定

**文件：**
- 修改：`crates/astra-account/src/{imap.rs 或新 smtp.rs, config.rs（[smtp] 腿
  进引擎提交形态）}` + 测试
- 移植自：MC `mailcal-account` 的 provider 注册（MC 如何在 IMAP 旁绑定 SMTP
  提交 provider）；68eab00 的 provider-imap 提交类型。

**接口：**
- 产出：`connect_mail_providers`（或其调用者）按 `[smtp]`（host/port/security
  加凭据）注册账户的提交 provider 使引擎 drain 可投递；门控 harness 账户的
  SMTP 绑定对 127.0.0.1:2025 工作、证书例外同路流动。

- [ ] 步骤 1：测试先行（假件）：带 [smtp] 的账户注册集含提交绑定；缺
  [smtp] 保持仅邮件（记录形态）。
- [ ] 步骤 2：实现 + 单元绿；提交
  `feat: bind the smtp submission provider at connect`。

### 任务 3：宿主命令与 IPC 线

**文件：**
- 修改：`client/desktop/src/commands/**`（+同级测试）、
  `client/web/src/services/{ipc.ts, ipc.test.ts}`、
  `client/web/src/events/surfaceEvents.ts`（+测试）

**接口：**
- 产出：`sending() -> SendingSnapshot` 拉取；ipc.ts：SubmitRichMail +
  OutboxIntent 构造器 + SendingSnapshot 类型，字面量镜像；surfaceEvents 增
  `sending`。

- [ ] 步骤 1：测试先行（命令拉取 + 双侧线 + 事件）。
- [ ] 步骤 2：实现；提交 `feat: expose sending over ipc`。

### 任务 4：撰写器 Send 按钮与发件箱面板

**文件：**
- 修改：`client/web/src/features/composer/**`（Send 按钮：以当前文档派发
  SubmitRichMail、成功即关、类型化错误保持打开；Close 切到
  Save{then_close: true}）；新建：
  `client/web/src/features/outbox/{OutboxPanel.tsx + tests}`；toast 接线
- [ ] 步骤 1：按 D5 测试先行（发送成功/失败、关闭冲写载荷切换、发件箱行
  与动作精确载荷、未确认提示、surface 信号上的 toast）。
- [ ] 步骤 2：实现；提交 `feat: add the send button and the outbox panel`。

### 任务 5：活体投递验证、契约文档与能力行

**文件：**
- 修改：`crates/astra-app/src/tests_live_harness/**`（+发送测试）、
  `docs/sending.md`（+zh 镜像）、`docs/capabilities.md` + zh 镜像、
  `docs/changelog/unreleased/outbox-sending.md`

- [ ] 步骤 1：门控发送测试：连接 astra1（IMAP + SMTP 绑定、证书接受）->
  SubmitRichMail 给 astra2 -> drain -> outbox 空/已投递 -> 同步 astra2 ->
  其 INBOX 持有该邮件（主题钉死）。
- [ ] 步骤 2：活体运行：harness up、全部门控绿（dial/list/detection/
  reading/drafts/send）、slice-check PASS；记录输出。
- [ ] 步骤 3：实质移植 MC 的 docs/sending.md（never-send-twice、durable
  队列、未确认送达语义、取消与立即发送）+ zh 镜像；capabilities 行
  （sending partial、outbox partial）+ zh 镜像 + fragment。
- [ ] 步骤 4：全量验证；提交
  `feat: prove live delivery and record the sending contract`。

### 任务 6：收尾

- [ ] 步骤 1：全量验证绿；推 `sp2e-outbox-sending` 分支；开 PR 链接
  spec（注明 SP2d 前置兑现）；验收证据；清台账。

## 自审记录

- Spec 覆盖：D1（T1 提交）、D2（T1 族+surface）、D3（T1 门控 forget + T4
  关闭切换）、D4（T2）、D5（T4）、活体验证 + 契约文档（T5）。
- 接口链：submit/outbox intent + 快照（T1）-> SMTP 绑定（T2）-> ipc（T3）
  -> UI（T4）-> 活体（T5）。
- 携带风险：引擎的 pending-op 确认语义（ConfirmSent/NotSent 映射引擎 op——
  先读 68eab00 再适配；若引擎无直接确认 op，移植 MC 的 bindings 层机制并
  记录）；假件的 SMTP 腿须诚实建模可重试拒绝以覆盖未确认路径。
