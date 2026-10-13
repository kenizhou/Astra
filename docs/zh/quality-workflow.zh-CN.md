# 质量工作流(评审副本,非规范性)

本文件是把 gstack 技能套件接入 Astra 切片节奏的操作手册。AGENTS.md 仍是规范性规则书;本文档说明何时、以何种顺序、针对哪个界面运行哪个技能。本文档与其描述的任何工作流步骤在同一变更中更新。

## 前置条件:基于夹具的设计台架

webview 只能在 Tauri 宿主内运行,而可视化技能(`/browse`、`/design-review`、`/design-shotgun`、`/qa`、`/benchmark`)驱动的是真实的 Chromium。夹具台架弥合了这一差距,并已落地:

- `npm run dev:fixtures`(vite 的 `--mode fixtures`)把 Tauri 桥接别名到 `client/web/src/services/__fixtures__/harness.ts`;该桩把同目录下的 JSON 快照喂给 store,并让变更类命令安静了结。localhost 上的服务器由此提供带真实数据的真实 UI,无需 Tauri 宿主,也无需真实账户。其契约由旁边的 `harness.test.ts` 钉死。

## 项目启动时一次性执行

1. 用 `/design-consultation` 敲定设计系统(字体排印、色彩、间距、动效),作为 `client/web/src/styles/` 下的令牌。之后的所有界面都从这些令牌取值,而不是各自发明一套。

## 每个切片(自 SP2m 起)

先设计,再实现,再验证,全部落在规则书契约流程所要求的同一个变更内。

1. 用 `/spec` 在 superpowers 规格旁边细化切片范围。
2. 针对台架为新界面做稿:单一界面用 `/design-html`;方向有争议、需要变体对比板时用 `/design-shotgun`。
3. 对切片计划运行 `/plan-design-review`。影响面宽的切片(OAuth、插件、MCP 表面)改用 `/autoplan`,一次性获得 CEO、工程、设计与开发者体验四类评审。
4. 按规则书要求测试先行地实现。
5. 通过台架对建成的界面运行 `/design-review`;在切片 PR 开启前修掉发现的问题。
6. 若切片改变 AI 行为(提示词、流水线、工具表面),评估运行与流水线版本号提升随同一 PR 落地,与规则书一致。夹具评估保持零成本;任何付费的线上模型运行先用 `/eval-plan` 预注册(基准线、实验组、诱饵、停止规则、计价试点),然后才动用资金。
7. 契约文档、`docs/capabilities.md` 行与变更日志碎片在同一变更中落地,与规则书一致。

## 每个 PR

1. 底线不变:`cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo run -p xtask -- gate`。
2. 对差异运行 `/review`。门(gate)强制契约;评审评判变更本身。
3. 触及信任边界(见下表)的差异运行 `/security-review`。触及边界而未做安全评审的差异不得合并。
4. 以 squash 合并,标题为 "PR title (#number)",合并后删除分支。

可选护栏:`/freeze` 限定到切片目录,可让长切片的编辑始终落在边界内。

## 每个里程碑(每个切片结束,以及锁定阶段)

1. 针对下表每个信任边界运行 `/cso`,每个边界一轮。发现的问题在同一里程碑内修复,或作为已知缺口记录在该边界的契约文档中,绝不静默搁置。
2. 运行 `/test-audit`,清理低价值与重复测试,以及它们保活的仅测试代码。
3. 运行 `/deslop-shared-libs`,在 CLI 动词、桌面命令、MCP 工具与插件钩子这些共同包装 astra-app 表面的代码中寻找值得做的共享抽取。
4. 用 `/health` 查看门禁、clippy 与 vitest 底线之上的质量趋势。
5. 通过台架对 webview 包运行 `/benchmark`,让让 UI 膨胀的切片在里程碑处被拦下,而不是落入用户手中。

### 信任边界

| 边界 | 所在位置 |
|---|---|
| 不可信消息内容围栏 | crates/astra-mcp(`fence`,以及 astra-ai 的 `tag_untrusted`)|
| 阅读窗格 HTML 消毒器 | 阅读视图背后的 ammonia 配置 |
| DLP 脱敏门 | crates/astra-ai(`DlpPolicy`、`redact`、`apply`)|
| sidecar 权限桥 | sidecar/src/permissions(失败即关闭,默认需批准)|
| 发送拒绝 | 任何发送类动词都不到达代理;由 astra-mcp 与 CLI 中的测试钉死 |
| 插件权限表 | crates/astra-plugin(清单、钩子 API、批准默认值)|
| OAuth PKCE 与令牌处理 | crates/astra-oauth 与宿主登录会话 |
| 凭据存储 | 桌面宿主中的操作系统 keyring 接缝 |
| CLI 门控 | cli 动词、变更操作上的 `--preview`、审计信封 |

确定性不变量(发送拒绝、默认需批准)仍由 Rust 测试钉死;`/cso` 负责捕捉测试未建模的问题。

## 每周与每月

- 每周:对本周的切片与陷阱运行 `/retro`。
- 每月:运行 `/learn` 评审并清理积累的习得;若里程碑跳过了 `/test-audit`,在此补跑。

## 与工具默认行为的已记录冲突

- `/ship` 会提升 VERSION 并编辑变更日志;规则书只允许在发布 PR 中移动 VERSION,且变更日志条目采用碎片形式。使用 `/ship` 时须抑制其版本步骤,或手动落地。
- `/document-generate` 在事后补写文档;规则书要求契约文档与功能在同一变更中移动。仅在切片 PR 内、用于非契约性散文时使用它。
- `/benchmark-models` 为 astra-ai 的层级表比较模型;它只服务于 `TierTable` 配置,绝不发送用户数据。
