# 关注回复计划

规格：docs/superpowers/specs/2026-11-14-watch-replies-design.md。分支
`watch-replies` 自规格提交从 main 分出。每个任务：先红、实现、电池、一个约定式提
交。一个契约变更：新 docs/watch-replies.md 连同其 zh 镜像在同一提交，capabilities
行连同其 zh 镜像移动，落变更日志碎片，porting-backlog.md 把 watch-replies 移入已交
付列表。无引擎改动、无 AI bump。member-key 重解析入 v1（衍生 thread 重新命名陷阱）。
无焦点抑制 + drain 时消费是升级的参考偏差。

### 任务 1：关注 store + 核心动词 + 检查 + 偏好（D1，D2）

**文件：** 新 crate crates/astra-watches（Cargo.toml + src/lib.rs + src/store.rs +
src/migrations/0001_init.sql，astra-reminders 形态；watch_replies 表
[account_id, thread_id, member_key, created_at, 主键(account_id,thread_id)]；
watch/unwatch/list，一次性消费 = unwatch，畸形跳过）+ store 测试；
crates/astra-app/src/watch_replies.rs（install_watch_store + watch_thread/
unwatch_thread[如 categories.rs resolve_thread_key 解析 + member-key 捕获] +
list_watches + check_watched_replies(batch) async pass 后[偏好门，按账户分组，每账
户一次 mail_by_keys 取批次 key + 关注 member key，解析对解析匹配，构建 notice，消
费，失败开放]）+ 钩子测试；watch_replies_enabled 偏好（preferences/mod.rs 字段 +
state.rs 投影 + persist，SetWatchRepliesEnabled SettingsIntent 臂[settings/mod.rs
在上限回收]，SettingsBlock wire 字段）；App 槽（lib.rs 在上限回收）。

- [ ] 在钩子处确认：astra-reminders store 形态、categories.rs resolve_thread_key
      （:158-180）、list_rows::thread_key（:312-317）、mail_by_keys（engine-api
      reads.rs:280）、mail_on_threads（reads.rs:261）、notifications_enabled 偏好模
      式、dispatch fakes（FakeProvider + deliver + RefreshMail + drain）。测试先行
      （store：一次性/幂等/list 序/账户域/迁移竞争；钩子：关注 + 回复触发、重新命名
      回归[回复 msgid 排在原消息前]、偏好关 -> 无 notice + 关注未消费、未关注 ->
      无、消费后第二条回复 -> 无、失败开放、provider-threaded 腿）；随后 store + 核
      心 + 偏好。提交 `feat: the watch store and reply check`。
- [ ] astra-watches/astra-app 不豁免上限（lib.rs/settings/mod.rs 在上限回收）。
      newtype。member-key 重解析承重（衍生 thread 合并重新命名；两侧检查时经引擎解
      析）。无引擎写。既有测试保持绿。

### 任务 2：宿主钩子 + 回复通知（D3 宿主）

**文件：** boot 安装（boot.rs assemble_state 打开 watches.sqlite +
install_watch_store，categories 模式，无定时器，WATCHES_STORE_FILE 常量）；
WatchPort supertrait（app_state/watch_port.rs，trait AppPort: WatchPort）+ 每 impl
子块（app_state/port、commands_fakes/port、showcase/app）；新
commands/watch_replies.rs（watch_set/watch_clear/watch_list，commands.rs 在上限）
在 lib.rs 注册；HostDelivery::pass 内 pass 钩子（delivery.rs:274-277，drain 后，门
控 notifications_enabled）调 check_watched_replies + NotifyBridge::deliver_replies；
notify.rs deliver_replies（deliver_reminders 形态，无焦点抑制，标题「Reply
received」，reply 事件 REPLY_EVENT + with_reply_history + windowed 接线）。

- [ ] 测试先行（notify-bridge 测试：deliver_replies 尊重 enabled、无焦点抑制、reply
      事件；命令路由基于 fakes）；随后 boot + supertrait + 命令 + 钩子 + 通知。提交
      `feat: the watch reply hook and notice`。
