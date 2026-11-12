# 加密接收：契约

**范围。** 经阅读管线解密传入的加密邮件并呈现安全状态。加密特性的接收半
（SP4c）；发送半（SP4b+ECS）另行交付。本子切片（SP4c-1）覆盖 OpenPGP
PGP/MIME 加密邮件。约束阅读管线与加密 store。引擎按设计对加密无知：它
运输字节，Astra 解释加密 framing（发送先例的镜像）。

## 路径

1. 阅读管线经既有的 `engine.message_source` 读取抓取原始 RFC5322 源
   （缓存优先、可离线）。无新引擎接缝：接收骑解码接缝已消费的字节交接。
2. 一个内置 `MessageDecoder`（加密解码器）调用接收编排器
   （`astra-crypto-store::receive_openpgp`），它检测并解包 PGP/MIME
   `multipart/encrypted` framing（协议 `application/pgp-encrypted`），解析
   账户的 OpenPGP 私钥，并直接经 `OpenpgpBackend::decrypt_with_signers`
   解密（提供 keystore 签名者与已收集证书，使内嵌签名对已知对等体验证）。
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

## 已知缺口，记录而非静默

- 本子切片仅 OpenPGP PGP/MIME 加密。内联 PGP armor 与 multipart/signed
  （分离签名）是后续；S/MIME 接收是 SP4c-2；Autocrypt 消费、CRL 抓取与
  W9 密码流程是 SP4c-3；静态加密草稿是 SP4c-4。
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
| 结果行（message_crypto_results 的第一个写入者） | ✅ | `receive.rs`、`results.rs` |
| 阅读集成：腿、banner、恢复主题、隐藏芯片 | ✅ | `astra-app/src/crypto_decode.rs`、`reading.rs` |
| 复合解码器（加密裁决优先、插件落穿） | ✅ | `client/desktop/src/decode.rs` |
| 失败即关闭：无密钥/失败/篡改绝不渲染密文 | ✅ | closed 臂 + 结构性接缝清除 |
| 活实证（发送加密、打开、解密、no-key 臂） | ✅ | `tests_live_harness/crypto_receive.rs` |
| S/MIME 接收、内联 armor、分离签名、静态草稿 | N/A | SP4c-2/3/4，上述缺口 |
| 密钥管理、token 注册表、原生安全 UI | N/A | SP4d |