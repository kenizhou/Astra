# SP2d 撰写与草稿实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-09-sp2d-compose-drafts.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 端到端交付撰写与草稿：astra-composer 文档模型、引擎服务器侧草稿
上的草稿族（空闲自动保存与恢复）、DraftStatus surface、线往返的裁剪版
TipTap 撰写器、活体存储验证。

**架构：** 叶子 crate astra-composer（serde 文档、确定性渲染、清单）；
astra-app 增草稿族、基于引擎 put/delete_draft 的草稿模块、空闲自动保存任务
与 DraftStatus surface；SPA 撰写器在 TipTap 状态与 ComposerDocument 线之间
互转并按状态 surface 打徽标。

**技术栈：** 既有 workspace；client/web 的 TipTap v3（kylins 版本）加裁剪
扩展集；锁定 rev 的引擎草稿写入。

**Spec：** `docs/superpowers/specs/2026-10-09-sp2d-compose-drafts-design.md`

## 全局约束

- 沿前序家规：SPDX、500 行上限、deny-everything lints、提交前 gate（绝不接
  管道）、conventional commits、无破折号、英式英语、zh 镜像同提交、只允许
  增量线上变更。
- 移植根：MC = D:/Projects/mailclient/opensource/mail-calendar
  （mailcal-composer、mailcal-app draft_ops、mailcal-account 草稿辅助）；
  KY = D:/Projects/mailclient/kylins（TipTap 接线）；引擎经
  `git show 68eab00:<path>`。
- AR1-AR7 适用。

---

### 任务 1：astra-composer crate

**文件：**
- 新建：`crates/astra-composer/**`，镜像裁剪后的 MC `mailcal-composer/src`
  （types.rs 含 ComposerDocument/Blob 清单、确定性 html 与文本的渲染路径）
  + 测试
- 移植自：MC `crates/mailcal-composer/**`。

**接口：**
- 产出：`ComposerDocument`（MC 自带 serde；MC 定义的分块模型）加本片草稿
  往返所需的最小渲染 API（MC 测试的完整确定性渲染向量；发送切片后续消费
  渲染 API——若渲染入口超出 SP2d 所需，因它就是该 crate 的存在目的，连同
  测试一并移植，任何裁剪都记录）。

- [ ] 步骤 1：先移植 MC 的 composer 测试（文档 JSON 往返、渲染确定性向量、
  清单形状）；空 crate 跑红。
- [ ] 步骤 2：按 AR1-AR4 移植模块（astra 重命名、无 uniffi、MC 用过的引擎
  类型经再导出、time crate）。
- [ ] 步骤 3：测试/clippy/fmt/gate 绿；提交
  `feat: port the composer document crate from mail-calendar`。

### 任务 2：astra-app 的草稿族、自动保存与状态 surface

**文件：**
- 修改：`crates/astra-app/src/{protocol/intent.rs, protocol.rs, lib.rs}` +
  protocol 测试；新建：`crates/astra-app/src/{drafts.rs, drafts_tests.rs}`；
  修改 `Cargo.toml`（astra-composer 依赖）与（若枚举）workspace 成员表
- 移植自：MC `mailcal-app/src/{draft_ops.rs（族派发）, protocol/
  intent_drafts.rs}` + MC `mailcal-account` 草稿辅助；锁定 rev 的引擎
  put/delete_draft。

**接口：**
- 产出：`DraftsIntent::{Save{composition, from, to, cc, bcc, subject,
  document, then_close}, Discard{composition}, Close{composition}}`（MC
  形态；blobs 推迟到附件切片，记录）；`CompositionId` newtype（按 MC 拒绝
  空白）；dispatch 族臂；草稿模块：保存经引擎 `put_draft`（先读 68eab00 的
  签名）、丢弃经 `delete_draft`、恢复读取（账户最新草稿）、空闲自动保存
  （三十秒、tokio time、关闭/丢弃时取消）；`Surface::DraftStatus`（事件
  `draft_status`、不去抖）带 astra-viewmodel 的最小状态快照类型
  （composition、state、saved_at）及其 Surfaced 单元。

- [ ] 步骤 1：protocol 线上测试先行（三个变体、CompositionId 拒绝空白）。
- [ ] 步骤 2：drafts_tests 先行（内存引擎 + 假件）：保存持久并读回；空闲后
  而非提前自动保存（tokio time 暂停或短常量缝）；丢弃移除；关闭保存后清
  状态；恢复还原；状态随转换发布。
