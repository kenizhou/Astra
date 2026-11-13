# SP4d-1b 计划：CLI 密钥动词与 sidecar 虚拟工具

规格：docs/superpowers/specs/2026-11-13-sp4d-1b-cli-key-verbs-design.md。分支
`sp4d-1b-cli-key-verbs` 自规格提交从 main 分出。每个任务：先红、实现、电池、一个
约定式提交。一个契约变更：docs/cli.md 增动词行 + gating map + 计数连同其 zh 镜像
在同一提交，capabilities 行连同其 zh 镜像移动，并落一个变更日志碎片。无引擎改
动。仅公开材料；私钥导入留桌面。

### 任务 1：五个 CLI 动词（D1）

**文件：** cli/Cargo.toml（astra-crypto-store 路径依赖 + testing dev-feature）；新
cli/src/verbs/keys.rs（五动词体 + 惰性 crypto.sqlite 打开 + 从
astra-app/src/collected.rs 复制的 promote/reject 编排）；cli/src/verbs/keys_tests.rs；
脚手架答于 verb.rs（变体 + name() + mutating()）、parse.rs（臂 + SUBVERBS）、
mod.rs（两个 execute 臂 + 模块注册）、audit.rs（arg_preview 臂）、completions.rs
（descriptor + PROBES）。

- [ ] 在钩子处确认：account_id 解析（AccountId newtype）、crypto.sqlite 路径
      （data_dir.join，5s busy timeout）、store 函数签名（import_public_cert/_as、
      list_crypto_keys_for_account、list_collected_keys_for_account、
      promote_collected_key + record_promote_trust、remove_collected_key +
      put_trust_decision）。测试先行（以临时数据目录 + 经 register_account 播种的
      crypto.sqlite + 测试主密钥驱动 run()：list/collected 答 shaped 行；import 落
      地公钥证书 + unverified trust；promote 配对 trust 写入；reject 移除 + 记录
      rejected；preview 不写任何内容；每次调用的审计行；携带秘密输入被拒指向桌
      面；FK 未注册错误；读动词带 --preview 是 usage 错误）；随后动词 + 脚手架。
      提交 `feat: the CLI key verbs`。
- [ ] cli 不豁免上限：mod.rs（468）与 parse.rs（457）为五动词留 ~30-40 行余量——
      execute 臂保持单行、crypto 打开放 keys.rs；若文件破上限，拆子模块（E0119 陷
      阱：trait impl 不可拆）。promote/reject 编排按设计写两份（CLI + app），双侧
      测试钉住（docs/cli.md）。arg_preview 仅形状，绝不携带调用方文本。既有 CLI 测
      试保持绿。

### 任务 2：sidecar 工具 + 宿主 gating + 契约（D2，D3）

**文件：** 新 sidecar/src/tools/gated.ts（从 mailTools.ts 抽取共享 runGated /
execCli / buildCliArgv / defineMailTool，释放其余量）；新
sidecar/src/tools/keyTools.ts（五工具，createKeyTools）；sidecar/src/index.ts（把
createKeyTools 接入两个 harness）；sidecar/src/tools/keyTools.test.ts +
keyToolsMap.test.ts（stub-cli 模式，keys stub 副本）；宿主
client/desktop/src/agent/mod.rs default_policy（+5 行：读 auto，写 confirm）+
permissions.rs shaped_arg_preview（+5 臂）+ policy_tests.rs（+5 行）+
permissions_tests.rs（形状）；docs/cli.md（动词表 + gating map + 计数 ten->fifteen
tools）+ zh；docs/capabilities.md（行）+ zh；
docs/changelog/unreleased/sp4d-1b-cli-key-verbs.md（Platforms: desktop，Bump:
minor）。

- [ ] 测试先行（sidecar 工具钉住精确 argv 语法 + 批准信封 level/args +
      approve/reject/abort + map pin；宿主 policy 钉住五行 + 预览形状）；随后抽取
      + 工具 + gating + 文档。提交 `feat: the key virtual tools and contract`。
- [ ] sidecar 不豁免上限：mailTools.ts（487）不能容纳工具——gated.ts 抽取必需；
      mailTools.test.ts（497）与 index.test.ts（487）接近上限，新测试放兄弟文件。
      三个 pin（policy_tests、permissions_tests、sidecar map 测试）同动。
      confirm_preview 留记录缺口（交付 confirm）。无秘密跨越此面；口令绝不为 CLI
      标志（私钥导入留桌面）。注释现在时；无 em/en 破折号；英式英语。docs/zh 镜像
      同提交。
- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；cargo
      clippy --workspace --all-targets -- -D warnings；cargo test --workspace；npm
      --prefix sidecar run build && npm --prefix sidecar test；npx tsc --noEmit &&
      npx vitest run（client/web）；cargo run -p xtask -- gate。若 plugin 测试以
      AlreadyGranted 失败，清理陈旧 /tmp/astra-desktop-plugin-* scratch 目录
      （Windows pid 回收 flake）再重跑。
- [ ] 离线电池即证明（import/promote/reject 机制已在 SP4d-1 / SP4d-2 经产品面活实
      证；CLI 级活实证是新领域，后续）。
- [ ] 分支推送；PR（预期 squash）；CI 观察；squash 合并、删分支、同步 main；ledger
      清理。

## 自审笔记

- 规格覆盖：D1 的五 CLI 动词（T1）；D2 的 sidecar 工具 + 宿主 gating + D3 的契约 +
  电池 + PR（T2）。
- 风险：紧上限（cli mod.rs/parse.rs ~30-40 余量，sidecar mailTools.ts 在上限 ->
  gated.ts 抽取）；promote/reject 编排写两份（双侧钉住）；FK 未注册边界；三个 pin
  同动；私钥导入被阻塞（keyring 规则，记录缺口）；confirm_preview 留缺口。无秘密
  跨越此面；arg_preview 仅形状。
- 不评测/不提升 pipeline（面向模型不变）。离线电池即证明。活实证教训：若电池在
  AlreadyGranted 上 flake，清理陈旧 plugin scratch 目录。