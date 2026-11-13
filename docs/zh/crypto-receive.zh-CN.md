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
- **需要密码**（一个 SKESK 槽且无 PKESK 密钥适配）：一个「需要口令」banner
  连同密码提示 affordance，无明文，无结果行（一个瞬态交互，而非持久化
  状态）。下述 W9 密码流用用户提供的口令解密它。
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
密钥的审阅/提升面在 SP4d-2 落地（下述审阅面）；此处暂存静默。

## CRL 抓取路径（SP4c-3b）

S/MIME 链检查消费 CRL 缓存；SP4c-3b 填充它。在 `verify_with_context` 之前，
接收钩子从签名者叶与每个中间证书提取 CRL 分发点 URL（smime crate 的
`crl_distribution_point_urls`，SP4c-2 解析规则），去重，并经 `CrlFetcher`
端口逐个抓取或缓存。仅 `http` 与 `https` URL 可接受：分发点 URL 受攻击者
影响（它来自证书），故任何其他 scheme 被丢弃。

端口遵循 `SendCryptoPolicy` 先例：store 定义 `CrlFetcher`，宿主实现它，故
store 绝不依赖 astra-app。desktop 实现（`HttpCrlFetcher`）做一次有界 HTTP
GET：短超时、响应大小上限、仅 http(s)、无重定向。新鲜缓存的 CRL 直接沿用
而不抓取；过期或缺失的触发一次抓取与 upsert。抓取失败即软：任何传输、
scheme、大小或解析失败都把证书降级为 unchecked 吊销，绝不阻塞邮件打开
（链本就软失败）。链按签名时刻验证，故新抓取的 CRL 仍对签名瞬间检查。来自
恶意 URL 的伪造 CRL 失败于链的 CRL 签名检查（检查器对签发 CA 验证）。

吊销状态如今在实践中可达。吊销签名者序列号的 CRL 使链硬失败：
`chain_valid=false`、`revocation_state=revoked`、Invalid 签名状态，以及被
区分的吊销 banner。无可用 CRL（空缓存、抓取失败、或链读作 unchecked 的过期
CRL）时打开以 `revocation_state=unchecked` 继续。离线 no-op 抓取器在未附
宿主抓取器时使阅读路径仅缓存。

两个考量被记录而非隐藏。隐私：抓取 CRL 向 CA 暴露正在验证哪些证书（一个
追踪向量）；OCSP/stapling 替代是后续。残余盲 SSRF：scheme 过滤接受 http(s)
但不阻止私有或环回地址，故恶意分发点仍能把有界 GET 瞄准内部主机；地址段
过滤是硬化后续。抓取保持有界且内联；后台预取是后续。

## W9 密码流（SP4c-3c）

受口令保护的 OpenPGP 邮件携带一个 SKESK 槽（对称加密），而非或同时携带
PKESK 密钥包装。解包后，`has_skesk` 检测它。当无 PKESK 密钥适配时，
`receive_openpgp` 答瞬态 `NeedsPassword` 裁决，而非 no-key/failed：一封邮件
可同时携带两种槽，密钥适配时密钥路径仍胜出，故 SKESK 标志仅重定向无适配
密钥的臂。硬错误（篡改、坏签名）即使存在 SKESK 槽也失败即关闭，绝不降级
为提示。

解码器把 `NeedsPassword` 映射为失败即关闭的「需要口令」banner 连同密码提示
affordance（`ReadingSnapshot.password_prompt`）：无腿、无密文、无结果行。
阅读 pane 在该 affordance 上挂载其密码对话框。对话框收集口令并经一个 desktop
命令提交，该命令在进程内携带它（`crypto_wrapper` 先例：秘密留在宿主内存，
绝不记录或持久化）。命令在一个新鲜 `engine.message_source` 上重跑
`receive_openpgp_password`，并经同一渲染接缝刷新阅读快照（阅读重入模块，
从接近上限的 `reading.rs` 拆出）。

`receive_openpgp_password` 经 `decrypt_symmetric` 解密，采用与密钥路径相同的
`known_signers` / `extra_signer_blobs` 解析，故内嵌签名仍对已知对等体验证，
未知签名者退化而非中止。成功时落地与密钥路径相同的裁决、恢复主题与 `ok`
行，affordance 落下（对话框在正文上关闭）。错误口令答瞬态 `WrongPassword`
重提示信号：对话框重提示，命名拒绝，无明文，无行。结构性失败恰如密钥路径
失败即关闭。取消绝不到达编排器：裁决保持 needs-password（失败即关闭）。

