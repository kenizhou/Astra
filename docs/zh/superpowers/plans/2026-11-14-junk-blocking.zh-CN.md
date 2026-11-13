# 垃圾邮件阻止计划

规格：docs/superpowers/specs/2026-11-14-junk-blocking-design.md。分支
`junk-blocking` 自规格提交从 main 分出。每个任务：先红、实现、电池、一个约定式提
交。一个契约变更：新 docs/junk-blocking.md 连同其 zh 镜像在同一提交，capabilities
行连同其 zh 镜像移动，落变更日志碎片，porting-backlog.md 把 junk blocking 移入已
交付列表。无引擎改动、无新 store、无 AI bump。仅抑制（参考红线；归档仍是 Junk 动
词的事）。

### 任务 1：阻止列表偏好 + 抑制门（D1）

**文件：** crates/astra-app/src/preferences/mod.rs（blocked_senders:
BTreeSet<String>，sender_names 模式，空则 skip_serializing_if）；
crates/astra-app/src/protocol/intent_settings.rs（BlockSender/UnblockSender 臂）+
settings/mod.rs（set_setting setter 腿，地址规范化）；
crates/astra-app/src/notifications.rs（arm 刷新阻止集，qualifies 的「发件人未被阻
止」腿，读失败开放，被阻止胜过 VIP）+ 测试。

- [ ] 在钩子处确认：sender_names 偏好模式（preferences/mod.rs:156-158）、
      set_setting（settings/mod.rs:147-164）、SettingsIntent（intent_settings.rs:60）、
      NewMailQueue arm（notifications.rs:123）+ qualifies（:239-250）、sync 设置读
      （sync.rs:106-111）、VIP normaliseAddress 惯用法。测试先行（设置 intent 往
      返、地址规范化、队列抑制被阻止发件人 notice、未被阻止仍合格、读失败开放、被
      阻止胜过 VIP 顺序）；随后偏好 + intent + 门。提交
      `feat: the blocked-sender preference and suppression gate`。
- [ ] astra-app 不豁免上限（notifications.rs/preferences/mod.rs 增长保持 500 内；
      必要时子模块）。newtype；地址规范化小写/去空白/addr-spec。门是确定性新邮件
      lane，非 AI incoming pass（无 eval/bump）。既有测试保持绿。

### 任务 2：web 阻止面 + 契约 + 活实证（D2，D3）

**文件：** 精简行菜单「Block sender」项（rowMenu.ts 接近上限，紧邻 vipItem）；新独
立 BlockSenderDialog 组件（阻止 + 可选「Also report this conversation as junk」复
选框默认开，复用已交付垃圾举报动词；举报失败绝不撤销阻止）+ 测试；设置管理段（列
被阻止发件人 + 解除，既有类别）；阻止列表经设置 wire 读（无新 ipc.ts 类型）；
docs/junk-blocking.md + zh；docs/capabilities.md 行 + zh；
docs/changelog/unreleased/junk-blocking.md（Platforms: desktop，Bump: minor）；
docs/porting-backlog.md + zh；crates/astra-app/src/tests_live_harness/
junk_blocking_proof.rs（阻止发件人、经 harness 提交路径从其送达、断言队列无
notice；解除、再送达、断言 notice 返回；撤回副本）+ tests_live_harness.rs 注册。

- [ ] 测试先行（阻止对话框阻止 + 可选举报腿、举报失败绝不撤销阻止、管理段解除；离
      线跳过的门控活实证）；随后对话框 + 菜单 + 设置 + 文档。提交
      `feat: the junk blocking surface and contract`。
- [ ] client/web 不豁免：对话框自成组件（rowMenu.ts 接近上限，菜单项保持精简）；
      ipc.ts 在上限（字段走既有设置快照，无新类型）。vitest 陷阱：act() 测试中段，
      整体 mock 服务。注释现在时；无 em/en 破折号；英式英语；源文件以换行结尾（CI
      rustfmt 更严）。docs/zh 镜像同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；cargo
      clippy --workspace --all-targets -- -D warnings；cargo test --workspace；
      npm --prefix sidecar run build && npm --prefix sidecar test；npx tsc
      --noEmit && npx vitest run（client/web）；cargo run -p xtask -- gate。若
      plugin 测试以 AlreadyGranted 失败，清理陈旧 /tmp/astra-desktop-plugin-*
      scratch 目录（Windows pid 回收 flake）再重跑。留意磁盘空间（增量缓存膨胀则
      清理）。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后 up +
      env；cargo test -p astra-app tests_live_harness::junk_blocking_proof --
      --test-threads=1），随后 unset。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步 main；ledger
      清理。

## 自审笔记

- 规格覆盖：D1 的偏好 + 门（T1）；D2 的 web 面 + D3 的契约 + 活实证 + 电池 + PR
  （T2）。
- 风险：仅抑制红线（无自动归档；归档仍是 Junk 动词的事，命名非目标）；门是确定性
  lane 非 AI pass（无 eval/bump）；阻止列表读失败开放；被阻止胜过 VIP；rowMenu.ts
  接近上限（精简项 + 独立对话框）；ipc.ts 在上限（设置 wire）。设备本地阻止列表、
  无 CLI 动词、设置类别细节（记录缺口）。
- 不评测/不提升 pipeline（面向模型不变）。最小抑制范围无参考偏差。活实证教训：先
  reset harness；若电池 flake 清理陈旧 plugin scratch 目录；确保末尾换行。