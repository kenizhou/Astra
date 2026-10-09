# SP2e 发件箱与发送设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-09-sp2e-outbox-sending-design.md`）。

日期：2026-10-09
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 5、15 节）
携带硬前置：SP2d 终审的 then_close 门控修复在本片落地，并与 Close 切换到
Save{then_close: true} 成对。

## 1. 目的

发邮件且绝不静默丢失发送：新撰写发送路径经引擎 durable outbox、发件箱族
（显示、取消、立即发送、未确认送达提示）、Sending surface、连接时的 SMTP
provider 绑定、离开即冲写的关闭。回复与转发（引用、发件人预填）随下一片；
本片发送撰写器所持内容。

## 2. 决策

- **D1**：`Intent::SubmitRichMail { from, to, cc, bcc, subject, document,
  blobs }` 按 MC 形态移植（blobs 在线上存在，本片编辑器不产任何 blob；
  附件后续到来）；经 astra-composer 渲染并经引擎 `submit_mail` 提交，进入
  durable outbox。撰写器的 Send 按钮以当前编辑器内容派发（无冲写窗口
  缺口）。
- **D2**：发件箱族按 `OutboxIntent::{Show, Cancel{queued}, SendNow{queued},
  ConfirmSent, ConfirmNotSent}` 移植（MC 形态减去 Edit——它需要附件切片的
  staging-directory 流程；记录推迟）；`Surface::Sending` 携带队列快照
  （逐账户行：op id、状态 Queued | Sending | Unconfirmed | Failed、主题行、
  到达时刻）。
- **D3**：关闭冲写：撰写器的 Close 切到 `Save{then_close: true}` 且
  then_close 的 forget 移入 `close_composition` 形态的保存门下（SP2d 前置；
  两半一起落地，由扩展到 then_close 路径的 fire-then-close 竞态测试钉死）。
- **D4**：SMTP 腿在连接时绑定：`connect_mail_providers` 在 IMAP 绑定旁按
  账户 `[smtp]` 配置注册提交 provider（MC 的 provider 集），引擎 outbox
  drain 因此可投递；drain 在每次 RefreshMail 后（已接线）与每次提交后
  运行。
- **D5**：前端新增：Send 按钮（派发 SubmitRichMail 后关面板；类型化错误
  则呈现错误、面板保持打开、文字不丢）、发件箱面板（队列行带取消与立即
  发送动作、未确认行带 Mark as sent 与 Send again 提示）、surface 信号上的
  发送结果 toast。

## 3. 测试

- 单元（astra-app，内存引擎 + 带 SMTP 腿的假件）：提交落入排队 op；drain
  经假 SMTP 投递；SendNow 立即重试；Cancel 撤回；未确认路径（送达状态
  未知）呈现提示行；ConfirmSent 与 ConfirmNotSent 解除；Sending surface
  每次转换都发布；SP2d 的 then_close 竞态钉扩展到门控 forget。
- 线上：SubmitRichMail + 发件箱族 + Sending 快照字面量双侧钉死。
- 前端（vitest）：Send 以当前文档派发、成功即关、失败保持打开并呈错；
  发件箱行动作以精确载荷派发；未确认提示；toast。
- 活体（门控，harness）：经 harness SMTP 从 astra1 向 astra2 提交邮件；
  断言 outbox 排空为已投递且同步后 astra2 的 INBOX 持有该邮件；未确认
  路径仅由单测的假拒绝变体覆盖。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建与 vitest 全绿。
2. 活体验证中撰写器发出的邮件到达 astra2 的 INBOX；取消与立即发送行为
   正确；未确认提示可解除。
3. SP2d 前置兑现：Close 冲写最终窗口，扩展的竞态测试钉死 then_close 路径
   的门控 forget。
4. `docs/capabilities.md` 行移动（sending partial、outbox partial），zh
   镜像、changelog fragment，同变更落地实质移植自 MC 的 `docs/sending.md`
   契约文档（never-send-twice、未确认语义）。
5. 只允许增量线上变更；既有夹具与测试原样通过。
