# SP4c-3c 设计：W9 密码接收流程

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** SP4c-3 的第三条腿：密码保护的 OpenPGP 邮件（SKESK）。加密操作
双向已存在（decrypt_symmetric、has_skesk、类型化的 WrongPassphrase），但
接收未接线：receive_openpgp 只做 PKESK 基于密钥的解密，故仅 SKESK 的邮件
失败即关闭（NoKey / Failed）且从不提示。本切片接线接收密码流程：检测
SKESK、呈现需要密码的裁决、用用户提供的口令解密。口令仅存于内存，绝不
记录或持久化（password.rs 先例）。无引擎改动；不评测/不提升 pipeline。

## D1：需要密码的裁决 + 密码编排器

一个新的 `ReceiveState::NeedsPassword` 臂（增量）与一个密码编排器入口
`receive_openpgp_password(store, account, message, source, passphrase)`：

- 解包后，`has_skesk(&ciphertext)` 检测密码保护邮件。当 SKESK 存在且无
  PKESK 密钥匹配时，裁决为 NeedsPassword（而非 NoKey/Failed）。其余情形
  保留失败即关闭臂。
- 密码入口调用 `decrypt_symmetric`，用与密钥路径相同的 known_signers /
  extra_signer_blobs 解析，故内嵌签名仍验证（未知签名者退化，绝不中止）。
  成功时写 ok 结果行 + 恢复受保护主题 + 返回腿。WrongPassphrase 时返回
  类型化的重新提示信号（绝不明文泄漏）。
- 需要密码状态是瞬态交互，非持久化行（decrypt_state CHECK 保持
  ok/no-key/failed/n/a）；成功写 ok。

## D2：解码器臂 + 密码提示

CryptoDecoder 把 NeedsPassword 映射为「需要密码」banner + 提示 affordance。
一个新的密码对话框（独立组件，useDialogFocus 惯用法）收集口令；一个新的
desktop 命令进程内携带它（web→desktop，crypto_wrapper 先例：密钥留在进程
内，绝不记录/持久化）并对新的 engine.message_source 重跑密码编排器，然后
刷新阅读快照。WrongPassphrase 时对话框重新提示；取消时裁决保持需要密码
（失败即关闭）。新 wire 类型落在所属特性服务，绝不在 ipc.ts（在上限）。
阅读重入口拆到新模块（reading.rs 接近上限）。

## D3：契约

docs/crypto-receive.md 新增 W9 密码流程注记（SKESK 检测、需要密码裁决、
内存口令、重新提示）并镜像到 zh。capabilities OpenPGP 行的注记命名密码
接收落地。一个变更日志碎片落地。

## 已知缺口，记录而非静默

- W9 发送侧本身未接线（apply_password_crypto 无调用方，SendDraft 无
  password_protected 字段）；活实证经 encrypt_symmetric 手工构造 SKESK
  邮件。接线 W9 发送是协调后续。
- 口令绝不持久化；重开重新提示（无缓存口令）。会话口令缓存是后续。
- 密码对话框仅限外壳（阅读窗），如其他接收面。

## 测试先行

每个行为都带着它的红落地。编排器测试（SKESK 邮件 → NeedsPassword；正确
口令 → ok + 文字 + 恢复主题；WrongPassphrase → 重新提示信号，无明文；内嵌
签名验证 / 未知签名者退化）。解码器测试（需要密码 banner）。命令测试
（口令重入口解密；取消保持需要密码）。一个门控的活 harness 实证：经
encrypt_symmetric 手工构造 SKESK 邮件、送达、打开 → 需要密码；提交口令 →
文字 + 恢复主题；错误口令 → 重新提示；撤回副本。无活模型证明；离线电池即
证明。