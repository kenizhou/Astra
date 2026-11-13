# 加密接收：契约

**范围。** 经阅读管线解密传入的加密邮件并呈现安全状态。加密特性的接收半
（SP4c）；发送半（SP4b+ECS）另行交付。SP4c-1 覆盖 OpenPGP PGP/MIME 加密
邮件；SP4c-2 覆盖 S/MIME enveloped-data（含嵌套先签后加密形态）。约束
阅读管线与加密 store。引擎按设计对加密无知：它运输字节，Astra 解释加密
framing（发送先例的镜像）。

## 路径

1. 阅读管线经既有的 `engine.message_source` 读取抓取原始 RFC5322 源
   （缓存优先、可离线）。无新引擎接缝：接收骑解码接缝已消费的字节交接。
2. 一个内置 `MessageDecoder`（加密解码器）调用统一接收分发器
   （`astra-crypto-store::receive`），它一次性嗅探 framing 并路由：PGP/MIME
   `multipart/encrypted`（协议 `application/pgp-encrypted`）到
   `receive_openpgp`，S/MIME `application/pkcs7-mime; smime-type=enveloped-data`
   到 `receive_smime`。OpenPGP 路径解析账户私钥，并直接经
   `OpenpgpBackend::decrypt_with_signers` 解密（提供 keystore 签名者与已收集
   证书，使内嵌签名对已知对等体验证）。
3. 编排器写 `message_crypto_results`（它的第一个写入者）：加密种类、解密
   状态（ok / no-key / failed）、签名状态、签名者指纹与邮箱，按
   （account, message）键控，linkage 在 Rust 侧强制。
4. 解密后的腿重跑同一消毒管线；恢复的受保护主题替换外层占位；安全 banner
   陈述裁决；密文伪附件芯片被隐藏。

## 失败即关闭状态

加密裁决失败即关闭（插件解码器的失败即开不适用于它）。一个被识别但无法
解密的邮件绝不渲染密文或外层 framing：

- **未加密**（非 PGP/MIME framing）：渲染保持不动，插件解码器仍可服务它；
  无结果行。
- **无密钥**（账户没有能解密它的私钥）：一个「无法解密」banner，无明文，
  一个 `no-key` 结果行。
- **失败**（被篡改的密文、坏签名、错误密钥，或被识别但密文无法提取的
  framing）：一个「无法解密」banner，无明文，一个 `failed` 结果行。
- **已解密**：腿与签名裁决（已验证，或未知签名者为未验证）、恢复的主题、
  一个 `ok` 行。

复合解码器先咨询加密解码器；它识别的任何裁决优先，插件绝不见到该邮件。
仅未加密的回答落到插件。阅读接缝在每个加密裁决上结构性地清除存储自己的
腿，故外层 framing 绝不与其并列渲染。

## 检测边界

协议解析（IMAP 抓取、blob 缓存、用于显示的 MIME 结构）是引擎的。加密
信封的检测与解释是 Astra 的加密层，恰如发送门的 MIME framing 住在 Astra。
引擎树保持不动。

## S/MIME 路径（SP4c-2）

`receive_smime` 解包 enveloped-data，解析账户的 S/MIME 私叶（p12 导入的
证书；token 持有的密钥路由到 W6 token 路径），并经 `SmimeBackend::decrypt`
解密。先签后加密邮件的明文本身是一个不透明 CMS SignedData；提取其 eContent
并经 `verify_with_context` 验证，输入从 store 解析：From 地址（From↔SAN
绑定）、信任锚、中间证书（存储的与内嵌证书集合并、按指纹去重）、CRL 缓存、
签名者的最新信任决策。发现的签名者叶被收集（绝不自动信任）。

签名状态阶梯精确：

- 链无效（缺失或不受信锚、过期证书、吊销证书）→ **Invalid**。
- 链有效但 From 地址与签名者证书 SAN 不匹配 → **Mismatch**。
- 链有效、身份匹配、且签名者信任显示为已验证（Verified 或 Personal）→
  **ValidVerified**。
- 否则（链到受信锚，签名者未被明确接受）→ **ValidUnverified**。

结果行携带 `standard` 判别字段与链字段（`chain_valid`、`revocation_state`、
细粒度 `failure_reason` 与 `revocation_reason`），由一个向后兼容的表重建
迁移添加。banner 陈述裁决（吊销情形被区分）；no-key / failed 臂恰如
OpenPGP 失败即关闭。p12 中间证书以 `key_type='intermediate'` 存储，绝不经
keystore 的证书路径（那会把它们泄入锚集）。

## Autocrypt 消费路径（SP4c-3a）

`receive_openpgp` 将传入的 Autocrypt 密钥消费进已收集暂存区，静默且在裁决
之后。两种头形态喂给它：

