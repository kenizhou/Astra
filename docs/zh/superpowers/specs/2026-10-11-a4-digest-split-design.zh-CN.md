# A4 Digest、分析、分箱收件箱设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-11-a4-digest-split-design.md`）。

日期：2026-10-11
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 9.3 节 A4）
消费：A1-A3 骨架（客户端、裁决器、缝、台账、eval 工装、来信通道的分类结果）、阅读窗、消息列表。

## 1. 目的

inbox-zero AI 目录中仅依赖邮件的最后几项：按需的近期邮件 digest、近期
窗口的分析报告、由已存分类派生的 FYI/Actioned 分箱收件箱。会议简报依赖
日历数据（日历功能切片尚未移植）——推迟到日历切片，记为已知推迟、绝不
静默。

## 2. 决策

- **D1（digest）**：迁移 `0010_ai_documents`（id、account、kind: digest |
  report、period_start、period_end、content、created_at、correlation_id）。
  `digest_generate(account, days?)`（默认 7、上限 31）收集窗口内邮件（既有
  最新优先读），从**已存** ai_results 分类构建类别汇总加逐类邮件清单，
  做**一次**总结生成（`DIGEST_PROMPT_VERSION`、DEFAULT 档；邮件 untrusted
  包裹、每类取头五封封顶）。digest 连同期入库；同期重生成替换同键行。
  阅读面不变；一个 Digest 面板（对话框或窗内节，实现者择一记录）渲染最
  新 digest 带 Generate 按钮；CLI 读。
- **D2（分析报告）**：`report_generate(account, days?)` 产三节报告（执行
  摘要、响应模式、建议），窗口相同，先算统计（逐类计数、未读率、已发
  数）再喂**一次**结构化生成（`REPORT_PROMPT_VERSION`、DEFAULT 档）。
  与 digest 同库（kind report）。同面板加 tab；CLI 读。
- **D3（分箱收件箱）**：**零新增模型调用**。分箱从已存分类 stage 结果
  派生：Actioned = primary；FYI = newsletter、notification、receipt、
  cold；无已存类别者只出现在 All。消息列表加三向开关（All / Actioned /
  FYI），按宿主按账户服务的分箱映射本地过滤（`split_map(account) ->
  {key -> Actioned|FYI}` 随快照拉取；增量）。映射是钉死数据，以
  `SPLIT_MAPPING_VERSION` 版本化（映射变更即 eval 重跑）。
- **D4（会议简报）**：推迟到日历切片；capabilities 行保持未开始并点名
  依赖。

## 3. 测试

- 单元：digest 收集（窗口上限、从已存行构建类别汇总、逐类头五、
  untrusted 包裹）；报告统计；同期替换；分箱映射（每个类别词映射或排
  除，钉死）；两个新常量 + 映射版本的 eval 夹具。
- 夹具服务器：DEFAULT 档形态。
- 前端：面板（两 kind 的生成/加载/渲染、期间标签）；分箱开关（过滤、
  计数徽标、无类别仅 All 规则）。
- 活体（门控）：对种子邮件的真实 digest；真实报告；对通道真实分类结果
  的分箱映射；成本合理。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建、vitest、
   sidecar 测试全绿。
2. 离线：digest 与报告生命周期钉死（生成 -> 入库 -> 渲染；重生成替换）；
   分箱映射端到端钉死。
3. 活体：digest + 报告对端点运行、成本合理；分箱映射对 harness 真实分类
   结果服务。
4. `docs/ai-digest.md`（+zh）载契约（窗口上限、汇总来源、替换语义、分箱
   映射、会议简报推迟），capabilities 行随镜像移动，fragment 落地，eval
   夹具带版本 pin。
5. 只允许增量线上变更；既有夹具与测试原样通过。
