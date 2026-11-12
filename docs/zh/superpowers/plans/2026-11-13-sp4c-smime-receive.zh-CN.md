# SP4c-2 计划：经阅读管线的 S/MIME 接收

规格：docs/superpowers/specs/2026-11-13-sp4c-smime-receive-design.md。分支
`sp4c-smime-receive` 自文档提交从 main 分出。每个任务：先红、实现、电池、
一个约定式提交。一个契约变更：crypto-receive.md 新增 S/MIME 节连同其 zh
镜像，capabilities S/MIME 行连同其 zh 镜像移动，并落一个变更日志碎片。无
引擎改动。范围限于加密 S/MIME（仅签名推迟）。

### 任务 1：助手、编排器、schema（D1，D2，D3 schema）

**文件：** crates/astra-crypto-core/src/smime_mime.rs（新，镜像 pgp_mime：
is_smime_enveloped / unwrap_enveloped，application/pkcs7-mime +
x-pkcs7-mime smime-type=enveloped-data，base64 到 DER，被识别但无法提取的
失败即关闭臂）+ mime.rs 中一个 From 提取助手（pub）；
crates/astra-crypto-smime/src/（一个 pub extract_signed_content eContent
提取器 + 一个 SignedData 嗅探，CMS 解析留在此）；
crates/astra-crypto-store/src/receive_smime.rs（新，镜像 receive.rs：解析
S/MIME 私叶、解密、以 store 输入 [from_email、锚、合并去重的中间证书、CRL
缓存、签名者信任] 调用 verify_with_context、状态阶梯、收集签名者叶、写带
链字段的结果行、恢复主题、失败即关闭）+ receive_smime_tests.rs；receive.rs
中一个统一 receive 分发器（嗅探 PGP/MIME vs S/MIME，路由；receive_openpgp
不动）；ReceiveOutcome/ResultVerdict/write_result/results.rs 为链字段 +
标准增量扩展；一个迁移（standard 列 + revocation_state CHECK 中的 'stale'，
表重建）。

- [ ] 测试先行（解包往返 + 失败即关闭臂、From 助手、eContent 提取、编排器
      状态阶梯 + 结果行链字段 + 失败即关闭底线）；随后助手、编排器、分发器、
      schema。提交 `feat: the S/MIME receive orchestrator`。
- [ ] 加密 crate 豁免上限。无引擎改动。状态阶梯精确：不受信/缺失锚或吊销
      -> Invalid；From/SAN 不匹配 -> Mismatch；链+身份+未信任 ->
      ValidUnverified；+Verified/Personal -> ValidVerified。p12 中间证书以
      key_type='intermediate' 存储，绝不经 keystore.put。

### 任务 2：阅读集成 + 活实证（D3 集成）

**文件：** crates/astra-app/src/crypto_decode.rs（切换到统一 receive；
S/MIME banner 臂：加密+已验证/未验证/不匹配/无效，关闭臂复用；如需隐藏
smime.p7s）+ crypto_decode_tests.rs（S/MIME banner 红）；
crates/astra-app/src/tests_live_harness/crypto_receive_smime.rs（门控活
实证：播下 CA 链 [根锚 -> 中间 -> 发送者叶] + 接收者 p12，经引擎接缝发送
S/MIME 加密+签名，打开行，断言文字 + 链已验证 banner + 恢复主题 + 结果行
[chain_valid、revocation_state、签名者 fp] + 无密文；no-key / 不受信锚
Invalid / From-SAN Mismatch 否定臂；撤回副本）；astra-app Cargo.toml dev-dep
astra-crypto-smime features=["testing"]；tests_live_harness.rs 注册。

- [ ] 测试先行（离线 banner 红 + 离线跳过的门控活实证）；随后解码器分发 +
      banner + 实证。提交 `feat: S/MIME receive through the reading pipeline`。
- [ ] 行数警戒：astra-app/client 不豁免上限（crypto_decode.rs 保持 <500；
      活实证 <500）。阅读接缝的结构性失败即关闭清除（SP4c-1）原样适用；
      复合解码器不变。活实证无 ASTRA_LIVE_HARNESS 时跳过（由控制者武装）。

### 任务 3：契约 + 收尾（D4）

**文件：** docs/crypto-receive.md（S/MIME 节：framing、验证输入解析、状态
阶梯、schema 扩展）+ zh；docs/capabilities.md（S/MIME 行的接收半落地；注记
命名剩余）+ zh；docs/changelog/unreleased/sp4c-smime-receive.md
（Platforms: desktop，Bump: minor）。

- [ ] 文档 + zh + 行 + 碎片。提交 `docs: the S/MIME receive contract`。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。
- [ ] 控制者武装活 harness 实证（scripts/dev/harness.sh up + env；cargo
      test -p astra-app tests_live_harness::crypto_receive_smime --
      --test-threads=1），随后 unset。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步
      main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的解包 + D2 的编排器 + D3 的 schema（T1）；D3 的集成 + 活
  实证（T2）；D4 的契约 + 行 + 碎片 + 电池 + 武证实证 + PR（T3）。
- 风险：状态阶梯精确（Invalid vs ValidUnverified vs Mismatch vs
  ValidVerified）；失败即关闭（无密钥/失败/篡改绝不渲染密文）；嵌套 eContent
  提取；From/SAN 绑定；schema CHECK 重建；p12 中间证书绝不经 put（信任
  越权）；astra-app/client 的 500 上限（新模块）；无引擎改动。
- 不评测/不提升 pipeline（面向模型不变）。推迟：仅签名 S/MIME、CRL 抓取
  （SP4c-3）、密钥管理 + 签名详情 UI（SP4d）。