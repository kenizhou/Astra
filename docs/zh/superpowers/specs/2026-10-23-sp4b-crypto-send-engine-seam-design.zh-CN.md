# SP4b-接缝设计：加密闸门移入引擎（绕行退役）

**状态。** 2026-10-23 已定稿。从 main 切出 `engine-crypto-seam` 分支。
本规格**取代** `2026-10-22-sp4b-crypto-send-design.md` 的 D3 绕行半
（该规格保留并加横幅；历史不改写）。用户决策 2026-10-23：保持架构简
洁 —— 改引擎而不是 Astra（其原话："现在的方向已经确定了，我们应该保
持架构的简洁性，应该重新修改engine 而不是 Astra"）；给出的三种形态
中他们选择：信任闸拒绝 = 引擎失败行 + Send Now；归档变体只包装一次、
线上变体 = 同一密文去掉 Bcc 头；IMAP 先行、trait 定义于中立的
engine-provider。

**反转的理由。** SP4b 的 D3 交付了第二台更小、仅用于加密邮件的发送
引擎：Astra 侧 `crypto_outbox` 及其自有的 SMTP 提交器 + IMAP Sent 归
档，因为当时钉定的引擎只接受 `Draft` 负载且不可改动。vendoring 切片
恰好消除了那个约束 —— 引擎如今就在树内（`vendor/engine/`），树内修
改是受支持的流程。绕行自身的已知缺口（第二台发送引擎必须重新赢得重
试/退避、unconfirmed 语义、部分 DATA 处理与 Sent 副本纪律，而引擎提
交器早已打磨好这些）就此退役：一台发送引擎、一个发件箱、一条
drain。

**范围。** 引擎的 MIME-wrap 接缝（engine-provider trait、
provider-imap 消费）、覆盖 `apply_crypto` 的 Astra 包装器、拨号时的安
装，以及 Astra 绕行的拆除（crypto outbox 表、Astra 传输、提交尾部分
支、加密行与动词）。加密存储（密钥/信任/解析）、闸门本体、加密方法
设置与编写器旗标形态不变。

## D1：`MimeWrapper` 定义于 engine-provider，逐草稿咨询

一个放在 `hand_over.rs` 旁的 provider 中立 trait（`ConnectObserver`
先例：中立 trait、经 config 以 Arc 携带）：

```rust
pub struct MimeWrapOutcome { pub wire: Vec<u8>, pub filed: Vec<u8> }

#[async_trait]
pub trait MimeWrapper: Send + Sync {
    async fn wrap(
        &self,
        draft: &Draft,
        filed_message: Vec<u8>,
    ) -> Result<MimeWrapOutcome, ProviderError>;
}
```

引擎只在包装器已安装**且**草稿要求时（`draft.sign ||
draft.encrypt`）咨询它；其余一切发送与今天逐字节一致。包装器接收归
档组装件（带 `Bcc` 头）并返回两个变体：`filed` 是包装后的消息，
`wire` 是同一密文去掉 `Bcc` 头（每次发送只包装一次）。拒绝即
`ProviderError::permanent(detail)`：发生在任何 socket 拨号与
`hand_over.commit()` 之前，因此被拒的发送绝不抵达服务器，op 携原因
落 `Failed`，引擎既有的 Send Now（`retry_pending_op_now`）会重新组
装并重新包装。`Draft` 增加 `#[serde(default)] pub sign: bool` 与
`pub encrypt: bool`（该类型自身的既有模式；变更前序列化的 payload
以两个 `false` 反序列化）。Graph 与 Google 也组装 MIME，可在后续切
片采纳同一 trait；JMAP 发送 `Email/set` 对象，不在范围内。

## D2：provider-imap 在拨号之前消费接缝

`ImapConfig` 增加 `mime_wrapper: Option<Arc<dyn MimeWrapper>>` 与
`with_mime_wrapper` builder（仿 `with_connect_observer`），沿 `smtp`
已有的穿线路径携带（`ImapConfig -> ImapAccount::connect -> build ->
ImapProvider`）。提交准备移到共享的 `prepare_message(draft, now,
wrapper)` 助手（新 `wrap.rs`，因为 `filing.rs` 已到文件长度预算）：
包装器已安装且旗标已置 -> 组装归档变体，`wrapper.wrap`，在
`Submission` 中携带两份字节；否则 `assemble_message`，今天的行为逐
字节保持。准备**提升到 transport 匹配之前**，使拒绝在隐式 TLS、
STARTTLS 与明文三条路径上都在拨号之前发生。归档副本的选择优先取包
装输出；今天的 no-Bcc 快路与归档重组只在未包装的发送中存活。
`refile()`（`file_sent_copy` 修复路径）运行同一助手，因此
`Unfiled` 的加密发送绝不可能重新归档一份明文 Sent 副本 —— 本变更
绝不发布的最危险泄漏。

