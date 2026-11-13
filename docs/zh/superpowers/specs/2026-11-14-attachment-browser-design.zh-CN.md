# 附件浏览器设计

**状态。** 2026-11-14 定稿。切片自此提交从 main 分出。

**范围。** 附件仅能在阅读窗按邮件触及；没有跨邮件浏览。本 kylins 附加新增附件浏览
器：一个瞬态 modal，列出跨邮件附件，按发件人分组，按 mime 过滤，带 Save As 与打开
源邮件。它是对引擎既有缝的有界扇出（search has_attachment -> 解析 ->
message_attachments），一个返回列表 + 覆盖报告的命令（非新 Surface），一个 modal
覆盖层（非第四视图）。无引擎改动；不评测/不提升 pipeline。

## 参考偏差，升级（AGENTS.md）

锁定的 kylins 参考（attachment_index.rs、attachment-browser 特性）以纯缓存扫描构
建可重建 SQLite 索引：读缓存原始源，绝不拨服务器（miss 计为 skipped）。Astra 唯一
引擎缝（message_attachments）缓存优先但 miss 即取，且 engine-api 无纯缓存附件读。
本切片交付有界 miss 即取（无引擎改动）：首开经网络预热被扫描邮件的源，受扫描上限
约束，覆盖脚注如实陈述窗口。这是对参考纯缓存扫描的刻意偏差，在此与 PR 升级，非静
默。理由：纯缓存缝会是 engine-api vendor 编辑（每次上游 sync 后重施）；有界取保持
引擎边界清洁、切片自包含。若 owner 要参考保真，纯缓存对齐（cached_message_attachments
缝）是命名后续。has_attachment 标志在源落定前是提供商的猜测（真猜测无部件贡献零
行；假猜测带部件被漏）；覆盖警告记录在案。

## D1：核心聚合

新 crates/astra-app/src/attachment_browser.rs（lib.rs 在上限，回收数行注释），
impl App 方法 attachment_browser(filter) -> AttachmentBrowserView。按账户 handle
（search.rs 模式）：search_mail("has_attachment:true", cap) -> mail_by_keys ->
每邮件 message_attachments -> 把部件平铺 beside 源行元数据（account、key、
subject、from、sent_at）。扫描上限每账户 200 邮件（warm 上限；kylins 用 500，较小
界限陈述）。每邮件失败记录并继续（尽力）；每账户失败是诚实错误条目。答带行 + 覆盖
报告（scanned/indexed/skipped/失败账户）。发件人过滤（精确、忽略大小写）与 mime 前
缀过滤收窄行。无持久索引：Astra 每开重算（常新；kylins 持久直到 clear，记录差异）。

## D2：view-model + 命令 + web 服务

新 crates/astra-viewmodel/src/attachment_browser.rs（从 lib.rs 重导出）：条目 =
部件元数据（id u32、file_name、media_type、size）beside 源引用（MessageKey
account+key）+ subject/from/sent_at，及覆盖报告。跨边界 newtype；部件 id 保持 save
命令所取的消息域 u32。新 desktop 命令在 commands/attachment_browser.rs
（commands.rs 在上限）经新 AppPort 方法拉取，在 lib.rs 注册。web wire 类型 + 包装走
services/attachments.ts（绝不在 ipc.ts，在上限）。Save As 原样复用既有
save_attachment 命令（浏览器行带 account+key+部件 id）；打开源邮件派发
intents.openMessage 并关 modal（notification-history 先例；相对 kylins v1 加性，
后者无此动作）。

## D3：modal + 入口 + 契约

瞬态覆盖层（非第四视图；ShellView 保持 mail/calendar/people，栏不动，kylins「绝不
是文件夹」红线）。新 features/attachment-browser/（AttachmentBrowser.tsx +
attachmentBrowserStore.ts + 纯分组/chip 助手），复制 ShortcutsHelp 覆盖层模式
（zustand isOpen、闭时 null、useDialogFocus、背景/Escape 关、role=dialog
aria-modal），在 AppShell 尾部紧邻 CommandPalette 挂载一次。行按发件人分组（最新
优先组、每组计数 + 总大小），mime chips（All + 顶部桶，image/* 折叠为 Images，
application/pdf 为 PDF，源自 mime 未过滤快照使活动过滤绝不折叠 chips），发件人预
过滤横幅 + Clear，覆盖脚注（从 N 缓存邮件索引、K 未缓存、扫描窗口、失败账户）+
Refresh，空状态告知用户打开邮件使源落缓存。入口是 palette 命令（saved-searches 形
态）。Preview 保持占位（阅读窗先例；字节 wire 是后续）。

契约：新 docs/attachment-browser.md（扇出、升级的 miss 即取偏差、has_attachment
猜测警告、扫描上限、无持久索引规则、记录缺口）+ zh；capabilities 行；
porting-backlog 已交付列表移动；变更日志碎片。zh 镜像同提交移动。

## 已知缺口，记录而非静默

- 纯缓存参考对齐（cached_message_attachments 引擎缝）是升级偏差的后续；本切片
  miss 即取，有界。
- has_attachment 猜测覆盖警告（带部件的假猜测被漏）。
- 无持久索引（每开重算；kylins 持久直到 clear）。
- Preview 保持占位；返回字节的 preview wire 是后续。
- 无 ContactDetail 发件人预过滤入口（store 接受发件人过滤故廉价可加）；无日期界限
  UI；无 Clear 动作。
- CLI attachments list 动词是后续（只读 messages list/search 形态）；v1 无需 MCP
  工具。

## 测试先行

每个行为都带着它的红落地。核心聚合测试基于 fake provider（播种 multipart/mixed
源，跑浏览器方法，断言平铺行 + 源引用 + 最新优先 + 上限 + 覆盖报告 + 每账户错误诚
实 + 发件人/mime 过滤）。纯分组/chip 助手表测试。web 视图测试（vitest，act() 测试
中段，整体 mock 服务）。desktop 命令测试基于 fakes + SaveHost 缝。一个门控活
harness 实证（attachment_browser_proof.rs：提交带附件笔记、同步、经新核心方法浏
览、断言行 + 字节读、撤回两副本）。无活模型证明；离线电池即证明。