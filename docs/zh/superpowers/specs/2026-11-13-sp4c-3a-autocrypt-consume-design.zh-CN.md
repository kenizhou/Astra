# SP4c-3a 设计：Autocrypt 消费

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** SP4c-3 的第一条腿：Autocrypt 的传入半。发送门已发出
`Autocrypt:` 与 `Autocrypt-Gossip:` 头；接收路径已把暂存的已收集密钥作为
额外签名者 blob 消费。缺的是消费者：解析传入的头并暂存密钥。本子切片是
SP4c-3 中最有界的（仅豁免上限的加密 crate，无网络、无 UI、无迁移）；CRL
抓取（SP4c-3b）与 W9 密码流程（SP4c-3c）作为各自切片跟进。无引擎改动；
不评测/不提升 pipeline（面向模型面不变）。

## D1：传入解析器

一个位于 `astra-crypto-store/src/autocrypt.rs` 发射器旁的消费模块（模块
文档已命名该缺口）：

- 展开并读取 `Autocrypt` / `Autocrypt-Gossip` 头的值。按 `;` 拆分属性，
  提取 `addr`、`prefer-encrypt`（仅 Autocrypt）与 `keydata`；base64 解码
  `keydata`（发射器用二进制 `minimal_export` blob 的 base64-STANDARD）。
- 对 blob 调用 `parse_first_cert`（既有传入原语）→ 指纹。畸形 keydata
  不暂存任何东西。
- 绑定地址：Autocrypt Level 1 要求 `Autocrypt` 的 addr 与 From 地址一致，
  否则丢弃；gossip 的 addr 是接收方对等体。
- 经 `stage_collected_key_if_absent` 以 `source='autocrypt'` 暂存（去重键
  为 (account, standard, fingerprint, source)，故重开的邮件不重复；
  `promote_origin` 已映射 'autocrypt'）。

一个小的 pub core 助手暴露头的值（store 不能调用 pub(crate) 的
`mime::header_value`；SP4c-2 的 `from_email` 先例）。属性/keydata 解析落在
autocrypt.rs 发射器旁。加密信封解释留在 Astra 的加密层（检测边界）。

## D2：接收钩子

- 明文 `Autocrypt:` 头：`receive_openpgp` 已把源拆为 (outer, entity) 并丢弃
  outer；消费读取 outer 头并暂存发送者密钥（addr 须与 From 一致）。
- `Autocrypt-Gossip:` 头住在加密部分的受保护头包装内；
  `unwrap_protected_headers` 之后，消费从解密后的内层头块读取并暂存对等体
  密钥。
- 暂存是静默的（无 UI；审查/提升面是 SP4d）。收益是即时的：
  `receive_openpgp` 已把已收集证书提供给 `decrypt_with_signers` 与
  `resolve_signer`，故暂存的密钥改进下一封邮件的内嵌签名验证。

## D3：契约

docs/crypto-receive.md 新增 Autocrypt 消费注记（传入解析、addr 绑定、静默
暂存、即时验证收益）并镜像到 zh。capabilities OpenPGP 行的注记命名消费半
落地。一个变更日志碎片落地。porting-backlog 不受影响（这是 C 簇部分行，
而非 kylins 增补）。

## 已知缺口，记录而非静默

- 已收集密钥的审查/提升 UI 是 SP4d；此处暂存静默。
- Gossip 消费读取发送门发出的受保护头包装；超出该包装的第三方 gossip
  放置是后续。
- CRL 抓取（SP4c-3b）与 W9 密码流程（SP4c-3c）是独立切片；W9 发送侧本身
  未接线（3c 的协调注记）。

## 测试先行

每个行为都带着它的红落地。解析器测试（发射→消费往返：发出一个 Autocrypt
头、消费它、暂存密钥匹配；addr≠From 丢弃；畸形 keydata 不暂存；gossip
解析；重开去重）。接收钩子测试（明文 Autocrypt 头暂存发送者密钥；密文内的
gossip 头暂存对等体密钥；暂存密钥随后验证一个后续内嵌签名）。一个门控的
活 harness 实证：发送门发出 Autocrypt 头（send_autocrypt_headers 开 +
默认密钥），接收方打开，断言一个 collected_keys 行（source='autocrypt'、
peer_email=发送者、指纹）；gossip 臂（加密+签名给 ≥2 接收者）暂存 gossip
密钥；addr≠From 与畸形否定不暂存；撤回副本。无活模型证明；离线电池即
证明。