口令仅保存在内存：它作为一个 `SecretBox`（drop 时清零）移入后端，绝不记录
或持久化。needs-password 与 wrong-password 状态是瞬态交互，而非持久化行
（`decrypt_state` CHECK 保持 ok/no-key/failed/n/a）；成功写 ok。重新打开会
重提示（无缓存口令）。

## 静态草稿镜像（SP4c-4）

引擎以明文持久化撰写草稿（排队时的 outbox 载荷、warm 后的同步 blob 缓存与
message_body/FTS），且不可更改。Astra 在 `engine.put_draft` 之前自行构造
Draft，故正文可拦截。SP4c-4 增加一个由 Astra 持有的加密草稿镜像，外加一个
恢复读取接缝：持久的 Astra 持有副本经 vault 加密，恢复优先读它，仅对从未
镜像的草稿回退到引擎正文。

镜像是 `encrypted_drafts` 表（迁移 0005），以 (account_id, draft_key) 为键，
其中 draft_key 是草稿稳定的 Message-ID（`Draft.message_id`，预生成、无尖
括号），即恢复从存储邮件推导的同一身份。正文是 vault blob
（`hex(0x01 || nonce || ct+tag)`，OS 钥匙串主密钥下的 AES-256-GCM），以
AAD `astra:{account}:draft:{message_id}:1` 包装，镜像私钥 AAD，故从某一
账户或草稿移用的 blob 在别处无法通过 AEAD 验证。存储把载荷当作不透明
字节；Draft 到 JSON 的 serde 位于 astra-app（`drafts/encrypt.rs`）。

保存钩子在 `engine.put_draft` 之前运行：它序列化渲染后的 Draft 并写入镜像。
vault 错误拒绝保存（失败即关闭，不持久化明文；撰写器在内存中保留草稿，
拒绝被记录）。账户行先被幂等注册（FK 目标，MIME 包装器惯例），故无加密
包装器的普通账户仍能保存。镜像在引擎保留草稿的保存中存活：停泊保存
（outbox 在 provider 调用之前记录 op）与已应用但未记录的写入都保留草稿，
故镜像站立。仅当引擎未保留任何内容时才回滚（op 入队之前的载荷编码失败）；
歧义的存储错误保守地保留镜像，因为孤儿镜像是无害密文，而回滚一个被保留
草稿的镜像会剥去其静态保护。重保存 upsert 同一键；摘要/取代逻辑不受影响。
删除钩子在两条丢弃路径与发送后清理（经撰写丢弃路由）上触发。

恢复读取接缝先调用 `get_encrypted_draft`。三种结果彼此区分：存在的镜像
反序列化 Draft 并映射为 DraftResume（文字来自 vault，而非引擎缓存）；缺席
的镜像（`Ok(None)`，草稿从未镜像）照旧回退到 `engine.message_body`；存在
但无法解密的镜像失败即关闭（阅读快照上的「无法解密」banner，不恢复，
绝不回退明文）。DraftResume 与阅读快照形状不变。

明文仅存在于内存与 vault blob 内：主密钥留在 OS 钥匙串（清零、绝不记录），
此处不记录草稿内容。

记录的缺口：引擎自身的静态副本仍是明文（排队时的 outbox 载荷、同步 blob
缓存、warm 后的 message_body/FTS）。本切片保护持久的 Astra 持有副本并把
恢复路由经 vault；它不清除引擎残留。若要求是「磁盘上绝不明文」，则
引擎前加密（交给 put_draft 一个携带密文的 Draft）是后续；它使服务端草稿
对其他客户端不可读并退化搜索/摘要，故是一个产品决策，而非本切片。AI
draft_pool.reply 明文是相邻的静态暴露，超出范围（同一 vault，一个后续）。
CLI 对等：CLI 直接读引擎正文；镜像是 desktop 阅读路径保护，恰如其他接收面。

## 密钥导入面（SP4d-1）