- 外层头块上的明文 `Autocrypt:` 头携带发送者密钥。消费展开值、按 `;` 拆分
  属性、提取 `addr`、`prefer-encrypt` 与 `keydata`、base64 解码 keydata
  （二进制最小导出）并解析第一个证书取其指纹。Autocrypt Level 1 将 `addr`
  绑定到 From 地址（忽略大小写）；不匹配则丢弃该头。
- `Autocrypt-Gossip:` 头骑在加密部分内部，位于解密后内层实体的根头块
  （发送门发出的受保护头包装器）。gossip 的 `addr` 是收件人对等体，无 From
  绑定；每个对等体密钥入暂存。

暂存是静默的：`stage_collected_key_if_absent` 写一行 `source='autocrypt'` 的
`collected_keys`，按（account, standard, fingerprint, source）去重，故重新
打开的邮件绝不重复。畸形 keydata 不入暂存且绝不 panic。消费在裁决计算并
持久化之后运行，故绝不改变本邮件的解密或签名结果；失败即关闭臂在其之前
返回，故失败的接收不入暂存。

收益是即时的：`receive_openpgp` 已将已收集证书提供给 `decrypt_with_signers`
与 `resolve_signer`，故入暂存的密钥改善下一封邮件的内嵌签名验证。已收集
密钥的审阅/提升面是 SP4d；此处暂存静默。

## 已知缺口，记录而非静默

- 按标准仅加密接收：OpenPGP PGP/MIME（SP4c-1）与 S/MIME enveloped-data
  （SP4c-2）。内联 PGP armor 与分离签名形态（PGP multipart/signed、S/MIME
  multipart/signed 与不透明 signed-data）是后续；Autocrypt 消费在 SP4c-3a
  落地（上述接收半）；CRL 抓取（SP4c-3b）与 W9 密码流程（SP4c-3c）仍在；
  静态加密草稿是 SP4c-4。
- Autocrypt 暂存静默：已收集密钥的审阅/提升 UI 是 SP4d。发送门在密文内发出
  `Autocrypt-Gossip`，但尚无明文自身 `Autocrypt` 头；当传入邮件携带明文头
  （对等客户端的）时消费读取它，自身头的发出是后续。gossip 消费读取发送门
  发出的受保护头包装器；超出该包装器的第三方 gossip 放置是后续。
- S/MIME 吊销仅缓存：CRL 缓存在 SP4c-3 抓取器落地前为空，故实践中吊销读作
  unchecked（有 CRL 时吊销证书仍使链硬失败）。链的 Stale 吊销状态在抓取器
  落地前持久化为 unchecked。
- 尚无密钥管理面（SP4d）：实证以编程方式播下接收者密钥。一个最小导入
  路径是使接收在测试之外可用的承重后续。
- 仅携带 Issuer KeyID（无 IssuerFingerprint 子包）的遗留签名可验证却报告
  无签名者指纹，故标为 NotSigned 而非 ValidVerified。一个退化，绝非假的
  「已验证」；现代客户端发出 IssuerFingerprint。
- CLI `messages read` 仍直接读引擎正文（加密邮件在那里答空）；接收路径
  是 desktop 阅读管线。CLI 安全字段是命名的后续。
- token 注册表（cryptoki、openpgp-card）与原生安全 UI 是 SP4d。

## 矩阵

| 规则 | 状态 | 位置 |
|---|---|---|
| OpenPGP PGP/MIME 接收：检测、解包、解密、验证状态 | ✅ | `astra-crypto-store/src/receive.rs` |
| S/MIME enveloped 接收：解包、解密、链验证、状态阶梯 | ✅ | `astra-crypto-store/src/receive_smime.rs` |
| 统一接收分发器（PGP/MIME vs S/MIME 路由） | ✅ | `receive.rs` |
| 结果行（message_crypto_results 的第一个写入者） | ✅ | `receive.rs`、`receive_smime.rs`、`results.rs` |
| 链字段 schema（standard、revocation 'stale'、表重建） | ✅ | `migrations/0004_smime_receive.sql` |
| 阅读集成：腿、banner、恢复主题、隐藏芯片 | ✅ | `astra-app/src/crypto_decode.rs`、`reading.rs` |
| 复合解码器（加密裁决优先、插件落穿） | ✅ | `client/desktop/src/decode.rs` |
| 失败即关闭：无密钥/失败/篡改绝不渲染密文 | ✅ | closed 臂 + 结构性接缝清除 |
| Autocrypt 消费：传入解析、addr 绑定、静默暂存 | ✅ | `astra-crypto-store/src/autocrypt.rs`、`receive.rs` |
| 活实证（发送加密、打开、解密、否定臂） | ✅ | `tests_live_harness/crypto_receive.rs`、`crypto_receive_smime.rs`、`crypto_receive_autocrypt.rs` |
| 内联 armor、分离签名、CRL 抓取、静态草稿 | N/A | SP4c-3/4，上述缺口 |
| 密钥管理、token 注册表、原生安全 UI | N/A | SP4d |