- [ ] 步骤 3：实现（状态快照类型与线上测试同时在 viewmodel 侧落）。
- [ ] 步骤 4：全绿；提交
  `feat: add the drafts family with idle autosave and status surface`。

### 任务 3：宿主命令与 IPC 线

**文件：**
- 修改：`client/desktop/src/{commands.rs, app_state.rs（若拉取清单增长）}`
  + 同级测试文件；`client/web/src/services/{ipc.ts, ipc.test.ts}`；
  `client/web/src/events/surfaceEvents.ts`（+测试）

**接口：**
- 产出：`draft_status()` 拉取命令；surface 联合加 `draft_status` 及其拉取；
  ipc.ts 加 ComposerDocument + DraftsIntent + 状态类型（字面量镜像 Rust
  测试）。

- [ ] 步骤 1：commands 测试先行（拉取反映转换；空闲清理后 None）。
- [ ] 步骤 2：实现并注册；双侧线上测试；扩展 surfaceEvents（沿 reading
  先例）。
- [ ] 步骤 3：全绿；提交 `feat: expose drafts and the status surface over ipc`。

### 任务 4：SPA 的撰写器

**文件：**
- 新建：`client/web/src/features/composer/{Composer.tsx,
  ComposerFields.tsx, Editor.tsx, documentWire.ts, useAutosave.ts}` + 测试；
  修改 `client/web/src/components/layout/AppShell.tsx`（新邮件动作 + 撰写
  面板）、`client/web/package.json`（KY 版本的 TipTap v3 + 裁剪扩展集）
- 移植自：KY `kylins.client.frontend` 的 composer/TipTap 接线；MC 字段语义。

**接口：**
- 产出：documentWire.ts（TipTap 状态 <-> ComposerDocument JSON 双向、确定
  性）；useAutosave（空闲三十秒、编辑即重置、派发 Save；未变更时跳过）；
  Composer（To/Cc/Bcc 开关、Subject、From 只读、编辑器、按状态 surface 的
  徽标、Discard 与 Close 按钮）；新邮件铸组合；应用启动恢复从引擎草稿水合
  （经恢复拉取命令或经 dispatch 的草稿读取——选诚实缝，多半是本任务加的
  `draft_resume()` 拉取命令并测试）。

- [ ] 步骤 1：按简报清单测试先行（双向往返含列表/链接/换行；假计时器的
  自动保存时序；徽标转换；新邮件铸造；恢复水合；丢弃/关闭以精确载荷派发）。
- [ ] 步骤 2：实现；npm install（TipTap 无 postinstall；allowScripts 不动）。
- [ ] 步骤 3：vitest + build + gate 绿；提交
  `feat: add the composer with autosave and resume`。

### 任务 5：活体存储验证与文档

**文件：**
- 修改：`crates/astra-app/src/tests_live_harness.rs`（+草稿测试）；
  `docs/capabilities.md` + zh 镜像；
  `docs/changelog/unreleased/compose-drafts.md`

- [ ] 步骤 1：门控草稿测试：连接 astra1 -> 经 dispatch Save 文档 -> 断言
  引擎库持有草稿（存储读或恢复路径）-> Discard -> 断言消失 -> 再 Save ->
  恢复路径返回内容。
- [ ] 步骤 2：活体运行：harness up、门控绿、slice-check PASS；记录输出。
- [ ] 步骤 3：capabilities 行（compose -> partial、drafts -> partial）+ zh
  镜像 + fragment（Platforms: desktop、Bump: minor）。
- [ ] 步骤 4：全量验证；提交
  `feat: prove drafts against the live store and record the capability`。

### 任务 6：收尾

- [ ] 步骤 1：全量验证绿；推 `sp2d-compose-drafts` 分支；开 PR 链接
  spec；验收证据；清台账。

## 自审记录

- Spec 覆盖：D1（T1）、D2（T2）、D3（T2 状态 surface + T3 ipc）、D4（T4）、
  D5（T4 入口/恢复）、活体验证 + 文档（T5）。
- 接口链：ComposerDocument（T1）-> 草稿族（T2）-> ipc 线（T3）-> 撰写器
  （T4）-> 活体验证（T5）。
- 携带风险：68eab00 上引擎 put/delete_draft 签名（T2 步骤 3 指名）；
  TipTap 扩展集裁剪 vs 文档模型保真（T4 钉往返；裁剪集之外的 MC 文档分块
  必须原样往返或被响亮拒绝，绝不静默丢弃——钉死这一点）。
