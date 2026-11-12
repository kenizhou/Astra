# SP4c-1 设计：经阅读管线的 OpenPGP 接收

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** 加密接收半（SP4c）的第一个子切片：经阅读管线解密传入的
OpenPGP PGP/MIME 邮件并呈现安全状态。发送半（SP4b+ECS）已交付并实证；
其实证手工完成「提取并解密」的尾部，本切片把它变成产品路径。解密/验证
原语已存在（`OpenpgpBackend::decrypt_with_signers`，「接收路径的入口」）；
缺的是编排器与阅读集成。无引擎改动：接收骑既有的 `engine.message_source`
原始字节读取与 `MessageDecoder` 接缝；检测住在 Astra 的加密层，是发送
先例的镜像（引擎运输字节，Astra 解释加密 framing）。

## D1：接收编排器（astra-crypto-store）

一个位于 `wrap.rs`（发送模型）旁的新接收模块，如该 crate 其余部分一样
豁免行数上限。`receive_openpgp(account, message_id, source)`：

1. **检测 + 解包** 原始 RFC5322 源中的 PGP/MIME framing：协议为
   `application/pgp-encrypted` 的 `multipart/encrypted`（发送半所包的
   RFC 3156 形态）。提取密文部分。一个新的解包助手（`pgp_mime::wrap_encrypted`
   的逆）；既有 `mime::split_message` 可复用。非该加密 framing → 答「未
   加密」（解码器保持存储渲染不动）。
2. **解析密钥**：从加密 store（`SqliteKeyStore`，private 行）取账户的
   OpenPGP 私钥。无密钥 → `no-key` 状态（失败即关闭，绝不渲染密文）。
3. **解密 + 验证**：直接调用 `decrypt_with_signers`（而非 trait `decrypt`，
   后者丢弃 weak/unverifiable 通道），提供 keystore 签名者 + 已收集证书。
   映射 `DecryptOutcome`：payload、`weak_warning`、`unverifiable_signature`
   → 签名状态（未知签名者为 `UnknownKey`，六态词表）。
4. **持久化结果**：写 `message_crypto_results`（schema 已就绪，今天零
   写入者；这是它的第一个写入者）：crypto_kind、decrypt_state
   （ok/no-key/failed）、signature_state、签名者 fp/email，PK
   （account_id, message_id） linkage 在 Rust 侧强制。
5. **恢复受保护主题**：真实 Subject 骑在受保护头内（发送半的
   `wrap_protected_headers`）；编排器返回它，使快照显示真实主题而非外层
   `...`。

结果是一个中性类型（解密后的腿 + 安全状态 + 恢复的主题），无引擎依赖，
镜像 `wrap.rs` 的依赖方向（store crate 不依赖 astra-app）。

## D2：阅读集成（astra-app + desktop）

一个内置 `MessageDecoder`（一个新的 astra-app 模块；`reading.rs` 接近
上限，故用兄弟模块而非增长）调用编排器并把结果映射为
`DecodeOutcome { html, plain, banner }`：

- 解密后的 plain/html 腿重跑同一 `render_html_leg` 消毒管线（解码接缝已
  如此做）。
- banner 陈述安全状态：已解密+已验证、已解密+未验证（未知签名者）、无法
  解密（无密钥）、失败。失败即关闭：无密钥或失败渲染 banner，绝不渲染
  密文；插件解码器的失败即开不适用于内置加密裁决。
- 隐藏密文伪附件芯片（`smime.p7m` / `encrypted.asc` 附件），即
  `hide_invitation_parts` 先例。

desktop boot 安装一个复合解码器（内置加密与既有 `PluginDecoder` 并列咨询；
今天只安装一个解码器）。加密裁决对它识别的邮件优先；插件解码器仍服务它
自己的邮件。

## D3：契约

一个新的 `docs/crypto-receive.md`（+ zh）：接收路径、失败即关闭状态、检测
边界（Astra 解释 framing，引擎运输字节）、结果行 linkage、受保护主题
恢复。capabilities 的 S/MIME/OpenPGP 行移动：OpenPGP 接收半落地（行注记
命名剩余：S/MIME 接收、安全 UI、密钥管理）。一个变更日志碎片落地。
porting-backlog 不受影响（这是一个 C 簇部分行，而非 kylins 增补）。

## 已知缺口，记录而非隐藏

- 本子切片仅 OpenPGP PGP/MIME 加密。SP4c 内推迟：内联 PGP armor、
  multipart/signed（分离签名）、S/MIME 接收（SP4c-2）、Autocrypt 消费 +
  CRL 抓取 + W9 密码流程（SP4c-3）、静态加密草稿（SP4c-4）。
- 尚无密钥管理面（SP4d）：实证以编程方式播下接收者密钥，即发送实证自己
  的纪律。一个最小导入路径是使接收在测试之外可用的承重后续。
- CLI `messages read` 仍直接读引擎正文（加密邮件在那里答空）；接收路径
  是 desktop 阅读管线。CLI 安全字段是命名的后续。
- token 注册表（cryptoki、openpgp-card）是 SP4d；token 密钥今天失败
  TokenNotPresent。

## 测试先行

每个行为都带着它的红落地。离线（`reading_decode_tests.rs` 仿件模式，
`FakeProvider::with_sources`）：一个 PGP/MIME 加密源解密为文字并带已验证
banner；无密钥 → no-key banner 且无明文；未知签名者 → 未验证；被篡改的
密文 → 失败（失败即关闭）；受保护主题被恢复；密文芯片被隐藏；非加密源
不动；结果行带正确状态落地。编排器单元测试钉住解包、密钥解析、状态映射、
结果行 linkage。一个门控的活 harness 实证（发送实证的尾部自动化）：播下
astra2 密钥，经引擎接缝发送加密 astra1→astra2，打开送达行，断言快照携带
文字 + 已验证 banner + 恢复主题 + 结果行，且密文绝不渲染；no-key 否定臂；
撤回所有副本。不评测/不提升 pipeline（面向模型面不变；AI 传入同步对加密
邮件见不到文本，失败即软，记录在案）。