接收与发送弧暂存并消费密钥，但人需要一个把密钥带进来的途径。SP4d-1 落地
desktop 密钥导入面：导入你自己的私钥/身份密钥（使收到的加密邮件可解密），
以及导入对等体的公钥证书（使签名可验证、加密发送可解析）。CLI 密钥动词与
sidecar 虚拟工具是 SP4d-1b；已收集密钥的审阅/提升面在 SP4d-2 落地（下述审阅面）。

两条路径，刻意分离以避免 has_private 污染：

- 公钥证书路径（`astra-crypto-store::import_public_cert`）解析材料（OpenPGP
  armor 或二进制经 `parse_certs`，或 S/MIME PEM/DER 证书经
  `fingerprint_of_cert_der` / `cert_summary_of_der`），并经
  `upsert_crypto_key` 以 `private_data: None`、`key_type: 'public'`（OpenPGP）
  或 `'cert'`（S/MIME）、`origin: 'imported'` 存储仅公钥 blob。它绝不经过
  后端 `import_key` / keystore `put`，那会记录私钥 blob 并把 `has_private`
  置真，污染接收解密候选过滤器与解析器的 Personal/Ready 分支。携带秘密的
  OpenPGP 输入在此被拒绝：私钥属于私有路径。跨标准导入被拒绝
  （`import_public_cert_as` 钉住标准）。
- 私钥/身份路径复用既有的后端 `import_key`（口令门控的秘密解密、经 keystore
  持久化），它正确记录在账户绑定 AAD 下静态包裹的私钥 blob。S/MIME 臂使用
  `import_key_with_chain`，把包中的中间证书落为 `intermediate` 行（绝不作为
  信任锚）。

app 方法（`astra-app/src/keys.rs`，`reading_password.rs` 形态）嗅探材料并
分发：OpenPGP TSK 或 S/MIME 密钥包走私有路径，其余走公钥路径。某标准的第一个
私钥在尚无默认时成为账户的默认签名密钥，故导入后账户即可签名；后续导入绝不
夺取既有默认。`list_keys` 跨两个标准应答账户的行，仅公开视图。

desktop 命令（`commands/crypto.rs`）在进程内携带字节与可选口令
（`submit_reading_password` 先例：秘密留在宿主内存，绝不记录或持久化）。web
入口位于设置 Keys 类别（`features/keys`）：一个文件选择器与一个粘贴字段，
可选口令仅保存在局部状态、用完即清。wire 类型走 `services/crypto.ts`，绝不
走 `ipc.ts`。

一个共享的信任写入助手（`record_import_trust`）在 `put_trust_decision` 之上
记录 `verified` / `unverified` 决定；已收集密钥的提升面（SP4d-2）复用它。显
式决定正是使对等密钥可用于发送的东西。公钥导入本身记录默认接受：导入的对等
密钥落地为 `unverified`（发送解析器把 `unverified` 视为可加密，故导入后即可
用于发送；日后带外验证把它提升为 `verified`）。既有决定绝不降级，故重导入一
个已验证密钥保持 `verified`；无 email user id 的证书没有可决定的对等地址，由
审阅面（SP4d-2）接管。

对已私持指纹的公钥重导入保留既有行：默认签名/默认加密标志、token 绑定、以及
`key_type`（私钥绝不降级为 `public`）。upsert 的冲突子句无条件覆盖这些字段，
故导入把既有值带上前；两条路径上后来的导入都绝不窃取既有默认。

口令仅保存在内存：它以 `SecretBox`（drop 时清零）流转，绝不记录或持久化；
只有 vault 包裹的密钥 blob 静态落地。

门控活实证（`tests_live_harness/crypto_key_import.rs`）经产品面导入两个密钥
（无编程播种），发送一封加密+签名邮件，并断言其解密且验证，在公钥导入上钉住
has_private 守卫。除非 `ASTRA_LIVE_HARNESS=1` 否则跳过。

## 已收集密钥审阅面（SP4d-2）

接收与发送弧把对等体密钥静默暂存进 `collected_keys`（上述 Autocrypt 消费；
gossip、WKD 与 S/MIME 收割同样如此），但人需要一个审阅、接受或拒绝暂存密钥的
途径。SP4d-2 在完整的存储层之上落地 desktop 审阅/提升面：列出暂存密钥、接受
（提升进 `crypto_keys` 并附带一个信任决定）、接受为已验证、或拒绝（移除暂存行
并附带一个 rejected 决定）。CLI 密钥动词与 sidecar 虚拟工具是 SP4d-1b（desktop
优先的先例）。