- [ ] client/desktop 不豁免上限：WatchPort supertrait 使 app_state.rs 保持上限
      （E0119 不同 trait）；commands.rs 在上限（子模块）。同步 NewMailQueue commit
      fold 不动（钩子是 async pass 后）。drop 序不动（无定时器、无 AppState 字段）。
      既有测试保持绿。

### 任务 3：web 面 + 契约 + 活实证（D3 web，D4）

**文件：** 新 client/web/src/services/watchReplies.ts（invoke 包装，绝不在 ipc.ts）；
features/watch-replies/watchStore.ts（zustand 关注集从 watch_list 播种，切换刷新）+
watchMenu.ts（Watch replies/Unwatch，单行，批量禁用，blockSenderMenu 模式）+ 测试；
rowMenu.ts 接线（在上限，构建器 import + 数组行）；events/replyFlow.ts +
replyEvents.ts（reminderFlow 克隆）+ App.tsx 挂载喂 notificationHistoryStore；
General 设置 watch_replies_enabled 开关（GeneralDetail.tsx）；如需 surfaces.ts；
docs/watch-replies.md + zh；docs/capabilities.md 行 + zh；
docs/changelog/unreleased/watch-replies.md（Platforms: desktop，Bump: minor）；
docs/porting-backlog.md + zh；crates/astra-app/src/tests_live_harness/
watch_replies_proof.rs（提交原消息、经核心关注、送达带 In-Reply-To 回复[回复 msgid
排在前，证明真实 wire 重新命名存活]、运行 pass + 检查、断言一条「Reply received」
notice + 关注行消失 + 第二条回复无 + 两消息在 INBOX 未移动、撤回）+
tests_live_harness.rs 注册。

- [ ] 测试先行（菜单标签翻转 + 批量禁用、服务、设置开关、reply 事件挂载；离线跳过的
      门控活实证）；随后 web + 文档。提交
      `feat: the watch replies surface and contract`。
- [ ] client/web 不豁免：rowMenu.ts 在上限（watchMenu 构建器）；ipc.ts 在上限
      （services/watchReplies.ts）。vitest 陷阱：act() 测试中段，整体 mock 服务。注
      释现在时；无 em/en 破折号；英式英语；源文件以换行结尾。docs/zh 镜像同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；cargo
      clippy --workspace --all-targets -- -D warnings；cargo test --workspace；npm
      --prefix sidecar run build && npm --prefix sidecar test；npx tsc --noEmit &&
      npx vitest run（client/web）；cargo run -p xtask -- gate。若 plugin 测试以
      AlreadyGranted 失败，清理陈旧 /tmp/astra-desktop-plugin-* scratch 目录
      （Windows pid 回收 flake）再重跑。留意磁盘空间（增量缓存膨胀则清理）。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后 up +
      env；cargo test -p astra-app tests_live_harness::watch_replies_proof --
      --test-threads=1），随后 unset。
- [ ] 分支推送；PR（预期 squash）在正文升级无焦点抑制 + drain 时消费偏差；CI 观察；
      squash 合并、删分支、同步 main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的 store + D2 的核心/检查/偏好（T1）；D3 的宿主钩子/通知（T2）；D3 的
  web + D4 的契约 + 活实证 + 电池 + PR（T3）。
- 风险：衍生 thread 重新命名陷阱（member-key 重解析必须入 v1；两侧检查时经引擎解
      析；重新命名回归测试是杀手测试）；pass 后钩子（同步 commit fold 不动）；上限
      压力（lib.rs/settings/mod.rs/commands.rs/app_state.rs 在上限 -> 回收/
      supertrait；rowMenu.ts 在上限 -> 构建器）；升级的无焦点抑制 + drain 时消费偏
      差。溢出折叠/仅背景 lane/陈旧成员链接/设备本地/无 CLI/徽章/列表/多选（记录缺
      口）。
- 不评测/不提升 pipeline（非 AI）。无引擎改动。活实证教训：先 reset harness；回复
  msgid 排在前证明重新命名存活；若电池 flake 清理陈旧 plugin scratch 目录；确保末尾
  换行。