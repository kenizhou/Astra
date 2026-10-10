# 二元裁决器设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-10-binary-judge-design.md`）。

日期：2026-10-10
状态：proposed（待审阅）
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 8、9 节）
供：A1（规则、分类、冷邮件拦截、eval 工装）、A2（草稿浮现）、A3（知识门控）、A4（digest 筛选）

## 1. 目的

Astra 的 AI 决策里有很大一部分只有一比特：这是不是冷邮件、这条规则是否
命中、这份草稿值不值得浮现、这段内容能否安全送进模型、这个行为是否通过
eval。这些决策要的是专门原语：便宜、严格、对不确定性诚实的二元裁决调用，
而不是事后解析的散文补全。

## 2. 原语

- 后台客户端（Rust）上的 **`judge(use_case, question, context,
  correlation) -> JudgeVerdict`**，sidecar 需要处配镜像工具：一次由
  版本化裁决模板构建的 prompt（问题 + 打标上下文，不可信内容经加固包装
  包裹）、一次强制单 token 的补全、解析为 `JudgeVerdict::{Yes, No,
  Abstain}`。
- **Abstain 是一等状态。** 不确定的裁决就说不确定。每个消费方预先声明
  自己对 Abstain 的安全默认（下表）；Abstain 后在同档以更冷 prompt 重试
  一次，然后适用默认。Abstain 绝不静默折成 Yes 或 No。
- **解析严格**：`yes`/`no`（大小写不敏感、去标点）或 Abstain 标记；其余
  一律 Abstain。没有 JSON 信封、没有修复循环：单 token 形态本身就是
  意义所在。
- **台账**：每次裁决如常落 `llm_calls` 行（use case、tier、token、
  correlation id）；裁决结果记入行的 outcome 字段，裁决总体可审计。

## 3. 安全默认表

| Use case | Tier | Abstain 默认 | 理由 |
|---|---|---|---|
| `JudgeColdEmail` | nano | No（不动） | 凭硬币移动邮件比留着更糟 |
| `JudgeRuleMatch` | economy | No（规则不执行） | 确定性动作需要正向命中 |
| `JudgeDlp` | nano | Block（fail closed） | 隐私从严关闭 |
| `JudgeInjection` | nano | 按敌意处理（更硬包裹） | 安全从严关闭 |
| `JudgeDraftSurface` | nano | No（不浮现） | 未浮现的草稿仍在文件夹里 |
| `JudgeSummaryFaithful`（eval） | economy | 判 eval 失败 | eval 从严 |
| `JudgeScenario`（eval） | economy | 判场景失败 | 同上 |

## 4. 边界：裁决器永不把门安全

权限桥、发送类拒绝与 CLI 门控保持确定性。裁决结果可以门控用户体验
（浮现、移动、包裹、筛选），绝不门控授权。裁决器对「这个工具调用安全吗」
说 Yes 改变不了任何事；策略表与人决定。此边界与安全裁决规则并列写入
AGENTS.md。

## 5. Use-case 清单（注册表增长）

SPA0 交付六个 use case；裁决器族随功能落地：A1 加 JudgeColdEmail、
JudgeRuleMatch、JudgeInjection 加 eval 工装裁决器（JudgeScenario、
JudgeSummaryFaithful）；A2 加 JudgeDraftSurface；A3、A4 随设计加各自的
筛选裁决器。全部映射 nano 或 economy 档；不进 chat 与 draft 档。

## 6. Eval 工装落点（A1）

inbox-zero 模式建议的 eval 工装随 A1 落地，裁决器占其半壁：
JudgeScenario 裁决场景转录（能用确定性夹具的场景必须用夹具，语言性的
行为才用裁决器），裁决 prompt 随管线常量版本化，裁决调用如产线般入
台账。确定性夹具优先。

## 7. 成本形态

单 token 输出让裁决器成为最便宜的调用类别：nano 档为它而设，周度花费
护栏（A1）首先盯 nano。裁决模板短且版本化；上下文按模板封顶（头 N 截断
加标记，绝不整库带入）。

## 8. 测试

- 单元：严格解析器（yes/no/abstain/标点/大小写/垃圾）；重试一次后走默认
  的状态机；安全默认表本身作为钉死测试（新 use case 不带默认不能交付）。
- 夹具服务器：裁决线形态（单 token prompt 形态）与 abstain 路径。
- Eval：裁决 prompt 变更即 bump 管线常量并重跑场景套件（规则书 AI 规则）。
