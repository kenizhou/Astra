# SP2c 阅读窗实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-09-sp2c-reading-pane.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 端到端交付阅读窗：ReadingSnapshot 与 Reading surface、
OpenMessage/CloseReading Intent、Rust 正文管线（净化、CID data URI、远程
门控）、带远程图片开关的沙箱渲染器、对富种子邮件的活体验证。

**架构：** 新推送 surface `Reading` 走单一
`Surfaced<Option<ReadingSnapshot>>`；正文管线在 astra-app（镜像 MC 的
ammonia 净化、引擎部件解码、data URI 内联、占位改写）；SPA 在沙箱 srcdoc
iframe 中渲染净化 HTML，只拥有开关变换。

**技术栈：** 既有 workspace 加 ammonia 4（workspace 表，MC 的 pin）；engine
`message_source` 读取；SPA 的 iframe sandbox CSP。

**Spec：** `docs/superpowers/specs/2026-10-09-sp2c-reading-pane-design.md`

## 全局约束

- 沿 SP1/SP2a/SP2b 家规：SPDX、500 行上限、deny-everything lints、提交前
  gate（绝不接管道）、conventional commits、无破折号、英式英语、zh 镜像同
  提交、只允许增量线上变更。
- 移植源根：MC = D:/Projects/mailclient/opensource/mail-calendar（读 MC 的
  阅读路径：mailcal-app reading.rs、viewmodel reading.rs 与
  rendering-security 姿态）；锁定 rev 的引擎经
  D:/Projects/mailclient/opensource/email-calendar-sync-engine 的
  `git show 68eab00:<path>` 读。
- 适配规则 AR1-AR7 适用。

---

### 任务 1：viewmodel 的 ReadingSnapshot

**文件：**
- 新建：`crates/astra-viewmodel/src/{reading.rs, reading_tests.rs}`；修改
  `lib.rs`

**接口：**
- 产出：`ReadingSnapshot`（message: MessageKey { account, key }、subject、
  from_name、from_address、sent_at、html: Option<String>、plain:
  Option<String>、attachments: Vec<AttachmentRow>、remote_image_count: u32）、
  `AttachmentRow`（id: u32、file_name、media_type、size: u64）；serde 按家规
  （Option 与空 Vec skip_serializing_if，需要处 rename_all snake_case）。

- [ ] 步骤 1：线上测试先行：完整字面量（html + plain + attachments + count
  2）、最小字面量（仅 plain，线上无可选键）、双向往返。
- [ ] 步骤 2：实现；lib 再导出。
- [ ] 步骤 3：测试/clippy/fmt/gate 绿；提交
  `feat: add the reading snapshot view model`。

### 任务 2：astra-app 的 Intent 与正文管线

**文件：**
- 修改：`crates/astra-app/src/{protocol/intent.rs, protocol.rs, reference.rs
  （若需再导出）, dispatch.rs}` + 测试；新建：
  `crates/astra-app/src/{reading.rs, reading_tests.rs, sanitise.rs,
  sanitise_tests.rs}`；修改 `Cargo.toml`（ammonia workspace 依赖）与
  workspace 表
- 移植自：MC `mailcal-app/src/{reading.rs, sanitise 或等价物}` 及其测试；
  锁定 rev 的引擎读取。

**接口：**
- 产出：`Intent::OpenMessage { message: MessageRef }`、
  `Intent::CloseReading`（线上钉死）；`Surface::Reading` 变体（事件名
  `reading`、不去抖）；App 内 `Surfaced<Option<ReadingSnapshot>>`；管线函数
  `render_reading(engine, account, message) -> ReadingSnapshot`（按 MC 的
  ammonia 配置净化；CID 部件转 data URI 带尺寸上限常量；远程 src 转
  `data-astra-src` 并计数）；打开经既有写路径标已读；RefreshMail 重发布或
  消失即关闭。

- [ ] 步骤 1：protocol 线上测试先行（两个 Intent、reading 事件名）。
- [ ] 步骤 2：净化测试先行：移植 MC 净化向量（script 剥离、style/jscript
  策略、允许的标签/属性表、URL scheme）；CID 用例（内联部件 -> data URI、
  超限跳过留 alt、缺失部件对 plain 保持 cid 原样）；远程用例
  （http/协议相对 src 改写并计数、data: 与内联后的 cid: 不动）。
- [ ] 步骤 3：dispatch 测试先行（内存引擎 + 供源与部件的假件）：打开发布
  Reading 并标已读（未读翻转、刷新存活）；关闭清空；消息消失时刷新关闭；
  仍在时刷新重发布新内容。
- [ ] 步骤 4：实现（先读 68eab00 上 message_source/attachment 签名再适配
  MC 调用）。