app 方法（`astra-app/src/collected.rs`，`keys.rs` 形态）暴露：

- `list_collected(account)`：账户跨两个标准的暂存行，最新优先（存储的
  `seen_at DESC, id DESC` 排序跨标准合并，同秒平局按行 id 打破）。仅公钥材料。
- `promote_collected(account, standard, fingerprint, email, verified)`：存储
  `promote_collected_key`（排空暂存行且绝不降级既有 keystore 行）然后信任写入。
  这一配对是承重的：仅提升只是接受的一半，显式决定才使提升后的密钥可用于发送
  （无决定时解析器答 Undecided）并解除 S/MIME 收割锚排除。`verified` 选择总是
  记录 verified（一次升级）；`unverified` 选择仅在无决定成立时记录 unverified
  （绝不降级既有的 verified/personal 决定）。接受绑定 `email`，未命名时绑定暂存
  行的对等体邮箱；无对等体邮箱的密钥无地址可决定，提升仍落地密钥。
- `reject_collected(account, id, standard, fingerprint, email)`：按 id 移除暂存
  行并记录一个 `rejected` 决定。暂存是静默且决策盲的，故重新打开源邮件会重新暂
  存该密钥，审阅列表会再次提供它；拒绝仍然成立：重新暂存副本的 unverified 提升
  绝不降级它，且一旦 keystore 行存在解析器即答 `Rejected`（仅暂存密钥答 `NoKey`，
  决定只针对 keystore 行读取）。决策感知的暂存/列表（绝不重新提供被拒密钥）是记
  录在案的后续，而非本切片。

一个共享存储助手（`import.rs` 中的 `record_promote_trust`）以无降级守卫记录
提升接受，复用 `put_trust_decision` 之上的 `record_import_trust`。

desktop 命令（`commands/crypto.rs`：`list_collected`、`promote_collected`、
`reject_collected`）携带账户、标准、指纹、暂存 id、邮箱与 verified 标志，在宿主
注册。无秘密跨越此面（已收集密钥是公钥材料）。web 入口位于设置 Keys 类别
（`features/keys/CollectedKeysReview.tsx`，接入 Keys 详情）：每个暂存行显示指纹、
对等体邮箱、来源与标准，并提供 Accept（unverified）、Accept as verified 与
Reject。指纹被显示，故人在选择 Accept as verified 前可带外核验它。wire 类型
骑乘 `services/crypto.ts`，绝非 `ipc.ts`。

门控活实证（`tests_live_harness/crypto_collected_review.rs`）接收一封暂存对等体
密钥的 Autocrypt 邮件，经产品面列出它，提升它（信任决定落地），并发送由提升
单独准入的给该对等体的加密邮件（无编程信任播种）；它拒绝第二个暂存密钥并断言
其被移除，然后撤回每个副本。除非 `ASTRA_LIVE_HARNESS=1` 否则跳过。

## 已知缺口，记录而非静默

- 按标准仅加密接收：OpenPGP PGP/MIME（SP4c-1）与 S/MIME enveloped-data
  （SP4c-2）。内联 PGP armor 与分离签名形态（PGP multipart/signed、S/MIME
  multipart/signed 与不透明 signed-data）是后续；Autocrypt 消费在 SP4c-3a
  落地（上述接收半）；CRL 抓取在 SP4c-3b 落地（上述抓取路径）；W9 密码流在
  SP4c-3c 落地（上述密码流）；静态加密草稿在 SP4c-4 落地（上述镜像）。
- W9 发送半本身未接线（`apply_password_crypto` 无调用者，`SendDraft` 无
  `password_protected` 字段）：活实证经 `encrypt_symmetric` 手工构造 SKESK
  邮件。接线 W9 发送是一个协调后续。口令绝不持久化，故重新打开会重提示；
  会话口令缓存是后续。密码对话框仅 shell（阅读 pane），恰如其他接收面。
- Autocrypt 暂存静默：已收集密钥的审阅/提升 UI 在 SP4d-2 落地（上述审阅面）。
  发送门在密文内发出 `Autocrypt-Gossip`，但尚无明文自身 `Autocrypt` 头；当传入
  邮件携带明文头（对等客户端的）时消费读取它，自身头的发出是后续。gossip 消费
  读取发送门发出的受保护头包装器；超出该包装器的第三方 gossip 放置是后续。
