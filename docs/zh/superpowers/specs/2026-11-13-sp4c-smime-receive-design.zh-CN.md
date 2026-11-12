# SP4c-2 设计：经阅读管线的 S/MIME 接收

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** 加密接收半（SP4c）的第二个子切片：经阅读管线解密并验证传入的
S/MIME 加密邮件，镜像 SP4c-1（OpenPGP 接收，已交付）。范围限于加密 S/MIME
（`application/pkcs7-mime; smime-type=enveloped-data`，含发送门发出的嵌套
先签后加密形态）；仅签名形态（multipart/signed 分离、不透明 signed-data）
与其他分离签名工作一并推迟，恰如 SP4c-1 推迟 PGP multipart/signed。S/MIME
验证基础（`verify_with_context`、链验证、证书/信任/CRL store 查询）已存在
且经测试；本切片是 framing 解包、验证输入解析（后端文档指派的「G5 编排
器」）与阅读集成。无引擎改动：接收骑 `engine.message_source`；加密 framing
留在 Astra 层。

## D1：framing 解包与正文提取

astra-crypto-core 中一个新的 `smime_mime` 模块（镜像 `pgp_mime`，pub 以便
store 编排器调用）：检测并解包 `application/pkcs7-mime`（及
`application/x-pkcs7-mime`）且 `smime-type=enveloped-data`，base64 解码正文
为 EnvelopedData DER。被识别但无法提取的 framing 失败即关闭（SP4c-1
`is_pgp_encrypted` 先例）。core 中一个 `From` 提取助手（store 今天读不到
`header_value`，它是 pub(crate)）。

`SmimeBackend::decrypt` 之后，先签后加密邮件的明文本身是一个不透明 CMS
SignedData；astra-crypto-smime 中一个新的 pub `extract_signed_content` 助手
恢复内层正文字节（eContent）并做一个 SignedData 嗅探，使编排器恢复主题与
腿。CMS DER 解析留在 smime crate，绝不在 store。

## D2：编排器（astra-crypto-store）

一个镜像 `receive.rs` 的兄弟 `receive_smime.rs`，以及一个薄的统一
`receive(store, account, message, source)` 入口，一次性嗅探 framing
（PGP/MIME vs S/MIME enveloped）并路由；`CryptoDecoder` 调用 `receive`。
一个共享 `ReceiveOutcome`，增量扩展链字段（`chain_valid`、
`revocation_state`、`failure_reason`、`revocation_reason`）与一个标准判别
字段。`receive_openpgp` 保持不动。

`receive_smime`：解析账户的 S/MIME 私钥（p12 导入的叶，`standard="smime"`、
`key_type='cert'`、`has_private`，默认加密优先；候选循环把「无匹配收件人」
当作试下一个，其他错误失败即关闭）；经 `SmimeBackend::decrypt` 解密；对
嵌套签名形态，经 `verify_with_context` 验证，输入从 store 解析：

- `from_email` 来自新的 core 助手（From↔SAN 绑定）。
- `trust_anchor_ders` 来自 `list_trust_anchor_certs`。
- `intermediate_ders` 来自 `list_intermediate_certs` 与 SignedData 证书集的
  `extract_intermediates` 合并，按指纹去重。
- `crls` 来自 CRL 缓存（仅缓存，无抓取器；SP4c-3）；如需一个小的列全部
  助手，否则为空（吊销未检查）。
- `signer_trust` 来自 `get_latest_trust_decision`（自己的密钥为 Personal）。

状态阶梯（精确）：链无效（缺失/不受信锚、过期、吊销）→ Invalid；链有效但
From↔SAN 不匹配 → Mismatch；链有效 + 身份匹配 + 签名者信任显示为已验证
（Verified|Personal）→ ValidVerified；否则 → ValidUnverified。经
`insert_harvested_cert` 收集发现的签名者叶（绝不自动信任）。写
`message_crypto_results` 并填充 `chain_valid` 与 `revocation_state`；恢复
受保护主题。失败即关闭：无密钥/解密失败/篡改绝不产出明文或密文。

## D3：schema 与阅读集成

一个迁移扩展 `message_crypto_results`：一个 `standard` 判别列（增量）与
`revocation_state` CHECK 中的 `'stale'`（表重建，SQLite 无法改 CHECK）。
`ResultVerdict`、`write_result` 与 `results.rs` 读取器为链字段扩展。p12
中间证书以 `key_type='intermediate'` 存储，绝不经 `keystore.put`（它硬编码
'cert'，会把它们泄入锚集）。

`CryptoDecoder` 切换到统一 `receive`；banner 映射器学习 S/MIME 状态
（加密+已验证/未验证/不匹配/无效，no-key/failed 关闭臂复用 `closed`）。
`hide_crypto_parts` 已隐藏 `smime.p7m` 与 pkcs7 媒体类型。阅读接缝的结构性
失败即关闭清除（SP4c-1）原样适用。复合解码器不变（一个加密解码器，现含
两种）。

## D4：契约

`docs/crypto-receive.md`（+ zh）新增 S/MIME 节（framing、验证输入解析、
状态阶梯、schema 扩展）；capabilities S/MIME 行的接收半落地（注记命名
剩余：仅签名、安全 UI、密钥管理）；一个变更日志碎片落地。

## 已知缺口，记录而非静默

- 仅签名 S/MIME（multipart/signed 分离、不透明 signed-data）与其他分离
  签名工作一并推迟；它失败即开（明文+裁决），不同于加密的失败即关闭臂。
- CRL 抓取是 SP4c-3（此处仅缓存；实践中吊销未检查）。吊销的活实证臂需要
  手工构造的 CRL fixture。
- 尚无密钥管理面（SP4d）：实证以编程方式播下接收者 p12 与 CA 链（发送
  测试的模式）。
- 原生签名详情对话框（failure_reason/revocation_reason 显示）是 SP4d；
  banner 此处携带原因文本。

## 测试先行

每个行为都带着它的红落地。解包助手（enveloped 往返、被识别但无法提取的
失败即关闭臂、From 助手）；eContent 提取器（嵌套签名正文恢复）；编排器
（密钥解析、状态阶梯：已验证/未验证/不匹配/无效/no-key/failed、结果行链
字段、失败即关闭底线）；解码器（S/MIME banner 臂）。一个门控的活 harness
实证：播下一条 CA 链（根锚 → 中间 → 发送者叶）与接收者 p12，经引擎接缝
发送 S/MIME 加密+签名，打开送达行，断言文字 + 链已验证 banner + 恢复主题
+ 结果行（chain_valid、revocation_state、签名者 fp）+ 无密文；no-key 与
不受信锚（Invalid）与 From↔SAN 不匹配（Mismatch）否定臂；撤回副本。不
评测/不提升 pipeline（面向模型面不变）。