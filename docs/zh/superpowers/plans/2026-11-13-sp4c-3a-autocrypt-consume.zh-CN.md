# SP4c-3a 计划：Autocrypt 消费

规格：docs/superpowers/specs/2026-11-13-sp4c-3a-autocrypt-consume-design.md。
分支 `sp4c-3a-autocrypt-consume` 自文档提交从 main 分出。每个任务：先红、
实现、电池、一个约定式提交。一个契约变更：crypto-receive.md 新增 Autocrypt
消费注记连同其 zh 镜像在同一提交，capabilities OpenPGP 行连同其 zh 镜像
移动，并落一个变更日志碎片。无引擎改动。仅豁免上限的加密 crate（无网络、
无 UI、无迁移）。

### 任务 1：传入解析器 + 接收钩子（D1，D2）

**文件：** crates/astra-crypto-core/src/mime.rs（一个小的 pub 头值访问器，
from_email 先例）；crates/astra-crypto-store/src/autocrypt.rs（发射器旁的
消费解析器：展开 + 读取 Autocrypt / Autocrypt-Gossip 值、拆分属性、提取
addr / prefer-encrypt / keydata、base64 解码、parse_first_cert、addr 绑定、
stage_collected_key_if_absent source='autocrypt'）+ autocrypt_consume_tests.rs；
crates/astra-crypto-store/src/receive.rs（outer 头上的明文 Autocrypt 钩子 +
unwrap_protected_headers 之后的 gossip 钩子）。

- [ ] 测试先行（发射→消费往返、addr≠From 丢弃、畸形 keydata 不暂存、gossip
      解析、重开去重、接收钩子暂存密钥）；随后解析器 + 钩子。提交
      `feat: consume inbound Autocrypt keys into the collected staging`。
- [ ] 加密 crate 豁免上限。无网络、无 UI、无迁移。失败即关闭的接收臂保留
      （暂存静默，绝不改变解密裁决）。既有接收测试保持绿。

### 任务 2：契约 + 活实证 + 收尾（D3）

**文件：** docs/crypto-receive.md（Autocrypt 消费注记）+ zh；
docs/capabilities.md（OpenPGP 行注记命名消费半）+ zh；
docs/changelog/unreleased/sp4c-3a-autocrypt-consume.md（Platforms: desktop，
Bump: minor）；crates/astra-app/src/tests_live_harness/
crypto_receive_autocrypt.rs（门控活实证：发送门发出 Autocrypt 头，接收方
打开，断言 collected_keys 行；gossip 臂；addr≠From 与畸形否定；撤回副本）
+ tests_live_harness.rs 注册。

- [ ] 文档 + zh + 行 + 碎片 + 门控活实证（离线跳过）。提交
      `docs: the Autocrypt consume contract`。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后
      up + env；cargo test -p astra-app
      tests_live_harness::crypto_receive_autocrypt -- --test-threads=1），
      随后 unset。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步
      main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的解析器 + D2 的钩子（T1）；D3 的契约 + 行 + 碎片 + 活实证
  + 电池 + PR（T2）。
- 风险：addr 绑定（Autocrypt addr 须与 From 一致，否则丢弃）；畸形 keydata
  不暂存（绝不 panic）；重开去重（(account, standard, fingerprint, source)
  键）；暂存静默且绝不改变解密裁决；gossip 头住在受保护头包装内；pub core
  助手尊重检测边界。
- 不评测/不提升 pipeline（面向模型不变）。无网络/UI/迁移。审查/提升 UI 是
  SP4d；CRL（3b）与 W9（3c）是独立切片。
- 活实证教训（来自 SP4c-2）：干净实证前先 reset harness；播下发送门可链的
  叶（直接由锚定根签名）。