- S/MIME 吊销抓取在 SP4c-3b 落地（上述 CRL 抓取路径）：接收钩子经失败即软的
  `CrlFetcher` 端口抓取或缓存签名者链的分发点 CRL，故实践中吊销签名者使链
  硬失败。抓取失败或空缓存仍读作 unchecked（失败即软，绝不阻塞打开）。链的
  Stale 吊销状态持久化为 unchecked（Good/Stale 粒度是后续）。残余盲 SSRF
  硬化（私有/环回地址过滤）与保护隐私的 OCSP/stapling 替代仍是后续。
- 密钥导入面在 SP4d-1 落地（上述密钥导入面）：自身私钥与对等体公钥证书经
  产品面导入，活实证如今导入而非播种。CLI 密钥动词与 sidecar 虚拟工具是
  SP4d-1b；已收集密钥的审阅/提升面在 SP4d-2 落地（上述审阅面）。密钥生成、
  导出、超出首个导入的默认密钥选择，以及编写器收件人信任 UI 是后续。
- 已收集密钥审阅面在 SP4d-2 落地（上述审阅面）：经产品面列出、接受、接受为
  已验证与拒绝，提升与使密钥可用于发送的信任决定配对，活实证如今提升而非
  播种。CLI 密钥动词（`keys collected`/`promote`）与 sidecar 虚拟工具是
  SP4d-1b（动词随其虚拟权限工具落地，needs-approval 默认）。pin 入口
  （`set_pinned_key`，标记提升后的密钥为首选）是后续；提升无需它即落地可用
  于发送的密钥。编写器收件人信任 UI（发送时显示信任状态）是后续。暂存与审阅
  列表是决策盲的：被拒密钥在其源邮件重新打开时重新暂存，列表再次提供它。拒绝
  仍然成立（unverified 提升绝不降级它；一旦 keystore 行存在解析器即答
  Rejected），故这是重新提供，绝非静默重新接受。决策感知的暂存/列表（绝不重新
  提供被拒密钥）是后续行为变更。
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
| CRL 抓取：DP 提取、失败即软抓取器端口、接收钩子 | ✅ | `astra-crypto-smime/src/cert.rs`、`astra-crypto-store/src/crl.rs`、`receive_smime.rs`、`client/desktop/src/crl_fetch.rs` |
| W9 密码流：SKESK 检测、needs-password 裁决、内存口令、重提示 | ✅ | `astra-crypto-store/src/receive.rs`、`astra-app/src/reading_password.rs`、`crypto_decode.rs`、`client/desktop/src/commands/crypto.rs`、`client/web/src/components/reading-pane/PasswordDialog.tsx` |
| 静态草稿：加密镜像、保存/丢弃钩子、恢复读取接缝、失败即关闭拒绝 | ✅ | `astra-crypto-store/src/drafts.rs`、`migrations/0005_encrypted_drafts.sql`、`astra-app/src/drafts/encrypt.rs`、`drafts.rs`、`drafts/resume.rs` |
| 密钥导入：仅公钥存储路径、私有后端路径、has_private 守卫、内存口令、信任写入 | ✅ | `astra-crypto-store/src/import.rs`、`astra-app/src/keys.rs`、`client/desktop/src/commands/crypto.rs`、`client/web/src/features/keys/KeysDetail.tsx` |
| 已收集密钥审阅：列出、提升加信任配对、无降级守卫、rejected 决定 | ✅ | `astra-crypto-store/src/import.rs`（`record_promote_trust`）、`astra-app/src/collected.rs`、`client/desktop/src/commands/crypto.rs`、`client/web/src/features/keys/CollectedKeysReview.tsx` |
| 活实证（发送加密、打开、解密、否定臂、静态草稿、密钥导入、已收集审阅） | ✅ | `tests_live_harness/crypto_receive.rs`、`crypto_receive_smime.rs`、`crypto_receive_autocrypt.rs`、`crypto_receive_crl.rs`、`crypto_receive_password.rs`、`crypto_drafts_at_rest.rs`、`crypto_key_import.rs`、`crypto_collected_review.rs` |
| 内联 armor、分离签名 | N/A | SP4c-3，上述缺口 |
| 密钥生成与导出、token 注册表、原生安全 UI | N/A | SP4d 后续 |