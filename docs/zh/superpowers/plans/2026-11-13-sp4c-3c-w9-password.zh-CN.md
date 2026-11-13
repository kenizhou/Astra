# SP4c-3c 计划：W9 密码接收流程

规格：docs/superpowers/specs/2026-11-13-sp4c-3c-w9-password-design.md。分支
`sp4c-3c-w9-password` 自文档提交从 main 分出。每个任务：先红、实现、电池、
一个约定式提交。一个契约变更：crypto-receive.md 新增 W9 密码流程注记连同其
zh 镜像在同一提交，capabilities OpenPGP 行连同其 zh 镜像移动，并落一个变更
日志碎片。无引擎改动。

### 任务 1：需要密码的裁决 + 密码编排器（D1）

**文件：** crates/astra-crypto-store/src/receive.rs（增量
ReceiveState::NeedsPassword 臂；解包后 has_skesk 检测密码邮件，当无 PKESK
密钥匹配时裁决为 NeedsPassword；receive_openpgp_password(store, account,
message, source, passphrase) 入口调用 decrypt_symmetric，用相同的
known_signers/extra_signer_blobs 解析，成功写 ok 行 + 恢复主题，
WrongPassphrase 返回类型化重新提示信号）+ 测试。需要密码状态瞬态（非持久化
行；decrypt_state CHECK 不变）。

- [ ] 测试先行（SKESK 邮件 → NeedsPassword；正确口令 → ok + 文字 + 恢复
      主题；WrongPassphrase → 重新提示，无明文；内嵌签名验证 / 未知签名者
      退化）；随后裁决 + 密码入口。提交
      `feat: the OpenPGP password receive orchestrator`。
- [ ] 加密 crate 豁免上限。口令仅存内存，绝不记录/持久化。失败即关闭臂保留。
      既有接收测试保持绿。

### 任务 2：解码器臂 + 密码提示 + 契约（D2，D3）

**文件：** crates/astra-app/src/crypto_decode.rs（NeedsPassword banner 臂）；
一个新密码对话框组件（client/web，独立文件，useDialogFocus 惯用法）；一个新
desktop 命令进程内携带口令（crypto_wrapper 先例）+ 对新的
engine.message_source 重跑密码编排器 + 刷新阅读快照（新阅读重入口模块，
reading.rs 接近上限）；新 wire 类型在所属特性服务（绝不在 ipc.ts）；
docs/crypto-receive.md（W9 注记）+ zh；docs/capabilities.md（OpenPGP 行）+
zh；docs/changelog/unreleased/sp4c-3c-w9-password.md（Platforms: desktop，
Bump: minor）；crates/astra-app/src/tests_live_harness/
crypto_receive_password.rs（门控活实证：经 encrypt_symmetric 手工构造 SKESK
邮件、送达、打开 → 需要密码；提交口令 → 文字 + 恢复主题；错误口令 → 重新
提示；撤回副本）+ tests_live_harness.rs 注册。

- [ ] 测试先行（解码器 banner、命令重入口、离线跳过的门控活实证）；随后
      解码器臂 + 对话框 + 命令 + 文档。提交
      `feat: the W9 password prompt and receive contract`。
- [ ] astra-app/client/desktop/web 不豁免上限：密码对话框独立组件、阅读重
      入口新模块、wire 类型在特性服务（绝不在 ipc.ts）。口令留在进程内，绝不
      记录/持久化。注释现在时；无 em/en 破折号；英式英语。docs/zh 镜像同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后
      up + env；cargo test -p astra-app
      tests_live_harness::crypto_receive_password -- --test-threads=1），
      随后 unset。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步
      main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的裁决 + 密码编排器（T1）；D2 的解码器臂 + 提示 + D3 的契约
  + 活实证 + 电池 + PR（T2）。
- 风险：口令绝不记录/持久化（仅内存）；需要密码状态瞬态（decrypt_state
  CHECK 不变）；失败即关闭臂保留；WrongPassphrase 重新提示（无明文泄漏）；
  wire 类型在特性服务（ipc.ts 在上限）；阅读重入口新模块（reading.rs 接近
  上限）；密码对话框独立组件。
- 不评测/不提升 pipeline（面向模型不变）。W9 发送本身未接线（活实证手工构造
  SKESK 邮件）；接线 W9 发送是协调后续。活实证教训：先 reset harness。