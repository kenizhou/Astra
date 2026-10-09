# SP2f 回复与转发设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-09-sp2f-reply-forward-design.md`）。

日期：2026-10-09
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 5、15 节）

## 1. 目的

回信：从阅读窗做回复、回复全部与转发，带引用内容、主题与收件人预填，
发送路径复用 SP2e 的 durable outbox。这闭合核心邮件环（读、写、回、发），
后续一切切片都建在它上面。

## 2. 决策

- **D1**：`Intent::SubmitRichReply { from, to, cc, in_reply_to, subject,
  document, blobs }` 与 `Intent::SubmitRichForward { from, to, subject,
  document, blobs }` 按 MC 形态移植（in_reply_to 携带原信 Message-ID 头；
  转发没有）；两者经与 SubmitRichMail 相同的 durable-outbox 路径提交，
  线程头由引擎的 draft 构建器按 MC 契约设置。
- **D2**：回复引用在 Rust 内渲染：阅读快照的纯文本（或 html 转文本回退）
  成为 ComposerDocument 的 Quote 块并带原信归属行（发件人与日期，MC 格
  式）；引用风格待设置切片落地后随账户偏好（本片用常量默认值，记录）。
  转发把原信正文打包为带归属的 Quote；原信有附件时，若锁定引擎的带附件
  转发需要 staging 流程，则附件随该切片推迟并在 capabilities 记录。
- **D3**：`ComposeRequest` 形态的发起：阅读窗的 Reply / Reply all /
  Forward 动作派发 Intent 族 `Compose::{Reply{message}, ReplyAll{message},
  Forward{message}}`（与 Folders/Drafts/Outbox 并列的新族），其臂在服务端
  计算预填组合（收件人取自引擎消息读的原信头、引用按 D2）并发布到
  `Surface::ComposeRequest` 快照（不去抖）供撰写器水合——MC 的
  ComposeRequest surface 形态，单槽。
- **D4**：撰写器增加从 compose-request 水合（字段 + 引用文档 +
  in_reply_to 随草稿保存使已保存的回复可正确恢复）；主题预填按 MC
  （Re:/Fwd: 前缀、保留原主题）；Reply all 的 cc 取原信 cc 减去自己。
- **D5**：本片仅纯文本引用（html 原信有纯文本替代则用之，否则用净化文本
  回退）；富引用渲染（嵌套 html 引用块）推迟到 rich-paste 切片，记录。

## 3. 测试

- 单元（astra-app，内存引擎 + 带完整头的假件）：Reply 预填 to = 原发件人
  （若唾手可得则按 MC 做 list-unquote）；Reply all 的 cc = 原 cc 减自己、
  to 减自己；Forward 预填空 to；主题按 MC 规则加 Re:/Fwd: 前缀；回复设
  in_reply_to；Quote 块带归属与正文；提交路径线程正确（假件记录 draft 的
  头）。
- 线上：两个提交 Intent + Compose 族 + ComposeRequest 快照字面量双侧
  钉死。
- 前端（vitest）：阅读窗按钮以精确载荷派发 Compose Intent；撰写器从请求
  surface 水合字段与引用文档；主题前缀显示；Send 派发带 in_reply_to 的
  SubmitRichReply/Forward。
- 活体（门控，harness）：astra2 回复 astra1 的种子邮件送达 astra1（线程
  化：回复的 In-Reply-To 匹配原信 Message-ID，经引擎线程读或头断言）；
  转发以 Fwd: 主题送达另一账户。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建与 vitest 全绿。
2. 活体：astra2 对 astra1 种子邮件的回复送达且带线程头；转发带前缀主题
   送达；两者都走 durable outbox（队列排空）。
3. Reply all 按 MC 规则计算收件人（排除自己）；引用归属存在。
4. `docs/capabilities.md` 行移动（reply/forward 到 partial；threading 因
   回复关联到 partial），zh 镜像、changelog fragment，同变更在
   `docs/sending.md`（或 MC 拆分时的同级 compose 文档）加入回复/转发
   章节。
5. 只允许增量线上变更；既有夹具与测试原样通过。
