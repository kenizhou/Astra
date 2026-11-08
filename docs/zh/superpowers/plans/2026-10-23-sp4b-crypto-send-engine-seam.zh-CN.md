# SP4b-接缝计划：加密闸门移入引擎

规格：docs/superpowers/specs/2026-10-23-sp4b-crypto-send-engine-seam-design.md。
从 main 切出 `engine-crypto-seam` 分支。引擎 delta 先行以保持树始终
可编译；行为变化处先写红灯；约定式提交；末尾一个 squash PR（不是
vendor-sync PR，squash 没问题）。

### 任务 1：引擎接缝（D1、D2）

**文件（引擎）：** `engine-provider/src/draft_attachment.rs`（拆
分）、`submit.rs`（旗标 + 再导出）、`mime_wrap.rs`（新建）、
`lib.rs`；`engine-api/src/lib.rs`（再导出）；
`provider-imap/src/{config.rs, account.rs, provider.rs}`（穿线）、
`wrap.rs`（新建：`PreparedMessage` + `prepare_message` +
`filed_bytes`）、`filing.rs`（async 提升的 `prepare`、
`Submission.filed`、`file_result`、`refile`）；引擎
`docs/agent-guidance/{imap-smtp,providers,store-and-sync}.md`。

- [ ] 先写引擎测试（Draft serde 双向兼容、脚本化包装器：仅置旗标时
  包装、wire 与 filed 仅差 Bcc、永久拒绝在三种 transport 上都预拨
  号、refile 重新包装）；引擎电池 + 其 gate + stalwart-live。提交
  `engine: add the MIME-wrap seam for outbound crypto`。

### 任务 2：Astra 包装器与绕行拆除（D3、D5）

**文件（存储）：** `src/wrap.rs`（新建，含 `wrap_tests.rs`）、
`src/migrations/0003_drop_crypto_outbox.sql`、`src/lib.rs`、
`src/store.rs`、`Cargo.toml`；删除：`src/outbox.rs`、
`src/transport.rs` 及其测试。

**文件（app）：** `crypto.rs`（瘦身为 `CryptoFlags`）、
`crypto_outbox.rs`（删除）、`lib.rs`、`submit.rs`（盖印）、
`outbox.rs`（删加密路由）、`crypto_tests.rs`（重写）、
`settings/accounts.rs` + fakes + settings_proof（trait 签名）。

- [ ] 先写存储测试（strip_bcc、gate_failure_words、address_spec、
  工厂的 None/永远拒绝、端到端包装、带信任措辞的永久拒绝）；app 接
  缝测试。提交 `feat: wrap sends in the engine, retire the bypass`。

### 任务 3：宿主安装（D4）

**文件：** `client/desktop/src/crypto_wrapper.rs`（新建，取代
`crypto_send.rs` 及测试）、`boot.rs`（先开存储再拨号、AppState 字
段）、`boot/stored.rs`（dial_stored + DialConnector）、
`commands.rs`、`headless.rs`、`src/ai/live_*/harness.rs`、
`account_ops.rs`（重拨 + 非 IMAP 拒绝）。

- [ ] 桌面测试（包装器解析；store_crypto_method 重拨与失败关闭拒
  绝）。提交 `feat: install the crypto wrapper at dial`。

### 任务 4：harness 冒烟与记录（D6）

**文件：** `tests_live_harness/crypto_send.rs`（改写为引擎路径：密
封送达、Sent 由引擎归档、收件人解密、缺钥收件人落 Failed 且带信任
原因、服务器端零接收）；文档：`docs/plugins.md` Track B、
`docs/capabilities.md` 行、`docs/changelog/unreleased/sp4b-crypto-send.md`
（原地修订）、`docs/sending.md`/`docs/settings.md`（核查）、
`docs/upstream-sync.md`（delta 叙述），各带中文镜像。

- [ ] harness 串行绿；全量电池、clippy、xtask gate、引擎电池绿；
  提交 `feat: prove the engine-seamed crypto send live and record it`；
  推送、PR、squash 合并。

## 自审记录

- 规格覆盖：D1+D2 落入任务 1，D3+D5 落入任务 2，D4 落入任务 3，
  D6 落入任务 4；拆除随任务 2-3 进行使每步可编译。
- 接口链：trait（T1）是存储实现（T2）与宿主安装（T3）的对象；实
  机证明闭环（T4）。
- 风险：盖印在加密账户上绝不默认 false；`refile()` 必须重新包装；
  拆分先于一切新增引擎行；结束时 `git diff vendor-sync main --
  vendor/engine` 恰好显示本切片的引擎改动。