- [ ] 步骤 5：全绿；提交 `feat: add the reading pipeline and open/close intents`。

### 任务 3：宿主命令与 IPC 线

**文件：**
- 修改：`client/desktop/src/{commands.rs, commands_tests.rs, observer.rs（若
  枚举 surface 列表）}`、`client/web/src/services/{ipc.ts, ipc.test.ts}`

**接口：**
- 产出：`reading() -> Option<ReadingSnapshot>` 拉取命令（serde 直通）；
  `surface_changed` 载荷携带 `reading`；ipc.ts 的
  ReadingSnapshot/AttachmentRow 类型与两个 Intent；线上测试按 Rust 侧字面量
  钉死。

- [ ] 步骤 1：commands_tests 先行：拉取返回已发布快照（关闭后 None）；无
  panic 路径。
- [ ] 步骤 2：实现并注册；ipc.ts 类型 + 线上测试（镜像 Rust reading_tests
  字面量）。
- [ ] 步骤 3：全绿；提交 `feat: expose the reading surface over ipc`。

### 任务 4：SPA 的阅读窗

**文件：**
- 修改：`client/web/src/components/layout/{AppShell.tsx, ReadingPane.tsx}`
  （替换占位）；新建
  `client/web/src/components/reading-pane/{MessageHtml.tsx, AttachmentList.tsx,
  ReadingHeader.tsx}`；`client/web/src/stores/readingStore.ts`；扩展
  `client/web/src/events/surfaceEvents.ts`（reading -> 拉取）与消息列表
  （选中即打开）

- [ ] 步骤 1：测试先行：点击行派发 `open_message`（载荷精确）且窗从快照渲染
  头部（主题/发件人/日期）；沙箱 iframe 获得净化 srcdoc（断言 sandbox 属性
  + csp img-src data:）；文档中远程图片缺失且显示 `remote_image_count`；
  开关把占位换回 src 且计数清零；附件渲染元数据行；无 html 时纯文本回退；
  无内容空态；关闭按钮派发 `close_reading`；选中高亮跟随打开的消息。
- [ ] 步骤 2：实现；列表单击即打开（选中 = 打开；记录）。
- [ ] 步骤 3：vitest + build + gate 绿；提交
  `feat: render the reading pane with sandboxed html and remote gating`。

### 任务 5：富 harness 邮件与活体验证

**文件：**
- 修改：`docker/harness/stalwart/seed.sh`（一封富 MIME 邮件进 Projects：
  HTML 正文 + 远程图片 + 内联 CID png + 一个附件；按主题搜索幂等）、
  `crates/astra-app/src/tests_live_harness.rs`（+阅读测试）
- 修改：`docs/capabilities.md` + zh 镜像；
  `docs/changelog/unreleased/reading-pane.md`

- [ ] 步骤 1：扩展种子（手搓 MIME：极小合法 PNG 内联部件 + 小附件；主题
  "Harness rich note"）；不破坏 INBOX 三邮件契约（此邮件在 Projects）。
- [ ] 步骤 2：门控阅读测试：打开富邮件 -> 净化 html 存在、
  remote_image_count >= 1、内联 data:image/png URI、一行附件、未读转已读经
  RefreshMail 存活。
- [ ] 步骤 3：活体运行：harness up、门控测试绿、slice-check PASS、列表测试
  仍绿（INBOX 三邮件不动）；记录输出。
- [ ] 步骤 4：capabilities 行（reading -> partial、rendering security ->
  partial）+ zh 镜像 + fragment（Platforms: desktop、Bump: minor）。
- [ ] 步骤 5：全量验证；提交
  `feat: prove the reading pipeline live and record the capability`。

### 任务 6：收尾

- [ ] 步骤 1：全量验证绿（gate 不接管道、workspace 测试、cargo doc、
  desktop 构建、web 构建 + vitest）；树干净。
- [ ] 步骤 2：推 `sp2c-reading-pane` 分支；开 PR 链接 spec；验收证据；按
  SDD 技能清台账。

## 自审记录

- Spec 覆盖：D1（T1）、D2/D3（T2）、D4（T4）、D5（T2 管线设计 + T4
  iframe）、宿主缝（T3）、活体验证 + 文档（T5）。
- 接口链：ReadingSnapshot（T1）-> 管线 + Intent（T2）-> ipc 线（T3）->
  阅读窗（T4）-> 活体验证（T5）。
- 携带风险：68eab00 上的引擎部件/附件解码面必须先读再移植 MC 的正文解码
  （T2 步骤 4 指名）；WebView2 对 iframe CSP 属性的支持需运行时确认（T4 在
  jsdom 断言 sandbox 属性、报告里如实记录）。
