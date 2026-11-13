# 附件浏览器计划

规格：docs/superpowers/specs/2026-11-14-attachment-browser-design.md。分支
`attachment-browser` 自规格提交从 main 分出。每个任务：先红、实现、电池、一个约定
式提交。一个契约变更：新 docs/attachment-browser.md 连同其 zh 镜像在同一提交，
capabilities 行连同其 zh 镜像移动，落变更日志碎片，porting-backlog.md 把附件浏览
器移入已交付列表。无引擎改动。miss 即取扫描是升级的参考偏差（规格陈述；PR 升级）。

### 任务 1：核心聚合 + view-model + 命令（D1，D2）

**文件：** 新 crates/astra-app/src/attachment_browser.rs（impl App
attachment_browser(filter) -> AttachmentBrowserView：按账户 search_mail
has_attachment:true 上限 200 -> mail_by_keys -> message_attachments -> 平铺部件
beside 源元数据；每邮件尽力，每账户诚实错误；行 + 覆盖报告；发件人 + mime 前缀过
滤）+ 兄弟测试；新 crates/astra-viewmodel/src/attachment_browser.rs（条目：部件
id u32/file_name/media_type/size + MessageKey account+key + subject/from/sent_at；
覆盖报告；从 lib.rs 重导出）；App 槽/方法（lib.rs 在上限，回收注释行）；新
client/desktop/src/commands/attachment_browser.rs（commands.rs 在上限）经新
AppPort 方法，在 lib.rs 注册；web wire 类型 + 包装在 services/attachments.ts（绝不
在 ipc.ts）。

- [ ] 在钩子处确认：search_mail（engine-api reads.rs:33）、mail_by_keys（:273）、
      message_attachments（body.rs:169）、MailRow 元数据（search_index/mail.rs:53）、
      search.rs 账户 handle 模式（:101）、AttachmentRow（astra-viewmodel/
      reading.rs:77）、save_attachment（commands/attachments.rs:133，
      App::attachment_content）。测试先行（核心：基于 fake provider 播种
      multipart 源，断言平铺行 + 源引用 + 最新优先 + 上限 + 覆盖报告 + 每账户错误
      诚实 + 发件人/mime 过滤；命令：畸形 ref 拒绝，拉取答视图）；随后核心 +
      view-model + 命令。提交 `feat: the attachment browser aggregation`。
- [ ] astra-app/astra-viewmodel/client/desktop 不豁免上限：lib.rs 在上限（回收注释
      行，不超 500），commands.rs 在上限（子模块）。跨边界 newtype；部件 id 保持消
      息域 u32。无引擎改动（经 message_attachments miss 即取）。既有测试保持绿。

### 任务 2：modal + 入口 + 契约 + 活实证（D3）

**文件：** 新 client/web/src/features/attachment-browser/（AttachmentBrowser.tsx +
attachmentBrowserStore.ts + 纯分组/chip 助手）+ 测试，复制 ShortcutsHelp 覆盖层模
式（zustand isOpen、闭时 null、useDialogFocus、背景/Escape、role=dialog
aria-modal），在 AppShell 尾部紧邻 CommandPalette 挂载；行按发件人分组（最新优先、
计数 + 总大小），mime chips（All + 顶部桶，image/* -> Images，application/pdf ->
PDF，源自 mime 未过滤快照），发件人预过滤横幅 + Clear，覆盖脚注 + Refresh，空状
态；Save As 复用 saveAttachment，打开源邮件派发 intents.openMessage + 关 modal；
palette 命令（saved-searches 形态，commands.ts）；docs/attachment-browser.md + zh；
docs/capabilities.md 行 + zh；docs/changelog/unreleased/attachment-browser.md
（Platforms: desktop，Bump: minor）；docs/porting-backlog.md + zh；
crates/astra-app/src/tests_live_harness/attachment_browser_proof.rs（提交带附件笔
记、同步、经核心方法浏览、断言行 + 字节读、撤回两副本）+ tests_live_harness.rs 注
册。

- [ ] 测试先行（纯分组/chip 表测试；视图测试；离线跳过的门控活实证）；随后 modal +
      入口 + 文档。提交 `feat: the attachment browser surface and contract`。
- [ ] client/web 不豁免：新文件在 features/attachment-browser/；ipc.ts 在上限
      （services/attachments.ts）；AppShell.tsx 281 / commands.ts 344 有余量。modal
      非第四视图（ShellView 不变，栏不动）。Preview 保持占位。vitest 陷阱：act()
      测试中段，整体 mock 服务。注释现在时；无 em/en 破折号；英式英语；源文件以换
      行结尾（CI rustfmt 更严）。docs/zh 镜像同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；cargo
      clippy --workspace --all-targets -- -D warnings；cargo test --workspace；
      npm --prefix sidecar run build && npm --prefix sidecar test；npx tsc
      --noEmit && npx vitest run（client/web）；cargo run -p xtask -- gate。若
      plugin 测试以 AlreadyGranted 失败，清理陈旧 /tmp/astra-desktop-plugin-*
      scratch 目录（Windows pid 回收 flake）再重跑。留意磁盘空间（增量缓存膨胀则
      清理）。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后 up +
      env；cargo test -p astra-app
      tests_live_harness::attachment_browser_proof -- --test-threads=1），随后
      unset。
- [ ] 分支推送；PR（预期 squash）在正文升级 miss 即取参考偏差；CI 观察；squash 合
      并、删分支、同步 main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的聚合 + D2 的 view-model/命令（T1）；D3 的 modal + 入口 + 契约 + 活
  实证 + 电池 + PR（T2）。
- 风险：升级的 miss 即取参考偏差（kylins 纯缓存；记录 + 升级，纯缓存对齐后续）；
  has_attachment 猜测覆盖警告；N _body_fetch 成本（200 上限有界，陈述）；在上限的
  lib.rs/commands.rs（回收 + 子模块）；无持久索引差异。Preview 占位、无
  ContactDetail 预过滤/日期界限/Clear、CLI 动词延后（记录缺口）。
- 不评测/不提升 pipeline（面向模型不变）。活实证教训：先 reset harness；若电池
  flake 清理陈旧 plugin scratch 目录；确保末尾换行（CI rustfmt）。