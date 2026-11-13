# SP4c-4 设计：静态加密草稿

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** 撰写草稿由引擎以明文持久化：排队期间的 outbox pending_op_payload
JSON，以及预热后同步的 blob 缓存与 message_body/FTS。Astra 在交给
engine.put_draft 之前自行构建 Draft，故正文可拦截，但 Engine::open 没有
store/key 注入点，且 MimeWrapper 缝不覆盖 put_draft。在不允许引擎改动的前提
下，引擎自身的副本无法原地加密。本切片新增一个由 Astra 持有的加密草稿镜像，
外加一个恢复读取缝：持久化的 Astra 持有副本经 vault 加密，恢复优先取它，仅对
从未镜像的草稿回退到引擎正文。vault（AES-256-GCM、OS keyring 主密钥、
vault.rs）已存在。无引擎改动；不评测/不提升 pipeline。

## D1：加密草稿镜像（store）

astra-crypto-store 新增迁移 0005，加入 encrypted_drafts，键为
(account_id, draft_key)，body_enc BLOB 存 vault blob，updated_at，主键
(account_id, draft_key)。draft_key 是草稿稳定的 Message-ID 头
（Draft.message_id，预生成、无尖括号），与恢复从引擎消息导出的身份一致。blob
用新 AAD 命名空间 astra:{account}:draft:{message_id}:1 包裹，镜像私钥 AAD
（astra:{account}:private_key:1），故跨账户/字段重放会 AEAD 失败。keyring 主密
钥即根；无需新主密钥表。

store 把载荷当作不透明字节（绝不命名 engine_api::Draft，故无新依赖）：
put_encrypted_draft(account, key, plaintext) 用草稿 AAD 做 vault 加密并 upsert；
get_encrypted_draft(account, key) 读行（无行 -> Ok(None)）再 vault 解密
（Ok(Some(bytes)) 或类型化错误）；delete_encrypted_draft(account, key) 删行。
keyring 不可用是类型化的 CryptoVaultError::Keyring；栈纪律是失败即关闭。

## D2：保存/丢弃钩子（core）

astra-app 新增子模块 drafts/encrypt.rs（drafts.rs 接近上限），把渲染后的
Draft 序列化为 JSON（Draft 同时 derive Serialize + Deserialize）并驱动镜像。
save_draft_gated 的写钩子在 engine.put_draft 之前运行：先加密 + 写镜像，vault
出错即拒绝保存（失败即关闭，不持久化明文；撰写器把草稿留在内存并 surfaced 错
误）。put_draft 成功后镜像保留；若 put_draft 本身失败则回滚镜像。摘要/取代逻辑
不受影响（重保存 upsert 同一键）。删除钩子在草稿被移除处触发（两条丢弃路径与
发送后清理）。CryptoStore 恒安装（boot.rs），故镜像对每个草稿生效；vault 主密
钥首次使用时在 OS keyring 惰性创建。

## D3：恢复读取缝 + 契约

resume_latest_draft / resume_draft_by_key 先调 get_encrypted_draft。三种结果
彼此区分：Ok(Some(bytes)) -> 反序列化 Draft 并映射为 DraftResume（文字来自
vault，而非引擎缓存）；Ok(None) -> 草稿从未镜像（在别处写入），如今日回退
engine.message_body；Err(vault) -> 镜像存在但解不开（keyring 丢失），失败即关
闭 banner，绝不回退明文。docs/crypto-receive.md 新增静态草稿注记（镜像、读取
缝、内存明文、失败即关闭拒绝）并镜像到 zh。capabilities 行命名静态草稿落地。
落一个变更日志碎片。

## 已知缺口，记录而非静默

- 引擎自身的静态副本仍是明文：排队期间的 outbox 载荷、同步的 blob 缓存、预热
  后的 message_body/FTS。本切片保护持久化的 Astra 持有副本并把恢复路由经
  vault；不清除引擎残留。若要求是「磁盘上绝不明文」，交引擎前加密（给
  put_draft 一个带密文的 Draft）是后续；它使服务端草稿对其他客户端不可读、搜
  索/摘要退化，故是产品决策，而非本切片。
- keyring 不可用的保存被拒绝（失败即关闭），surfaced 为错误；草稿不以明文持久
  化。这是栈纪律。
- AI draft_pool.reply 明文是相邻的静态暴露，此处不在范围（同一 vault，后续）。
- CLI 对齐：CLI 今日直接读引擎正文；镜像是桌面阅读路径保护，如其他接收面。

## 测试先行

每个行为都带着它的红落地。store 测试（经 vault 的 put/get/delete 往返；AAD 绑
定账户 + 草稿键，故跨账户 blob 失败；缺行是 Ok(None)；keyring 不可用是类型化
的）。core 测试（保存钩子在 put_draft 之前写镜像并在 vault 出错时拒绝；丢弃删
除它；恢复优先镜像、Ok(None) 回退、Err 失败即关闭）。一个门控活 harness 实证：
武装测试主密钥，保存草稿、断言镜像行是密文（非明文）、恢复返回文字、丢弃移除
镜像；撤回副本。无活模型证明；离线电池即证明。