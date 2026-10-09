# SP2d 撰写与草稿设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-09-sp2d-compose-drafts-design.md`）。

日期：2026-10-09
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 5、15 节）

## 1. 目的

写邮件且不丢：移植撰写文档模型、裁剪版 TipTap 编辑器、基于引擎服务器侧
草稿的草稿族（保存、丢弃、关闭）、空闲自动保存、恢复、草稿状态 surface。
回复与转发（引用、发件人预填）随发送切片到来；本片从「新邮件」动作与既有
草稿恢复打开。

## 2. 决策

- **D1**：把 `mailcal-composer` 移植为 `crates/astra-composer`
  （MC 自带的 serde 版 ComposerDocument、确定性 HTML 与纯文本渲染、附件与
  内联 CID 清单），裁到 SP2d 编辑器所产内容；composer-security 姿态
  （生成 HTML 无脚本）随 MC 契约继承。
- **D2**：草稿族按 MC 形态移植：`DraftsIntent::{Save, Discard, Close}` 带
  `CompositionId` newtype；保存经引擎 `put_draft` 按服务器侧存储键写入，
  丢弃经 `delete_draft`；自动保存为组合三十秒空闲后核心侧触发（tokio
  time；astra-app 只可用 sync 加 time）。
- **D3**：新 `Surface::DraftStatus`（不去抖）携带最小状态快照（组合 id、
  状态 saving 或 saved、保存时刻）；SPA 拉取它为撰写器打徽标。
- **D4**：SPA 撰写器用 TipTap（kylins 选型）裁剪扩展集（段落、粗体、
  斜体、下划线、无序与有序列表、链接、硬换行）；编辑器状态与
  ComposerDocument JSON 线互相转换；To、Cc、Bcc、Subject 为普通字段；
  From 为默认发送账户（本片只读）。
- **D5**：入口：「新邮件」动作开新组合并铸 CompositionId；应用启动时若
  默认账户存在草稿则恢复进撰写器（MC 的 resume-into-composer 行为，本片
  单草稿范围）；关闭即保存后关闭；丢弃即删除。

## 3. 测试

- 单元（astra-composer）：文档 JSON 往返；HTML 与文本渲染确定性向量取自
  MC 测试；清单形状。
- 单元（astra-app，内存引擎 + 假件）：保存持久（读回存储）、空闲后才自动
  保存且不提前、丢弃移除、关闭保存后清状态、恢复路径还原内容；
  DraftStatus 每次转换都发布。
- 线上：ComposerDocument、DraftsIntent 与状态快照字面量双侧钉死。
- 前端（vitest）：编辑器与文档线双向往返；空闲后派发 Save（假计时器）而
  非每次按键；徽标反映状态 surface；新邮件铸组合；恢复水合字段与内容；
  丢弃与关闭正确派发。
- 活体（门控，harness）：经 dispatch 存草稿、断言引擎库持有；丢弃清除；
  重启路径恢复。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建与 vitest 全绿。
2. 撰写器中写的草稿空闲后自动保存、经应用重启存活（恢复还原字段与内容）、
   丢弃即移除（存储往返的活体验证）。
3. 状态徽标反映 saving 与 saved 转换。
4. `docs/capabilities.md` 行移动（compose partial、drafts partial），zh
   镜像与 changelog fragment 同变更。
5. 只允许增量线上变更；既有夹具与测试原样通过。
