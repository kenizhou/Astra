# SP4b 计划：加密存储与发送闸门

规格：docs/superpowers/specs/2026-10-22-sp4b-crypto-send-design.md。从
main 切出分支 `sp4b-crypto-integration`。每个任务：先写红灯测试，再实
现，跑测试组，按约定式提交。

### 任务 1：加密存储（D1）

**文件：** crates/astra-crypto-store/（schema 照抄、SqliteKeyStore 移
植、信任决策与收件人解析、默认签名密钥、recipient_trust_failures），
并配与 kylins 同形的 MemoryKeyStore 测试夹具。

- [ ] 先写测试；提交 `feat: open the crypto store`。

### 任务 2：发送闸门（D2）

**文件：** crates/astra-crypto-core/src/apply.rs（apply_crypto 移植）、
其所需的 pgp_mime 与受保护头/autocrypt 助手、CryptoSendError 枚举；
kylins 的 apply 夹具适配 Astra 存储。

- [ ] 先写测试；提交 `feat: gate sends with apply_crypto verbatim`。

### 任务 3：发件箱与传输（D3、D4）

**文件：** crates/astra-crypto-store/src/{outbox.rs,transport.rs}（队列表
与其状态、SMTP 提交与 Sent 归档、engine-tls）；crates/astra-app（提交臂
中的加密分支、crypto-method 偏好、发送快照的加密行、提交 intent 上的
编写器 sign/encrypt 标记）；线形钉。

- [ ] 先写测试；提交 `feat: send crypto mail through the Astra outbox`。

### 任务 4：harness 冒烟与记录（D5）

**文件：** crates/astra-app/src/tests_live_harness/crypto_send.rs（生成密
钥、astra1 -> astra2 加密发送、Sent 副本归档、缺钥收件人被阻断）；
docs/plugins.md 的 Track B 更新 + 中文副本；capabilities 行 + 中文副
本；changelog 片段。

- [ ] 提交 `feat: prove the crypto send live and record it`；收尾（全量电
  池、推送、PR）。

## 自审记录

- 规格覆盖：存储（T1）、闸门（T2）、发件箱与传输与开关（T3）、冒烟与
  记录（T4）。每个 D 恰好落入一个任务。
- 接口链：存储（T1）-> 闸门消费（T2）-> 接缝驱动（T3）-> 实机证明
 （T4）。
- 风险：组装必须用引擎自己的组装器，否则线形与非加密发送不一致；闸门
  必须在任何引擎队列写入之前运行（绝不在后）；加密队列的行绝不重复
  提交（unconfirmed 的作答是唯一的重发路径）；绕行传输对无法服务的
  provider 必须失败具名，绝不静默。
