# SP2f 回复与转发实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-09-sp2f-reply-forward.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 端到端交付回复、回复全部与转发：服务端计算预填组合（带引用）的
Compose 族、水合撰写器的 ComposeRequest surface、经 durable outbox 线程化
的两个提交 Intent、回复/转发的活体投递验证。

**架构：** astra-app 增 Compose 族臂（经引擎读头、经 astra-composer 分块建
引用、ComposeRequest 快照），两个提交 Intent 与 SubmitRichMail 并列共享
outbox 路径；SPA 阅读窗派发族、撰写器从请求 surface 水合。

**技术栈：** 既有 workspace；锁定 rev 的引擎消息读；MC 的回复/转发契约
（mailcal-app reply 路径、ComposeRequest surface）作形态源。

**Spec：** `docs/superpowers/specs/2026-10-09-sp2f-reply-forward-design.md`

## 全局约束

- 沿前序家规：SPDX、500 行上限、deny-everything lints、提交前 gate（直接
  退出码、绝不接管道）、conventional commits、无破折号、英式英语、zh 镜像
  同提交、只允许增量线上变更。
- 移植根：MC = D:/Projects/mailclient/opensource/mail-calendar（mailcal-app
  reply/forward + ComposeRequest、mailcal-composer 引用块）；引擎经
  D:/Projects/mailclient/opensource/email-calendar-sync-engine 的
  `git show 68eab00:<path>`。
- AR1-AR7 适用。

---

### 任务 1：Compose 族、引用构建、ComposeRequest surface

**文件：**
- 修改：`crates/astra-app/src/{protocol/intent.rs, protocol.rs, dispatch.rs}`
  + 测试；新建：`crates/astra-app/src/{compose_request.rs,
  compose_request_tests.rs}`；修改 `crates/astra-viewmodel/src/`（+
  ComposeRequest 快照与测试）
- 移植自：MC `mailcal-app` reply 路径 + ComposeRequest surface；68eab00 的
  引擎消息头读。

**接口：**
- 产出：`ComposeIntent::{Reply{message: MessageRef}, ReplyAll{message},
  Forward{message}}` + `Intent::Compose(ComposeIntent)`（如 Folders 的
  `#[non_exhaustive]` 族）；`Surface::ComposeRequest`（事件
  `compose_request`、不去抖）+ viewmodel 的 `ComposeRequestSnapshot`
  （kind: Reply | ReplyAll | Forward、to、cc、subject、in_reply_to:
  Option<String>、带引用块的 ComposerDocument、from 账户）；臂经引擎读
  原信头（先读 68eab00 的消息读签名）、按 MC 规则算收件人（reply：发件
  人；reply-all：发件人 + to + cc 减自己；forward：空）、按 MC 加主题前缀
  （Re:/Fwd: 带已有前缀感知）、构建带归属行的 Quote 块并发布。

- [ ] 步骤 1：protocol 线上测试先行（三个族变体 + compose_request surface
  词）。
- [ ] 步骤 2：viewmodel 快照线上测试（含 in_reply_to 与引用文档的完整
  reply 形态；无 in_reply_to 的 forward）。
- [ ] 步骤 3：compose_request_tests 先行（带完整头的假件）：按类型收件人
  规则（排除自己）、主题前缀（不双 Re:）、归属行格式、来自阅读内容的纯
  文本引用、html 回退到净化文本、转发引用、随派发发布。
- [ ] 步骤 4：实现；全绿；提交
  `feat: add the compose request family with quoted prefill`。

### 任务 2：回复与转发提交 Intent

**文件：**
- 修改：`crates/astra-app/src/{protocol/intent.rs, outbox.rs（提交缝）,
  dispatch.rs}` + 测试
- 移植自：MC 的 SubmitRichReply/SubmitRichForward 臂；68eab00 的引擎
  draft 构建器线程化（in_reply_to 处理）。

**接口：**
- 产出：`Intent::SubmitRichReply { from, to, cc, in_reply_to:
  MessageIdHeader（引擎再导出的 newtype）, subject, document, blobs }` 与
  `Intent::SubmitRichForward { from, to, subject, document, blobs }`；
  两者共享 submit_draft 的 outbox 路径，回复的 draft 携带 in_reply_to
  （引擎 Draft 构建器——先读 68eab00；构建器收该头则传入；记录形态）。

