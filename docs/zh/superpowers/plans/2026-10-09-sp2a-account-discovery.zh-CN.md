# SP2a 账户发现实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-09-sp2a-account-discovery.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 把 mail-calendar 的自动探测移植进 `astra-autodetect`，从宿主暴露
`detect_account_settings`，向导先探测、手动回退，并对 harness 的 JMAP 端点
活体验证探测。

**架构：** 叶子 crate `astra-autodetect` 持有策略（HTTP 探测加 DNS 查询）；
`astra-account` 增加推荐层（`recommend`、`SetupChoices`）；宿主加一条设置
路径命令；SPA 向导在显示手动表单前先调它。不改 Intent 与 dispatch。

**技术栈：** Rust（既有 workspace lints）、workspace 表中的 reqwest（同 MC
autodetect 用法）、DNS 用 MC autodetect 实际 pin 的 crate（先读 MC 再镜像）、
React 向导扩展。

**Spec：** `docs/superpowers/specs/2026-10-09-sp2a-account-discovery-design.md`

## 全局约束

- 同 SP1 计划：SPDX 头、500 行上限（MC 式拆分）、AR1-AR4 适配规则
  （astra 重命名、无历史注释、无 uniffi、引擎类型经再导出、time crate）、
  deny-everything lint 集、每次提交前 gate 绿、conventional commits、无破折号、
  英式英语、zh 镜像与英文源同变更。
- 邮箱地址绝不出现在 URL 里（account-autodetect 契约）；不可信（非 HTTPS）
  来源须过审批门，与 MC 实现一致。
- 移植源根 MC = D:/Projects/mailclient/opensource/mail-calendar。

## 适配规则增补

- **AR7**：HTTP 探测用 workspace 的 reqwest（MC 的 feature 集）；DNS 用 MC
  autodetect pin 的 crate（镜像进 workspace 表）；超时沿用 MC 常量。

---

### 任务 1：astra-autodetect crate

**文件：**
- 新建：`crates/astra-autodetect/**`（镜像 MC `crates/mailcal-autodetect/src`
  模块布局：jmap 探测、autoconfig、ispdb、srv、mx，加不可信来源门与公开
  `detect` 入口）
- 移植自：`MC/crates/mailcal-autodetect/**` 含测试文件。

**接口：**
- 产出：`detect(email, options) -> DetectedSettings`（async；按 MC 的顺序与
  优先级）、带 serde 的 `DetectedSettings` 与逐策略结果类型（跨 IPC 处）、
  `UntrustedSource` 审批门类型。

- [ ] 步骤 1：先移植 MC 的策略级测试（解析器、优先级）；对空 crate 跑红。
- [ ] 步骤 2：按 AR1-AR4 + AR7 移植模块；任何删减须记录（MC 今天只有这些
  策略）。
- [ ] 步骤 3：`cargo test -p astra-autodetect` 绿；clippy/fmt/gate 绿。
- [ ] 步骤 4：提交 `feat: port the account autodetect strategies from mail-calendar`。

### 任务 2：astra-account 推荐层

**文件：**
- 新建：`crates/astra-account/src/{autodetect.rs, setup_choices.rs}` + 测试
- 移植自：`MC/crates/mailcal-account/src/{autodetect.rs, setup_choices.rs}`。

**接口：**
- 产出：`recommend(email, detected, routes) -> SetupRecommendation`（变体
  Imap | Dav | Microsoft | Google | Jmap | Manual）、`SetupChoices`（向导渲染的
  开关集），两者 serde 供 IPC。

- [ ] 步骤 1：移植 MC 测试（按探测结果的推荐矩阵、选择集渲染）。
- [ ] 步骤 2：移植模块；astra-account 依赖 astra-autodetect。
- [ ] 步骤 3：测试/clippy/fmt/gate 绿；提交
  `feat: port the setup recommendation and choices layer`。

### 任务 3：宿主命令与线类型

**文件：**
- 修改：`client/desktop/src/commands.rs`（+测试）、
  `client/web/src/services/{ipc.ts, surfaces.ts}`（+测试）
- 移植自：`MC/crates/mailcal-bindings/src/autodetect.rs`
  （detect_account_settings 形态）。

**接口：**
- 产出：`detect_account_settings(email) -> RecommendationView` 命令（五秒
  超时、类型化错误）；SPA 的 IPC 类型镜像 Rust serde 线上形状（双侧测试钉
  死字面量）。

- [ ] 步骤 1：commands_tests：未知域名返回与手动默认一致的手动 IMAP 推荐；
  超时路径返回类型化错误、绝不 panic。
- [ ] 步骤 2：实现并注册命令；ipc.ts 包装 + 线上测试。
- [ ] 步骤 3：全绿；提交
  `feat: add the detect_account_settings host command and ipc wire`。

### 任务 4：向导集成

**文件：**
- 修改：`client/web/src/features/account-setup/**`（+测试）

- [ ] 步骤 1：测试先行：输入邮箱触发探测；推荐渲染选择集；手动开关显示
  预填推荐/手动默认的表单；探测失败或超时回退手动并在 UI 中提示。
- [ ] 步骤 2：在 AccountSetupFlow 实现探测步骤（加载态、错误面、客户端也
  加五秒护栏）。
- [ ] 步骤 3：vitest + build + gate 绿；提交
  `feat: wire account discovery into the setup wizard`。

### 任务 5：活体 JMAP 探测与文档

**文件：**
- 修改：`scripts/dev/harness.sh`（env 增加 JMAP URL）、
  `crates/astra-app/src/tests_live_harness.rs` 或 astra-autodetect 的同级
  门控 JMAP 探测测试
- 修改：`docs/capabilities.md` + zh 镜像、
  `docs/changelog/unreleased/account-discovery.md`

- [ ] 步骤 1：读 MC harness 如何暴露 JMAP；相应扩展 compose/env（回环端口
  映射）。
- [ ] 步骤 2：门控测试：对 astra1@example.test，JMAP 策略返回 JMAP 推荐
  （无 ASTRA_LIVE_HARNESS=1 即跳过）。
- [ ] 步骤 3：`bash scripts/dev/harness.sh up` 后门控测试绿、slice-check 仍
  PASS；记录输出。
- [ ] 步骤 4：capabilities 行（auto-detect -> partial；wizard 行注明发现能力）
  + zh 镜像 + changelog fragment（Platforms: desktop、Bump: minor）。
- [ ] 步骤 5：全量验证；提交
  `feat: prove live jmap detection and record the capability`。

### 任务 6：收尾

- [ ] 步骤 1：全量验证绿；推 `sp2a-account-discovery` 分支；开 PR 链接
  spec；PR 正文注明 SP1 推迟项就此兑现。
- [ ] 步骤 2：按 SDD 技能清台账；按 spec 第 4 节逐条给验收证据。

## 自审记录

- Spec 覆盖：D1（任务 1）、D2（任务 2）、D3（任务 3）、D4（全计划无 Intent
  变更；任务 3 只注册一条设置路径命令）、D5（任务 5）。验收标准 1-5 映射到
  任务 1-6。
- 接口链：DetectedSettings（任务 1）-> recommend（任务 2）->
  RecommendationView + ipc 线（任务 3）-> 向导（任务 4）-> 活体验证（任务
  5）。
