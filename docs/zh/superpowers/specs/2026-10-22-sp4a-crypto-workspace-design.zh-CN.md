# SP4a 设计：加密工作区，整体移植

**状态。** 2026-10-22 已定稿。从 main 切出 `sp4a-crypto-core`。用户指令
2026-10-22：完整移植 kylins 的加密功能，前后端全含。本切片是地基：
`kylins.client.crypto` 整个工作区迁入 Astra 工作区并保持全绿。Astra 侧
集成（存储、发送闸门、读取流水线）是 SP4b；原生安全 UI 是 SP4c。

**范围。** `crypto-core`、`crypto-openpgp`、`crypto-smime` 及其构建机制：
重命名为 `crates/` 下的 `astra-crypto-core`、`astra-crypto-openpgp`、
`astra-crypto-smime`，保留 kylins 的逐文件布局，使未来与上游的 diff 始终
可审阅。已发布的 `cms = 0.3.0-rc.0` 取代 vendored `vendor/cms`（验证见
下）。kylins 测试套件全部随行且必须在树内通过。不在本切片：具体存储、
发送闸门、解码流水线、令牌注册表与任何 UI（SP4b/SP4c）。

## D1：逐文件迁入，仅改 crate 名

三个 crate 复制为 `crates/astra-crypto-{core,openpgp,smime}`，源码与
kylins 原文件逐字节一致，除：包名与工作区内的路径依赖
（`crypto-core` -> `astra-crypto-core`）、`KylinsSmimeProfile` 与
`KylinsCrlChecker` 的重命名（改为 `AstraSmimeProfile` /
`AstraCrlChecker`，含 profile 名称字符串）、"Kylins crypto framework"
的 crate 文档、以及 `Standard::Sm` 桩（按上游现状保留，保持 diff 的机械
性）。不改任何逻辑：从相同源码编译通过的移植本身即证明保真，232 个上
游测试即回归电池。

## D2：保留 vendored cms 快照，已发布的 rc.0 经评估并具名否决

用户问是否有更新的已发布 `cms` 可替代 vendored 快照。已评估并否决，原
因具名：`cms 0.3.0-rc.0` 能对照当前 cipher 0.5 / elliptic-curve 0.14 线
编译且 builder API 相符，但其 `builder/kari.rs` 从不把 ANSI X9.63 KDF 的
输出落到实际用于包裹的 KEK 缓冲区，所有 ECC 收件人往返都过不了 AES-KW
完整性校验（`decrypt_round_trips_ecc_recipient`、
`token_decrypt_ecc_kari_round_trips`、openssl kari 互操作）；formats 主
分支同样缺失该修复，任何已发布或前沿构建都没有快照所携带的修复。与此
无关的另一约束：`pkcs12 0.2.0-pre.0`（唯一现代发布版，由
`p12-keystore` 引入）精确钉定 `cms =0.3.0-pre.1`，而 Cargo 会统一同一
版本线的预发布标签，任何注册表版本都不可能共存；只有路径包才能分叉。
因此保留 vendored 快照（带 KEK 修复的 post-pre.2 formats 快照）于
`crates/astra-crypto-smime/vendor/cms`，照用 kylins 的绕行方式。
`pkcs1`/`rsa` 精确钉定（`=0.8.0-rc.4`、`=0.10.0-rc.18`）：预发布 caret
范围今天会解析到更新的版本并打破快照的调用点（`RsaPssParams` 在 pkcs1
rc.5 改为泛型）。

## D3：500 行上限对上游迁入 crate 豁免

Astra 的文件长度门限把每个受跟踪 Rust 文件限制在 500 行；kylins 文件最
大约 3000 行（`smime/src/lib.rs`）。拆分它们会毁掉本移植全部的安全依据
——可审阅的 diff。xtask 的 file-length 检查新增显式豁免清单，覆盖
`crates/astra-crypto-{core,openpgp,smime}/`（及未来的 vendored 树），并
在检查中注明：为与来源保持可 diff 而迁入的上游代码，在本切片原生改写
之前豁免。其余门限（破折号卫生、许可头、英式拼写、clippy、fmt）照常适
用；迁入文件若触发上游从未有过的仓库 lint（注释中的破折号、fmt 差
异），调整该文件并记录偏差，而非盲目豁免。

## D4：依赖行照抄 kylins 清单，已缓存且钉定

依赖行照抄 kylins 清单（sequoia-openpgp 2.4.1 及其两个强制 allow 标
记、pkix-* 家族、五个别名双栈依赖、`rsa 0.10.0-rc.19`、
`p12-keystore 0.3.1`）。本机 cargo 缓存已全部具备（2026-10-22 验证）；
无 git 钉定、无 crates.io 之外的来源。工作区级 `[lints] workspace =
true` 适用于三个 crate；上游从未面对的 pedantic clippy 警告在模块根以
注明 kylins 来源的 `#[allow]` 处理，或按文件限定豁免范围，使原生代码保
持严格。

## D5：电池证明保真，而非仅证明可编译

当整个 kylins 测试电池在树内通过时本移植才算完成：crypto-core 的 25、
crypto-openpgp 的 85 + 13 round-trip + 7 个 gpg/sq 互操作（无 CLI 时静默
跳过）、crypto-smime 的 102（含 7 个 openssl 互操作，沿用上游的
Git-for-Windows 回退路径）。调试构建下的卡片加密较慢（openpgp lib 约 6
分钟），那是上游成本而非回归。`cargo test --workspace`、clippy、fmt 与
全部门限在豁免就位后全部通过。

## 已记录的已知缺口，不隐藏

- 别名双栈依赖（x509-cert 0.2/0.3、der 0.7/0.8、spki 0.7/0.8、
  signature 2/3、pkcs1 0.7/0.8）照抄；`chain.rs` 的 DER 重解析桥接上
  游既有，直到 pkix-* 家族切到 0.3 线。
- `Standard::Sm` 桩为保持 diff 保真按上游现状保留。
- 测试中 Git-for-Windows 的 openssl/gpg 回退路径保留（可跳过的便利，
  对其他平台无害）。
- 文件长度豁免是有意且记录在案的范围裁切；未来切片若原生改写这些文
  件，豁免随之收窄。
