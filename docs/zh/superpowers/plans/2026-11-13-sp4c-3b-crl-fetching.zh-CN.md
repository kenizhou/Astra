# SP4c-3b 计划：CRL 抓取

规格：docs/superpowers/specs/2026-11-13-sp4c-3b-crl-fetching-design.md。分支
`sp4c-3b-crl-fetching` 自文档提交从 main 分出。每个任务：先红、实现、电池、
一个约定式提交。一个契约变更：crypto-receive.md 新增 CRL 抓取注记连同其 zh
镜像在同一提交，capabilities S/MIME 行连同其 zh 镜像移动，并落一个变更日志
碎片。无引擎改动。

### 任务 1：DP 提取 + 抓取器端口 + 接收钩子（D1，D2）

**文件：** crates/astra-crypto-smime/src/cert.rs（cert_summary_of_der 旁的
CRL 分发点 URL 提取；x509-parser crl_distribution_points → GeneralName::URI；
仅 http(s)）+ 测试；crates/astra-crypto-store/src/crl.rs（CrlFetcher 端口 +
抓取或缓存逻辑：新鲜缓存不抓取直接用、stale/缺失触发抓取 + upsert、抓取失败
退化为 unchecked）+ 假抓取器测试；crates/astra-crypto-store/src/
receive_smime.rs（verify_with_context 之前的钩子：从叶 + 中间证书提取 DP
URL、抓取或缓存、list_crls 供给链）；client/desktop/src/（宿主 CrlFetcher
实现：reqwest 有界 GET、短超时、大小上限、仅 http(s)）+ 接线。

- [ ] 测试先行（DP 提取、假抓取器的抓取或缓存、抓取失败的失败即软、接收钩子
      revoked/unchecked）；随后提取 + 端口 + 钩子 + 宿主实现。提交
      `feat: fetch CRLs for the S/MIME receive revocation check`。
- [ ] 加密 crate 豁免上限；client/desktop 不豁免（宿主实现保持内聚）。抓取
      失败即软（绝不阻塞打开）、有界（超时 + 大小上限）、仅 http(s)。store
      绝不依赖 astra-app。既有接收测试保持绿。

### 任务 2：契约 + 活实证 + 收尾（D3）

**文件：** docs/crypto-receive.md（CRL 抓取注记；更新"缓存空、吊销
unchecked"缺口）+ zh；docs/capabilities.md（S/MIME 行命名吊销检查）+ zh；
docs/changelog/unreleased/sp4c-3b-crl-fetching.md（Platforms: desktop，
Bump: minor）；crates/astra-crypto-smime/src/testing.rs（一个 CRL 构造器；
CA 构造器已带 cRLSign）；crates/astra-app/src/tests_live_harness/
crypto_receive_crl.rs（门控活实证：本地 HTTP 测试服务器服务手工 CRL；播下
CA 链、吊销发送者叶序列号、打开邮件，断言 revocation_state=revoked +
chain_valid=false + 吊销 banner；抓取失败否定 → unchecked；撤回副本）+
tests_live_harness.rs 注册。

- [ ] 文档 + zh + 行 + 碎片 + CRL 构造器 + 门控活实证（离线跳过）。提交
      `docs: the CRL fetching contract`。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后
      up + env；cargo test -p astra-app tests_live_harness::crypto_receive_crl
      -- --test-threads=1），随后 unset。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步
      main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的提取 + D2 的端口/钩子（T1）；D3 的契约 + 行 + 碎片 + CRL
  构造器 + 活实证 + 电池 + PR（T2）。
- 风险：DP URL 不可信（仅 http(s)、超时、大小上限）；抓取失败即软（绝不阻塞
  打开）；伪造 CRL  failing 链的 CRL 签名检查；store 绝不依赖 astra-app
  （端口）；链按签名时刻验证；隐私考量已记录。
- 不评测/不提升 pipeline（面向模型不变）。W9 密码流程（3c）是独立切片。
  活实证教训：干净实证前先 reset harness。