- [ ] 步骤 1：protocol 线上测试先行（两个 Intent 含 in_reply_to 往返）。
- [ ] 步骤 2：提交测试先行（假件记录 draft）：回复 draft 携带
  in_reply_to；转发不携带；两者如 SubmitRichMail 般排队与排空；
  never-send-twice 不变。
- [ ] 步骤 3：实现；全绿；提交
  `feat: add reply and forward submit intents`。

### 任务 3：宿主命令与 IPC 线

**文件：**
- 修改：`client/desktop/src/commands/**`（+同级测试）、
  `client/web/src/services/**`（+测试）、
  `client/web/src/events/surfaceEvents.ts`（+测试）

**接口：**
- 产出：`compose_request() -> Option<ComposeRequestSnapshot>` 拉取（如
  reading 的 None-清除语义）；ipc.ts：Compose 族 + 两个提交 Intent + 快照
  类型，字面量镜像；surfaceEvents 增 `compose_request`。

- [ ] 步骤 1：测试先行（命令拉取/清除；双侧线；事件准入 + 拉取）。
- [ ] 步骤 2：实现；提交 `feat: expose compose requests over ipc`。

### 任务 4：阅读窗动作与撰写器水合

**文件：**
- 修改：`client/web/src/components/reading-pane/**`（Reply / Reply all /
  Forward 按钮以精确载荷派发）、`client/web/src/features/composer/**`
  （从请求 surface 水合：字段、主题前缀显示、引用文档、in_reply_to 随
  草稿保存；Send 按打开请求的类型派发 SubmitRichReply/Forward）、
  `client/web/src/stores/**`（+composeRequestStore）

- [ ] 步骤 1：测试先行：按钮按打开的消息以精确 Compose Intent 派发；撰写
  器水合（字段 + 引用以 Quote 块在编辑器可见——保留机制已使其往返；编辑
  器将显示为保留内容）；回复的 Send 派发带 in_reply_to 的正确提交
  Intent；未发送即关闭不留陈旧（请求 surface 在撰写器关闭或下一请求时
  清除）。
- [ ] 步骤 2：实现；提交
  `feat: wire reply and forward into the reading pane and composer`。

### 任务 5：活体验证与文档

**文件：**
- 修改：`crates/astra-app/src/tests_live_harness/**`（+回复/转发测试）、
  `docs/sending.md`（+zh 镜像，回复/转发章节）、`docs/capabilities.md`
  + zh 镜像、`docs/changelog/unreleased/reply-forward.md`

- [ ] 步骤 1：门控测试：astra2 回复 astra1 的种子邮件 -> 送达 astra1 且
  In-Reply-To 匹配原信 Message-ID（经引擎线程读或头读断言）；转发带
  Fwd: 主题送达；两队列为空；清理恢复契约（astra1 INBOX 静息时三封——
  送达的回复必须清理，否则列表测试的整集断言破裂；选诚实清理路线并钉
  死）。
- [ ] 步骤 2：活体运行：harness up、全部门控绿（现 7 套）、slice-check
  PASS；记录输出。
- [ ] 步骤 3：sending.md 回复/转发章节（收件人规则、引用、线程语义、若
  适用则含附件推迟）+ zh 镜像；capabilities 行（reply partial、forward
  partial、threading 因回复关联 partial）+ zh 镜像 + fragment。
- [ ] 步骤 4：全量验证；提交
  `feat: prove live reply and forward and record the contract`。

### 任务 6：收尾

- [ ] 步骤 1：全量验证绿；推 `sp2f-reply-forward` 分支；开 PR 链接
  spec；验收证据；清台账。

## 自审记录

- Spec 覆盖：D1（T2）、D2（T1 引用构建）、D3（T1 族 + surface）、D4（T4
  水合 + in_reply_to 随保存）、D5（T1 纯文本引用路径）、活体验证 + 文档
  （T5）。
- 接口链：Compose 族 + 快照（T1）-> 提交 Intent（T2）-> ipc（T3）-> UI
  （T4）-> 活体（T5）。
- 携带风险：68eab00 上的引擎头读与 draft 构建器的 in_reply_to 形态
  （T1/T2 指名：先读）；转发带附件可能需要 staging 流程（若如此则显式推
  迟，记入 capabilities + sending.md）；回复送达后列表测试的三封契约必须
  存活（T5 钉清理）。
