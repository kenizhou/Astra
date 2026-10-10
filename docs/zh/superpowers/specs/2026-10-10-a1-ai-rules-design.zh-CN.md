# A1 AI 规则、分类、冷邮件拦截设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-10-a1-ai-rules-design.md`）。

日期：2026-10-10
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 9.3 节）、
`docs/superpowers/specs/2026-10-10-binary-judge-design.md`
消费：已验证的活体端点（tier 3 测试）与 SPA0 的注册表、台账、客户端、CLI、sidecar。

## 1. 目的

在 SPA0 骨架上落第一批 AI 功能：自然语言生成结构化条件的规则、来信按
规则裁决并确定性执行、发件人分类、冷邮件裁决器、带安全默认的二元裁决
原语、所有邮件内容 prompt 上的 DLP 主动执行、带裁决器的 eval 工装、
nano 花费护栏。

## 2. 决策

- **D1（裁决原语）**：完全按二元裁决器 spec：`LlmClient::judge(
  use_case, question, context, correlation) -> JudgeOutcome`（裁决值加
  产出路径：首答/重试/abstain 默认）；严格单 token 解析；Abstain 同档
  重试一次；安全默认表作为 astra-ai 的数据存在并被测试钉死（缺默认的
  裁决 use case 交付即失败）。
- **D2（规则）**：规则入产品库（迁移 `0002`：id、名称、条件（自然语言，
  版本化模板）、动作（封闭集：`mark_read`、`set_flagged`；更广动作集随
  文件夹与草稿回复切片到来，记录）、启用）。`prompt_to_rules(text)` 经
  `complete` + `repair_json` 生成结构化规则 JSON（DEFAULT 档、管线常量
  `RULES_PROMPT_VERSION`）；来信匹配逐规则跑 JudgeRuleMatch（economy
  档、上下文封顶）；执行经既有 astra-app 写路径确定性落地；每次运行入
  台账与审计并携带轮次 correlation id。
- **D3（来信通道）**：每次 refresh 重建后有界通道（新见未读的头 N 条，
  N 为小常量）跑 AI 阶段：规则匹配、分类、冷判定；结果入迁移 `0003`
  （消息键、阶段、裁决、类别）以 correlation id 关联。A1 不改快照与 UI
  （徽标随 A2 表面到来）；结果可经 CLI 查询。
- **D4（分类 + 冷邮件 + DLP）**：分类为结构化五选一调用（primary、
  newsletter、receipt、notification、cold；DEFAULT 档、版本化）；冷邮件
  为 JudgeColdEmail 二元裁决（nano）。一切携带邮件内容的 prompt 经加固
  包装；DLP 从仅 allow 毕业：`redact` 在包裹前掩码邮箱、电话形态数字与
  长数字串；`block` 拒绝该消息的 AI 通道（台账 outcome `blocked`）；
  策略来自 `ASTRA_DLP`（默认 allow）。
- **D5（eval 工装）**：`evals/` 存场景夹具（prompt、输入、期望裁决）；
  cargo-test 运行器离线跑确定性场景（无网络：夹具补全），仅
  `ASTRA_LIVE_LLM=1` 下跑裁决器场景；裁决模板与功能 prompt 带管线常量
  （任何 prompt 变更即 bump，本切片 PR 内演示一次 eval 重跑）。
- **D6（CLI 增长）**：`rules list|create|delete` 与 `ai cold
  <message-key>` 随 `--json`、审计、变更动词 `--preview` 落地，遵 CLI
  契约。
- **D7（nano 花费护栏）**：周度 nano 预算（`ASTRA_LLM_NANO_BUDGET_MICROS`，
  宽松默认）在每次裁决调用前经台账周聚合检查；超限则该通道跳过并告警，
  跳过适用二元裁决器表中该 use case 的安全默认（DLP 与 injection 从严
  关闭；仅冷邮件与草稿浮现这类纯 UX 阶段才 fail open；诚实记录）。

## 3. 测试

- 单元：裁决解析器与重试状态机；安全默认表钉死；redact 模式；规则执行
  确定性（假件记录写意图）；有界来信通道（遵守 N、已见消息不重跑）。
- 夹具服务器：裁决线形态（单 token prompt 形态）、abstain 路径、结构化
  生成形态含修复路径。
- Eval：确定性场景离线入 CI；裁决器场景走活体门。
- 活体（tier 3，门控，所供端点）：样句 prompt-to-rules 返回可解析规则；
  对 harness 种子邮件的真实裁决调用返回裁决值；成本入台账；整个 A1 套件
  对 Docker harness 全绿。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建、vitest、
   sidecar 测试全绿。
2. 离线：经 CLI 的规则 CRUD 往返；规则对假来信命中并确定性执行；redact
   与 block 路径钉死；eval 运行器在 CI 过确定性场景。
3. 活体：门控套件对所供端点与 Docker harness 通过（生成、匹配、冷裁决、
   台账成本）。
4. prompt 变更 bump 其管线常量并演示 eval 重跑（本切片 PR 内一次故意
   bump）。
5. `docs/ai-rules.md`（+zh）载契约（规则、类别、冷邮件、裁决安全默认表
   引用、花费护栏、eval 工装），capabilities 行随镜像移动，fragment 落地。
