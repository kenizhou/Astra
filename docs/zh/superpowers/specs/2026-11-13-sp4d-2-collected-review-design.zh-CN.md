# SP4d-2 设计：已收集密钥的审阅与提升面

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** Autocrypt/gossip 消费把对端密钥静默暂存进 collected_keys（SP4c-3a）；
接收已把它们提供给解密/验证解析，但没有面向用户的途径去审阅、接受或拒绝一个暂
存密钥。promote_collected_key 在 store 中已存在且有测试，但零产品调用方。本切片
接线审阅/提升面：列出暂存密钥、接受（提升进 crypto_keys + 一个 trust 决策）、验
证（提升 + 一个 verified 决策）、或拒绝（移除暂存行 + 一个 rejected 决策）。store
层已完整（promote_collected_key 带其 no-downgrade 守卫与保留来源的 promote_origin、
list/remove 查询、trust 与 pin API）；缺的是其上的一切。CLI 密钥 verb 是 SP4d-1b
（桌面先例）；token registry 与原生安全 UI 是更重后续。无引擎改动；不评测/不提升
pipeline。

## D1：app 审阅方法 + 提升 trust 写入

astra-app 新子模块（reading_password.rs / keys.rs 形态：子模块 impl App 块读
self.crypto_store()；lib.rs 在上限，不增长）暴露：

- list_collected(account)：账户的暂存行（CollectedKeyRow，已 serde 可序列化，
  public_data 为 base64），最新优先。
- promote_collected(account, standard, fingerprint, email, verified)：调用 store
  promote_collected_key（它排空暂存行且绝不降级既有 keystore 行），然后记录 trust
  决策。这是承重配对：promote 本身只是接受的一半，显式决策才使被提升密钥可用于
  发送（无决策时解析器答 Undecided），并解除 S/MIME harvested-anchor 排除。
  verified 选择总是记录 verified（升级）；unverified 选择仅在无决策时记录
  unverified（绝不降级既有 verified/personal 决策）。无对端 email 的密钥没有可决
  定的地址；提升仍落地该密钥。
- reject_collected(account, id, standard, fingerprint, email)：移除暂存行
  （remove_collected_key 按 id）并记录一个 rejected 决策，故同一密钥的再暂存副本
  被解析器拒绝，而非静默再提供。

一个共享 store 助手以 no-downgrade 守卫记录提升接受（复用 record_import_trust 基于
put_trust_decision）。

## D2：desktop 命令 + web 审阅 UI

commands/crypto.rs（或从其声明的子模块；commands.rs 在上限，其中不加新 mod 行）
新 desktop 命令携带 account / standard / fingerprint / id / verified 标志，在
client/desktop/src/lib.rs 注册。features/keys/ 新 web 审阅组件（接入 Keys 详情）
列出暂存密钥，显示 fingerprint、对端 email、来源、标准，并提供 Accept（unverified）、
Accept as verified、Reject。wire 类型走 services/crypto.ts（绝不在 ipc.ts，在上
限）。无密钥穿过此面（已收集密钥是公开材料）；显示 fingerprint 以便人在选择
Accept as verified 前带外验证。

## D3：契约

docs/crypto-receive.md 新增审阅/提升注记（list/accept/verify/reject 流程、
promote+trust 配对、no-downgrade 守卫、rejected 决策）并镜像到 zh。capabilities
行命名审阅/提升落地。落一个变更日志碎片。

## 已知缺口，记录而非静默

- CLI 密钥 verb（keys collected/promote）+ sidecar 虚拟工具是 SP4d-1b（verb 与其
  虚拟权限工具同片落地，needs-approval 默认）；桌面先例。
- token registry（cryptoki、openpgp-card）与原生安全 UI 是更重后续；token 行与
  KeyHandle::Token 路由已存在。
- pin 导入面（set_pinned_key，标记被提升密钥为首选）是后续；提升无需它即落地可用
  于发送的密钥。
- 撰写器收件人 trust UI（发送时显示 trust 状态）是后续。

## 测试先行

每个行为都带着它的红落地。app 测试（list 返回暂存行；promote 落地密钥并记录 trust
决策使其可用于发送；verified 提升升级，unverified 提升绝不降级既有 verified 决策；
reject 移除暂存行并记录 rejected；无 email 提升仍落地密钥）。一个门控活 harness 实
证：接收一封暂存对端密钥的 Autocrypt 邮件，经产品面列出，提升它（trust 决策落地），
并向该对端发送仅由提升即可准入的加密邮件；拒绝第二个暂存密钥并断言其被移除；撤回
副本。无活模型证明；离线电池即证明。