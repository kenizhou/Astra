# SP4d-2 计划：已收集密钥的审阅与提升面

规格：docs/superpowers/specs/2026-11-13-sp4d-2-collected-review-design.md。分支
`sp4d-2-collected-review` 自规格提交从 main 分出。每个任务：先红、实现、电池、一
个约定式提交。一个契约变更：crypto-receive.md 新增审阅/提升注记连同其 zh 镜像在
同一提交，capabilities 行连同其 zh 镜像移动，并落一个变更日志碎片。无引擎改动。
CLI 密钥 verb + sidecar 虚拟工具是 SP4d-1b。

### 任务 1：app 审阅方法 + 提升 trust 写入（D1）

**文件：** 新 crates/astra-app/src/collected.rs 子模块（impl App：list_collected、
promote_collected、reject_collected；reading_password.rs / keys.rs 形态，lib.rs 在
上限不增长）；crates/astra-crypto-store/src/import.rs 共享 store 助手（以
no-downgrade 守卫记录提升接受，复用 record_import_trust）；如需 store.rs/lib.rs 接
线 + 测试。

- [ ] 在钩子处确认：promote_collected_key(store, account, standard, fingerprint,
      email) 排空暂存 + 绝不降级既有 keystore 行；list_collected_keys_for_account
      最新优先返回暂存行；remove_collected_key 取暂存 id（i64）。测试先行（list 返
      回暂存行；promote 落地密钥并记录 trust 决策使其可用于发送；verified 提升升
      级，unverified 提升绝不降级既有 verified 决策；reject 移除暂存行并记录
      rejected；无 email 提升仍落地密钥）；随后 app 方法 + store 助手。提交
      `feat: the collected-key review and promote methods`。
- [ ] 加密 crate 豁免上限；astra-app 不豁免（新子模块，lib.rs 不增长）。
      promote+trust 配对承重：promote 本身使解析器 Undecided、发送门中止。既有测试
      保持绿。

### 任务 2：desktop 命令 + web 审阅 UI + 契约（D2，D3）

**文件：** client/desktop/src/commands/crypto.rs（或从其声明的子模块；commands.rs
在上限，其中不加新 mod 行）新 desktop 命令，在 client/desktop/src/lib.rs 注册；新
client/web/src/features/keys/ 审阅组件（列表 + fingerprint/对端/来源/标准显示 +
Accept / Accept as verified / Reject）接入 Keys 详情；wire 类型在 services/crypto.ts
（绝不在 ipc.ts）；docs/crypto-receive.md（审阅/提升注记）+ zh；
docs/capabilities.md（审阅/提升行）+ zh；
docs/changelog/unreleased/sp4d-2-collected-review.md（Platforms: desktop，Bump:
minor）；crates/astra-app/src/tests_live_harness/crypto_collected_review.rs（门控活
实证：接收一封暂存对端密钥的 Autocrypt 邮件，经产品面列出，提升 + trust 决策，向该
对端发送仅由提升即可准入的加密邮件；拒绝第二个暂存密钥，断言移除；撤回副本）+
tests_live_harness.rs 注册。

- [ ] 测试先行（命令携带字段；web 审阅列出/接受/拒绝；离线跳过的门控活实证）；随后
      命令 + web UI + 文档。提交
      `feat: the collected-key review surface and contract`。
- [ ] astra-app/client/desktop/web 不豁免：新 core 逻辑在子模块（lib.rs/commands.rs
      在上限，不增长）；wire 类型在 services/crypto.ts（绝不在 ipc.ts）。无密钥穿过
      此面（已收集密钥是公开的）；显示 fingerprint 供带外验证。注释现在时；无 em/en
      破折号；英式英语。docs/zh 镜像同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；cargo
      clippy --workspace --all-targets -- -D warnings；cargo test --workspace；npm
      --prefix sidecar run build && npm --prefix sidecar test；npx tsc --noEmit &&
      npx vitest run（client/web）；cargo run -p xtask -- gate。若 plugin 测试以
      AlreadyGranted 失败，清理陈旧 /tmp/astra-desktop-plugin-* scratch 目录
      （Windows pid 回收 flake）再重跑。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后 up +
      env；cargo test -p astra-app tests_live_harness::crypto_collected_review --
      --test-threads=1），随后 unset。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步 main；ledger
      清理。

## 自审笔记

- 规格覆盖：D1 的 app 方法 + 提升 trust 写入（T1）；D2 的命令 + web 审阅 UI + D3 的
  契约 + 活实证 + 电池 + PR（T2）。
- 风险：promote+trust 配对（promote 本身是接受的一半，SP4d-1 教训）；no-downgrade
  守卫（unverified 提升绝不降级既有 verified 决策）；reject 记录 rejected 使再暂存
  副本被拒；lib.rs/commands.rs 在上限（新子模块）；ipc.ts 在上限（wire 类型在
  crypto.ts）。无密钥穿过审阅面。
- 不评测/不提升 pipeline（面向模型不变）。CLI 密钥 verb + sidecar 虚拟工具是
  SP4d-1b。活实证教训：先 reset harness；若电池在 AlreadyGranted 上 flake，清理陈旧
  plugin scratch 目录。