# SP4b 设计：加密存储与发送闸门（Astra 侧发件箱）

**状态。** 2026-10-22 已定稿。从 main 切出 `sp4b-crypto-integration`。
用户决策 2026-10-22：完整移植 kylins 加密（前后端全含）；发送闸门采用
**Astra 侧绕行路径**（不改上游引擎），在 Astra 内组装 MIME 后再包装；
敏感草稿同样加密保存（其原话："how to handle the Draft encrypted
emails? we need to keep the sensitive draft email in encrypted also"）。

**绕行存在的理由。** 钉定的引擎提交路径只接受 `Draft` 并在 drain 时内部
组装 MIME；没有原始 MIME op，也没有任何钩子（已核对钉定版本与上游主分
支）。kylins 的 `apply_crypto` 包装的是组装好的 MIME，因此加密闸门无法
搭乘引擎队列，而引擎不可改动。于是加密发送走 Astra 侧路径：Astra 用引
擎自己的 `engine_rfc5322::assemble_message`（同图内，输出逐字节一致）
组装邮件，用 kylins 的闸门包装，把包装后的字节排入 Astra 侧持久加密发
件箱，由一个小型 Astra 侧提交器 drain。非加密发送仍走引擎队列不受影
响。代价是第二台更小、仅用于加密邮件的发送引擎；其诚实的边界记录在
已知缺口。

**范围。** 加密存储（schema + KeyStore + 信任 + 解析）、发送闸门
（apply_crypto 语义逐字）、加密发件箱及其传输（SMTP 提交 + Sent 副本归
档）、以及按账户开启闸门的 crypto-method 设置。加密草稿静存是读取半
（SP4c），与解码流水线同片；令牌注册表与原生 UI 是 SP4d。

## D1：crates/astra-crypto-store，rusqlite，一个 crypto.sqlite

在 astra-crypto-* 旁新建 crate：位于 AI 存储旁的 `crypto.sqlite`，
rusqlite 实现，schema 照抄 kylins 原样：`crypto_keys`（含 'harvested'
的 `origin`、CHECK 约束与索引）、`trust_decisions`（仅追加，最新者
胜）、`collected_keys`、`contact_pinned_keys`、`crl_cache`、
`message_crypto_results`、`send_as_aliases`。该存储实现
`astra_crypto_core::KeyStore`（自 kylins `platform/keystore.rs` 移植的
SqliteKeyStore，按账户绑定，`standard|fingerprint` 的 KeyId 编码，
OpenPGP 子钥指纹回退）、信任决策 CRUD 与签名者信任解析、收件人解析
（`resolve_recipient_status_at` 移植：NoKey 带已收集提示、过期先行、自
有密钥即 Ready、决策阶梯 Verified/Personal/Unverified -> Ready、
Rejected、Undecided、AcceptedOtherEmail）、`pin_preferred_handle`、
`recipient_trust_failures`，以及默认签名密钥行（读/写）。sqlx 换成
rusqlite（Astra 的存储惯例，与 astra-ai 一致）；schema SQL 逐字节移
植。

## D2：发送闸门，apply_crypto 逐字

`apply_crypto` 移植到 `crates/astra-crypto-core/apply.rs`（它归属 core
crate：store 是其唯一新增依赖，core 早已引用它）：不签不加时直通；
要求签名而无默认密钥即 `NoSigningKey`；加密 = 发件人在前（自加密）+
全部 To/Cc/Bcc，每个收件人必须解析为 `Ready`，否则得到类型化的
`RecipientTrust` 失败（不产生密文，无明文回退）；钉选感知的句柄选择，
保证闸门评估与加密使用同一密钥；仅 S/MIME 的收件人证书链验证（对照
账户 `key_type='cert'` 锚点，得 `InvalidRecipientCert`）；W4 受保护头
主题隐藏（默认开，读取失败时安全默认开）；仅 PGP 的 Autocrypt-Gossip
注入（签加同启时）；`EncryptOp.sign_with` 组合（绝不先明文签名再加密
的分步组合）；按标准包装：S/MIME 用 `application/pkcs7-mime` enveloped
/ `multipart/signed`，PGP 用 RFC 3156 `multipart/encrypted` /
`multipart/signed`（kylins 的 pgp_mime 模块一并移植）。W9 密码模式
（SKESK 对称）带独立臂：密钥加密与密码并存即 `PasswordConflict`，口令
为空即 `PasswordRequired`，无收件人解析，无 gossip，受保护头规则完全
相同。

