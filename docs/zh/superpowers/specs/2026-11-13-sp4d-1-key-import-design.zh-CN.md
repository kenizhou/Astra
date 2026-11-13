# SP4d-1 设计：密钥导入核心

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** 加密接收/发送弧会暂存并消费密钥，但没有面向用户的导入途径：活实证
以编程方式播种密钥。docs/crypto-receive.md 把「最小导入路径」命名为使接收超出
测试可用的承重后续。store 层已很丰富（upsert_crypto_key、trust-decision API、
promote_collected_key、pin API 均已存在且有测试）；缺的是其上的一切。本切片接
线一个桌面密钥导入面：导入你自己的私钥/身份密钥（使接收邮件可解密），导入对端
的公钥证书（使签名验证与加密发送可用）。CLI 密钥 verb + sidecar 虚拟工具是
SP4d-1b（接收弧的桌面先例）；collected-keys 的审阅/提升面是 SP4d-2；token
registry（cryptoki、openpgp-card）与原生安全 UI 是更重后续。无引擎改动；不评
测/不提升 pipeline。

## D1：store 导入函数

两条导入路径，刻意分离以避免 has_private 污染：

- import_public_cert(account, bytes)：经 astra-crypto-openpgp parse_certs（armor
  或二进制、至少一个证书）+ fingerprint_of 解析/校验；把仅公钥 blob
  （cert_to_public_blob，hex）经 upsert_crypto_key 存储，key_type:'public'、
  origin:'imported'、private_data:None。绝不走 backend import_key /
  SqliteKeyStore::put_sync，它们硬编码私钥 blob，会置 has_private 为真，污染接收
  解密候选过滤（receive.rs:188）与解析器 Personal/Ready 分支（resolve.rs:170）。
  S/MIME 对齐用既有证书解析（fingerprint_of_cert_der / cert_summary_of_der），
  key_type:'cert'。返回 CryptoKeyRow。
- 私钥/身份密钥导入复用既有 backend import_key（口令门控的密钥解密、
  persist_cert），它正确记录私钥 blob；本切片接线它，不重造。

暴露一个共享 trust 写入助手（put_trust_decision verified/unverified）供导入面使
用，并被 SP4d-2 的 promote 复用（promote 本身只是接受的一半：显式 trust 决策才
使密钥可发送，并解除 S/MIME harvested-anchor 排除）。

## D2：app 方法 + desktop 命令 + web 导入面

astra-app 新子模块（reading_password.rs 形态：子模块 impl App 块读
self.crypto_store()；lib.rs 在上限，不增长）暴露 import_key(account, bytes, 可选
passphrase)，按输入是否携带密钥材料分派到公钥证书或私钥路径，以及
list_keys(account)。commands/crypto.rs（64 行，有余量；commands.rs 在上限，故其
中不加新 mod 行）新 desktop 命令进程内携带 bytes/passphrase（submit_reading_password
先例：密钥留在进程内，绝不记录），在 client/desktop/src/lib.rs 注册。新
client/web/src/features/keys/ 导入组件（文件/粘贴）+ wire 类型在
services/crypto.ts（绝不在 ipc.ts，在上限），接入设置栏。口令仅存内存，零化，
绝不记录/持久化。

## D3：契约

docs/crypto-receive.md 新增密钥导入注记（两条路径、has_private 防护、内存口令、
trust 写入）并镜像到 zh。capabilities 行命名密钥导入落地。落一个变更日志碎片。

## 已知缺口，记录而非静默

- CLI 密钥 verb（keys import/list）+ sidecar 虚拟工具是 SP4d-1b（verb 与其虚拟
  权限工具同片落地，needs-approval 默认）；接收弧的桌面先例。CLI messages
  read 仍直接读引擎正文（未解密）；CLI 安全字段是另行命名的后续。
- collected-keys 的审阅/提升面是 SP4d-2（promote + trust 写入 + 拒绝/删除 + 审阅
  UI）；在此之前暂存保持静默。
- token registry（cryptoki、openpgp-card）与原生安全 UI 是更重后续；token 行与
  KeyHandle::Token 路由已存在。
- 密钥生成、导出、撰写器收件人 trust UI 是后续。

## 测试先行

每个行为都带着它的红落地。store 测试（import_public_cert 往返一个公钥证书且
has_private 为假；畸形输入不暂存任何内容且绝不 panic；提取 fingerprint/user-id；
公钥路径绝不置私钥 blob；跨标准导入被拒；trust 写入助手记录决策）。app/命令测试
（导入分派公钥与私钥；list 返回行；口令绝不泄漏）。一个门控活 harness 实证：经
产品面导入对端公钥证书与自己的私钥，随后接收一封加密+签名邮件可解密并验证（无
编程播种）；撤回副本。无活模型证明；离线电池即证明。