## D3：astra-crypto-store 中的 Astra 包装器

`StoreMimeWrapper`（新 `src/wrap.rs`）在移植的闸门之上实现该
trait：幂等注册账户行、由引擎 `Draft` 构建闸门的 `SendDraft`（收件
人 + 旗标）、解析默认签名密钥、按绑定的 standard 选 backend、对归
档组装件调用 `apply_crypto`。产出为 `{ wire: strip_bcc(wrapped),
filed: wrapped }`；`CryptoSendError` 变为
`ProviderError::permanent(gate_failure_words(...))`。`strip_bcc`、
`gate_failure_words` 与 `address_spec` 自 astra-app 原样移入（连测
试）。工厂 `mime_wrapper_for(store, account_id, account_email,
method)` 在方法为 `none` 时回答 `None`，在存储无法服务时回答**永
远拒绝**的包装器（失败关闭，绝不明文）。`crypto_outbox` 表在迁移
0003 中删除（0002 作为不可变历史保留；SP4b 未发布，弃队安全，
changelog 注明）。`src/outbox.rs` 与 `src/transport.rs` 连同测试删
除。

## D4：拨号时安装，切换时重拨

`connect_mail_providers`（以及迟到文件夹绑定的
`connect_imap_mailbox`）增加可选包装器参数；桌面宿主为 `[crypto]`
节命名了方法的账户构建包装器并传入，boot 时 crypto 存储在
`dial_stored` **之前**打开。`store_crypto_method` 在持久化**之前**
重拨并原位重装 providers —— 先前的 "no re-dial" 契约反转，因为包
装器在拨号时绑定。为无 `[imap]` + `[smtp]` 的账户写入方法被宿主侧
拒绝（`encryption needs an IMAP/SMTP account`）：方法存在而无接缝
可挂将以明文发送，这是本切片拒绝发布的失败关闭倒退。设置行的
`crypto_method` 回显与账户页下拉不变。

## D5：Astra 尾部收缩为一次盖印

`submit_and_drain` 盖印 `draft.with_crypto(sign, encrypt)`（由编写器
的 `CryptoFlags` 解析，启用方法时默认同开 —— 无包装器时惰性，绝不
在加密账户上默认 false），然后走不变的引擎尾。`crypto_submit_maybe`、
`CryptoSendSupport` 端口、加密发件箱行与动词、`CRYPTO_OP_NAMESPACE`
全部删除；held 行回归插件闸门专用。插件闸门继续在盖印之前审查组
合，因此它看到的正是包装器将包装的内容；held 条目保留其旗标供
Send Anyway/Retry 使用。

## D6：拒绝 UX 就是引擎自己的

信任闸拒绝是一行带逐收件人原因的引擎 `Failed` 行（队列行免费从草
稿 payload 派生主题），用户接受密钥后 Send Now 重试。其余一切不
变：暂时性失败按退避重试、DATA 中途断连保持 `unconfirmed` 及确认
对、Sent 副本由引擎自己的 APPEND 路径归档 —— 绕行曾以一半体量重
现的全部语义。

## 已记录的陷阱，不隐藏

- 旗标必须 `#[serde(default)]`：否则 SP4b 版本排入的 op 在 drain
  时解码失败（用字面旧形 JSON 测试）。
- `file_result` 的 no-Bcc 快路在包装后绝不可达；归档副本永远是包
  装器的输出。
- 准备必须提升到 transport 匹配之前（明文臂原本先拨号）；拒绝在
  结构上就是预拨号、预 commit。
- `refile()` 重新包装；`Unfiled` 加密发送的修复只归档包装件或永
  久失败，绝不明文。
- 引擎文件长度 CI（submit.rs 498、filing.rs 483）迫使先拆分再加
  行。
- boot 先开 crypto 存储再拨号；迟到绑定经 `DialConnector` 穿透包
  装器；拨号时无法构建的包装器以永远拒绝安装。
- 非 IMAP 账户的方法写入被宿主侧拒绝；已带该配置的存量配置在
  boot 记日志。
- 引擎自身 gate（nightly 钉定 fmt、clippy `-D warnings`、文件长
  度）在 `vendor/engine` 内运行，provider 变更对真实 Stalwart
  harness 实证，而非仅离线 mock。
