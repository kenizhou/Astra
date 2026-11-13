# SP4d-1 计划：密钥导入核心

规格：docs/superpowers/specs/2026-11-13-sp4d-1-key-import-design.md。分支
`sp4d-1-key-import` 自规格提交从 main 分出。每个任务：先红、实现、电池、一个约
定式提交。一个契约变更：crypto-receive.md 新增密钥导入注记连同其 zh 镜像在同一
提交，capabilities 行连同其 zh 镜像移动，并落一个变更日志碎片。无引擎改动。CLI
密钥 verb + sidecar 虚拟工具是 SP4d-1b。

### 任务 1：store 导入函数（D1）

**文件：** 新 crates/astra-crypto-store/src/import.rs（import_public_cert 基于
astra-crypto-openpgp parse_certs + fingerprint_of + cert_to_public_blob，仅公钥
upsert_crypto_key key_type:'public' origin:'imported' private_data:None；S/MIME
对齐经 fingerprint_of_cert_der / cert_summary_of_der key_type:'cert'；共享 trust
写入助手基于 trust.rs put_trust_decision）；store.rs 模块接线 + 测试。私钥路径
复用既有 backend import_key（不重造）。

- [ ] 测试先行（import_public_cert 往返一个公钥证书且 has_private 为假；畸形输
      入不暂存任何内容且绝不 panic；提取 fingerprint/user-id；公钥路径绝不置私
      钥 blob；跨标准导入被拒；trust 写入助手记录决策）；随后导入函数 + 助手。
      提交 `feat: the public-cert import store path`。
- [ ] 加密 crate 豁免上限。绝不走 backend import_key / put_sync（has_private 污
      染）。既有 store 测试保持绿。

### 任务 2：app 方法 + desktop 命令 + web 导入面 + 契约（D2，D3）

**文件：** 新 crates/astra-app/src/keys.rs 子模块（impl App 块：import_key 按密
钥材料分派公钥与私钥、list_keys；reading_password.rs 形态，lib.rs 在上限不增
长）；client/desktop/src/commands/crypto.rs 新 desktop 命令（进程内
bytes/passphrase，submit_reading_password 先例）在 client/desktop/src/lib.rs 注
册；新 client/web/src/features/keys/ 导入组件（文件/粘贴）+ wire 类型在
services/crypto.ts（绝不在 ipc.ts）接入设置栏；docs/crypto-receive.md（密钥导入
注记）+ zh；docs/capabilities.md（密钥行）+ zh；
docs/changelog/unreleased/sp4d-1-key-import.md（Platforms: desktop，Bump:
minor）；crates/astra-app/src/tests_live_harness/crypto_key_import.rs（门控活实
证：经产品面导入对端公钥证书 + 自己的私钥，接收一封加密+签名邮件可解密并验证，
无编程播种；撤回副本）+ tests_live_harness.rs 注册。

- [ ] 在钩子处确认：import_key 正确分派公钥（无密钥）与私钥（密钥 + 口令）；私
      钥路径复用 backend import_key；list_keys 返回账户行。测试先行（导入分派公
      钥与私钥；list 返回行；口令绝不泄漏；离线跳过的门控活实证）；随后 app 方
      法 + 命令 + web 导入面 + 文档。提交
      `feat: the key import surface and contract`。
- [ ] astra-app/client/desktop/web 不豁免：新 core 逻辑在新子模块
      （lib.rs/commands.rs 在上限，不增长；commands.rs 中加新 mod 行会破上限，
      命令子模块从 crypto.rs 声明）；wire 类型在 services/crypto.ts（绝不在
      ipc.ts）。口令留在内存，零化，绝不记录/持久化。注释现在时；无 em/en 破折
      号；英式英语。docs/zh 镜像同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。若 plugin 测试以 AlreadyGranted 失败，清理陈旧
      /tmp/astra-desktop-plugin-* scratch 目录（Windows pid 回收 flake）再重跑。
- [ ] 控制者在干净 harness 上武装活实证（scripts/dev/harness.sh reset，然后
      up + env；cargo test -p astra-app
      tests_live_harness::crypto_key_import -- --test-threads=1），随后 unset。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步 main；
      ledger 清理。

## 自审笔记

- 规格覆盖：D1 的导入函数 + trust 助手（T1）；D2 的 app 方法 + 命令 + web 导入
  面 + D3 的契约 + 活实证 + 电池 + PR（T2）。
- 风险：has_private 污染（公钥路径须用 upsert_crypto_key private_data:None，绝
  不 backend import_key/put_sync）；私钥路径复用既有 backend import_key；口令绝
  不泄漏；lib.rs/commands.rs 在上限（新子模块，不增长）；ipc.ts 在上限（wire 类
  型在 crypto.ts）。trust 写入助手被 SP4d-2 的 promote 复用。
- 不评测/不提升 pipeline（面向模型不变）。CLI 密钥 verb + sidecar 虚拟工具是
  SP4d-1b。活实证教训：先 reset harness；若电池在 AlreadyGranted 上 flake，清理
  陈旧 plugin scratch 目录。