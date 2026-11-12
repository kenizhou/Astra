# SP4c-1 计划：经阅读管线的 OpenPGP 接收

规格：docs/superpowers/specs/2026-11-13-sp4c-openpgp-receive-design.md。
分支 `sp4c-openpgp-receive` 自文档提交从 main 分出。每个任务：先红、实现、
电池、一个约定式提交。一个契约变更：一个新的 docs/crypto-receive.md
（+ zh），capabilities OpenPGP 行连同其 zh 镜像移动，并落一个变更日志
碎片。无引擎改动。

### 任务 1：接收编排器（D1）

**文件：** crates/astra-crypto-store/src/receive.rs（新， beside wrap.rs，
豁免上限：检测+解包 PGP/MIME multipart/encrypted，经 SqliteKeyStore 解析
账户私钥，直接以 keystore + 已收集签名者调用 decrypt_with_signers，把
DecryptOutcome weak/unverifiable 映射为签名状态，写 message_crypto_results
带 Rust 侧 PK linkage，恢复受保护主题；一个中性无引擎结果类型）+
receive_tests.rs；一个新的解包助手（pgp_mime::wrap_encrypted 的逆，复用
mime::split_message），若归属 astra-crypto-core 则放那里，否则放 store crate。

- [ ] 测试先行（解包、密钥解析、状态映射、结果行 linkage、no-key/未知
      签名者/被篡改的失败即关闭）；随后编排器。提交
      `feat: the OpenPGP receive orchestrator`。
- [ ] store crate 豁免上限；astra-crypto-core 豁免上限。无引擎改动。失败
      即关闭：无密钥/失败绝不产出明文。

### 任务 2：阅读集成（D2）

**文件：** crates/astra-app/src/crypto_decode.rs（新兄弟模块，reading.rs
接近上限：一个调用编排器的内置 MessageDecoder，映射为 DecodeOutcome
html/plain/banner，解密腿经 render_html_leg，失败即关闭 banner 状态，隐藏
密文伪附件）+ crypto_decode_tests.rs（reading_decode_tests.rs 仿件模式）；
client/desktop/src/boot.rs + 一个复合解码器（内置加密与 PluginDecoder 并列，
对识别的邮件加密裁决优先）；crates/astra-app/src/tests_live_harness/
crypto_receive.rs（门控活实证：播下 astra2 密钥，发送加密 astra1->astra2，
打开行，断言快照文字 + 已验证 banner + 恢复主题 + 结果行，no-key 否定臂，
撤回副本）。

- [ ] 测试先行（离线集成红 + 离线跳过的门控活实证）；随后适配器 + 复合 +
      实证。提交 `feat: OpenPGP receive through the reading pipeline`。
- [ ] 行数警戒：astra-app/client 不豁免上限——新模块，而非增长 reading.rs
      （462）或 ipc.ts（在上限；无需新 wire 类型，banner 骑既有
      SecurityBannerView）。活实证无 ASTRA_LIVE_HARNESS 时跳过（由控制者
      武装）。

### 任务 3：契约 + 收尾（D3）

**文件：** docs/crypto-receive.md（接收路径、失败即关闭状态、检测边界、
结果行 linkage、受保护主题恢复）+ docs/zh/crypto-receive.zh-CN.md；
docs/capabilities.md（OpenPGP 行的接收半落地；注记命名剩余）+ zh；
docs/changelog/unreleased/sp4c-openpgp-receive.md（Platforms: desktop，
Bump: minor）。

- [ ] 文档 + zh + 行 + 碎片。提交 `docs: the OpenPGP receive contract`。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。
- [ ] 控制者武装活 harness 实证（scripts/dev/harness.sh up + env；cargo
      test -p astra-app tests_live_harness::crypto_receive --
      --test-threads=1），随后 unset。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步
      main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的编排器（T1）；D2 的阅读集成 + 活实证（T2）；D3 的契约 +
  行 + 碎片 + 电池 + 武证实证 + PR（T3）。
- 风险：失败即关闭是承重的（无密钥/失败/篡改绝不渲染密文；插件失败即开
  不适用于加密裁决）；直接 decrypt_with_signers（trait 丢弃 weak/
  unverifiable 通道）；复合解码器优先级（加密 vs 插件）；受保护主题恢复；
  结果行 PK linkage；消毒管线在解密腿上重跑；无引擎改动；500 上限咬
  astra-app/client（新模块）。
- 不评测/不提升 pipeline（面向模型面不变）。推迟：内联 armor、分离签名、
  S/MIME（SP4c-2）、Autocrypt/CRL/W9（SP4c-3）、静态草稿（SP4c-4）、密钥
  管理 + token 注册表 + UI（SP4d）。