# SP4c-4 计划：静态加密草稿

规格：docs/superpowers/specs/2026-11-13-sp4c-4-drafts-at-rest-design.md。分支
`sp4c-4-drafts-at-rest` 自规格提交从 main 分出。每个任务：先红、实现、电池、
一个约定式提交。一个契约变更：crypto-receive.md 新增静态草稿注记连同其 zh 镜
像在同一提交，capabilities 行连同其 zh 镜像移动，并落一个变更日志碎片。无引
擎改动。

### 任务 1：加密草稿镜像（D1）

**文件：** crates/astra-crypto-store/src/migrations/0005_encrypted_drafts.sql
（新；encrypted_drafts(account_id REFERENCES accounts, draft_key, body_enc
BLOB, updated_at, PRIMARY KEY(account_id, draft_key))）；新
crates/astra-crypto-store/src/drafts.rs（put_encrypted_draft /
get_encrypted_draft / delete_encrypted_draft 基于 vault.rs，AAD
astra:{account}:draft:{key}:1 镜像 keys.rs private_aad；不透明字节，store 绝不
命名 engine_api::Draft）；store.rs 迁移链 + 模块接线 + 测试。

- [ ] 测试先行（经 vault 的 put/get/delete 往返；AAD 绑定账户 + 草稿键，故跨
      账户 blob AEAD 失败；缺行是 Ok(None)；keyring 不可用是类型化
      CryptoVaultError::Keyring）；随后迁移 + store 方法。提交
      `feat: the encrypted draft mirror`。
- [ ] 加密 crate 豁免上限。主密钥留在 OS keyring（vault.rs），零化，绝不记
      录。既有 store 测试保持绿。

### 任务 2：保存/丢弃钩子 + 恢复缝 + 契约（D2，D3）

**文件：** 新 crates/astra-app/src/drafts/encrypt.rs（Draft <-> JSON，镜像驱
动；drafts.rs 接近上限）；save_draft_gated 写钩子（在 engine.put_draft 之前，
vault 出错失败即关闭拒绝，put_draft 失败则回滚）+ 丢弃/发送后删除钩子
（drafts.rs）；恢复读取缝（drafts/resume.rs：Ok(Some) -> vault 文字，Ok(None)
-> 引擎正文回退，Err -> 失败即关闭 banner）；docs/crypto-receive.md（静态草稿
注记）+ zh；docs/capabilities.md（草稿行）+ zh；
docs/changelog/unreleased/sp4c-4-drafts-at-rest.md（Platforms: desktop，Bump:
minor）；crates/astra-app/src/tests_live_harness/crypto_drafts_at_rest.rs（门
控活实证：武装测试主密钥，保存 -> 镜像行是密文，恢复 -> 文字，丢弃 -> 镜像消
失；撤回副本）+ tests_live_harness.rs 注册。

- [ ] 在钩子处确认：稳定草稿键是 Draft.message_id，恢复从引擎消息导出同一
      Message-ID；渲染后的 Draft 是载荷（Serialize + Deserialize 双向）。测试
      先行（保存钩子在 put_draft 之前写镜像并在 vault 出错时拒绝；丢弃删除它；
      恢复优先镜像、Ok(None) 回退、Err 失败即关闭；离线跳过的门控活实证）；随
      后钩子 + 缝 + 文档。提交
      `feat: the drafts-at-rest vault seam and contract`。
- [ ] astra-app/client/desktop/web 不豁免：新 core 逻辑在新子模块，绝不
      drafts.rs/reading.rs/lib.rs/commands.rs 增长；ipc.ts 不加新 wire 类型
      （镜像在 core 侧，恢复快照形状不变）。注释现在时；无 em/en 破折号；英式
      英语。docs/zh 镜像同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后
      up + env；cargo test -p astra-app
      tests_live_harness::crypto_drafts_at_rest -- --test-threads=1），随后
      unset。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步 main；
      ledger 清理。

## 自审笔记

- 规格覆盖：D1 的镜像（T1）；D2 的保存/丢弃钩子 + D3 的恢复缝 + 契约 + 活实证
  + 电池 + PR（T2）。
- 风险：引擎残留仍是明文（记录缺口，交引擎前加密是后续）；keyring 不可用时保
  存失败即关闭拒绝；恢复处缺席与解不开的区分（Ok(None) 回退，Err 绝不）；草稿
  键须在保存/恢复/丢弃间一致；新 core 逻辑在子模块（上限）。vault 主密钥绝不
  离开 keyring/内存。
- 不评测/不提升 pipeline（面向模型不变）。活实证教训：先 reset harness；武装
  测试主密钥（testing feature）使 vault 无需真实 OS keyring 即可工作。