## D3：Astra 侧加密发件箱与其提交器

> **2026-10-23 被取代**，由
> `2026-10-23-sp4b-crypto-send-engine-seam-design.md`：闸门已移入
> vendored 引擎的 MIME-wrap 接缝，下述绕行已拆除（crypto outbox、
> Astra 传输与"第二台发送引擎"的已知缺口全部退役）。保留为历史记
> 录；引擎接缝规格现在是契约。

提交时（astra-app 的 `submit_*` 臂，在闸门接缝处），当账户的加密方式
为 `openpgp` 或 `smime`：Astra 用 `engine_rfc5322::assemble_message` 组
装 MIME，应用闸门，把包装后的字节排入 `crypto_outbox`（crypto.sqlite
中的表：id、account_id、standard、wire_bytes、filed_bytes、state
queued/sending/unconfirmed/failed、error、created/attempted）。加密提
交器（`crates/astra-crypto-store/src/transport.rs`）刻意保持小而明确：
EHLO、AUTH PLAIN、MAIL FROM、RCPT TO、带转义的 DATA、出错时 RSET，走
账户已拨号的 SMTP 设置与 engine-tls 连接器；随后以 IMAP APPEND 归档带
Bcc 头的 filed 变体副本。DATA 中途断连落为 `unconfirmed`（引擎
needs-confirmation 的形态）；用户回答已送达（了结）或未送达（重
发），与引擎发件箱的契约同形，而非复制其代码。行项并入既有发送快照
呈现（同一面板，不做第二个发件箱 UI），send-now 与撤回动词对加密队列
的行路由到加密队列。

## D4：账户的加密方式与闸门开关

每个账户新增 crypto-method 设置（`none | openpgp | smime`），随账户设
置快照下发，可在账户页编辑，随账户配置持久。`none` 为零开销默认：不
组装、不触存储。编写器获得逐草稿的 sign/encrypt 标记（SendDraft 形状
的 `sign`/`encrypt` 布尔），作为 compose intent 中的编写器选项，默认
取自账户策略。加密开启时保存的草稿属 SP4c 的加密草稿问题；本切片只
拦截发送。

## D5：证明

单元红灯钉住：存储 schema 与 KeyStore 往返（put/get/find_by_email/
remove、子钥回退）、信任决策最新者胜、收件人解析阶梯（每态、过期先
行、钉选优先）、`recipient_trust_failures`；闸门（直通、NoSigningKey、
收件人信任拒绝后绝不触达 encrypt、链无效的收件人、受保护头开启、密
码冲突/必需、签后加组合形态），使用 kylins 的 MemoryKeyStore 夹具；发
件箱（入队/drain/重试/撤回/unconfirmed/了结），配脚本化 SMTP 夹具；
以及 harness 冒烟：生成密钥，astra1 -> astra2 经加密发件箱的加密发
送，Sent 副本归档，缺钥收件人被阻断且待决行站立。

## 已记录的已知缺口，不隐藏

- 绕行是第二台更小、仅用于加密邮件的发送引擎：它自有重试/退避、
  unconfirmed 语义与传输韧性，而引擎的提交器曾花多个版本打磨这些
  （部分 DATA 处理、AUTH 续约、期限纪律）。该子集是诚实且经测试的，
  记此缺口以免误当对等。
- IMAP/Graph/Google/JMAP 各 provider 不受影响：本切片的加密提交器仅
  SMTP+IMAP(APPEND)；无 IMAP SMTP 设置的账户得到类型化的不可用错误，
  绝不静默丢弃。
- 令牌承载的密钥在有 provider 注册时经后端 TokenProvider 路由
  （SP4d 的注册表）；无 provider 的令牌密钥得到类型化失败
  （`TokenNotPresent`），绝不回退软件密钥。
- CRL 拉取随接收侧（SP4c）；S/MIME 发送侧链门只使用已存储锚点，与
  kylins 的 apply 一致。
- 加密草稿静存（用户注记）属 SP4c，与解码流水线同片，因二者同为静存
  解密问题；草稿池形态不因本切片改变。
