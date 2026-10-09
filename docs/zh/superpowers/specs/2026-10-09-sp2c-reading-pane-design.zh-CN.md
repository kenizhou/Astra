# SP2c 阅读窗设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-09-sp2c-reading-pane-design.md`）。

日期：2026-10-09
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 5、15 节）

## 1. 目的

读一封邮件：经引擎取正文、按 mail-calendar 的渲染安全姿态净化 HTML、解析
内联 CID 图片、把远程图片关在显式开关之后、列出附件元数据、打开即标已读。
阅读窗替换其占位。

## 2. 决策

- **D1**：新 `Surface::Reading` 携带 `ReadingSnapshot`（消息引用、主题、
  发件人显示、日期、净化后的 `html: Option<String>`、`plain:
  Option<String>`、`attachments: Vec<AttachmentRow>`（id、file_name、
  media_type、size）、`remote_image_count`），在 astra-viewmodel。SP2c 单
  阅读器（面板）：单一 `Surfaced<Option<ReadingSnapshot>>`，无逐阅读器
  map（mail-calendar 的 `SurfacedMap` 与窗口阅读器随多窗口切片回归；记录
  偏离）。
- **D2**：两个新 Intent：`OpenMessage { message: MessageRef }`（面板阅读器
  隐含；MC 的 `ReaderId` 参数是记录的偏离）与 `CloseReading`。打开经既有
  写路径标已读并发布快照；`RefreshMail` 在消息仍在时重发布打开的阅读、
  消失则关闭该 surface。
- **D3**：正文管线在 Rust 内：engine `message_source` 解码 -> 按 MC 净化器
  配置镜像用 ammonia 净化（读 MC 阅读路径与 rendering-security 契约；
  ammonia 4 入 workspace 表）-> 内联 CID 图片由引擎附件部件解码为 `data:`
  URI、超过尺寸上限的跳过并留 alt 文本 -> 远程 `src` 改写为
  `data-astra-src` 占位并计数呈现，不开开关就没有任何远程加载。纯文本
  替代原样透传。
- **D4**：SPA 在沙箱 iframe（`sandbox` 无 `allow-same-origin`、`csp` 限
  `img-src` 为 `data:`）中以 srcdoc 渲染净化 HTML；「显示远程图片」开关按
  消息把 `data-astra-src` 客户端改回 `src`；附件为只读元数据行；无 HTML
  时渲染纯文本；无打开内容时空态。
- **D5**：SP2c 字节不经文件跨界：内联图片以 data URI 随 HTML 字符串走，
  附件载荷仅元数据（保存随附件切片到来）。

## 3. 测试

- 单元（astra-app）：从 MC 阅读测试移植净化向量（script 剥离、style 约束、
  链接策略）；CID 解析（内联部件成 data URI；超限跳过）；远程占位改写与
  计数；打开即已读；刷新重发布与消失即关闭。
- 线上：ReadingSnapshot 字面量双侧钉死；新 Intent 字面量双侧钉死；
  `surface_changed` 携带 `reading`。
- 前端（vitest）：沙箱 iframe 渲染净化 HTML；开关前远程图片缺失、开后
  存在；附件行；纯文本回退；空态；从列表打开派发 `open_message`。
- 活体（门控，harness）：Projects 文件夹种一封富邮件（HTML 正文、一张
  远程图片、一张内联 CID 图片、一个附件）；门控测试断言净化输出、远程
  拦截计数、data URI CID、附件行与打开即读转换。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建与 vitest 全绿。
2. 打开种子邮件渲染净化 HTML：远程图片默认拦截、开关后可见；内联 CID 可
  见；附件元数据显示；消息标已读（未读数下降，经 RefreshMail 存活）。
3. CloseReading 与消息移除都会关闭阅读 surface。
4. `docs/capabilities.md` 行移动（reading 到 partial、rendering security 到
   partial），zh 镜像与 changelog fragment 同变更。
5. 只允许增量线上变更：既有夹具与测试原样通过。
