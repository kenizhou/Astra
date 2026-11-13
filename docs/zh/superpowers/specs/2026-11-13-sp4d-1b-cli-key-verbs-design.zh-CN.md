# SP4d-1b 设计：CLI 密钥动词与 sidecar 虚拟工具

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** 密钥管理桌面弧已交付（SP4d-1 导入、SP4d-2 审阅/提升），但 agent 优先
的 CLI 没有密钥动词，sidecar 没有密钥工具。本切片新增五个 CLI 动词（keys list /
collected / import / promote / reject）及匹配的 sidecar 虚拟权限工具，完成面向
agent 的密钥管理面。仅公开材料：私钥导入留桌面（vault 主密钥驻留 OS keyring，CLI
契约禁止打开 keyring；无安全口令传输）。CLI 直连 store（其精简先例），复制 app 的
promote/reject 编排并以测试钉住两份副本。无引擎改动；不评测/不提升 pipeline。

## D1：五个 CLI 动词

新 cli/src/verbs/keys.rs（knowledge.rs 形态）惰性打开 crypto.sqlite
（data_dir.join("crypto.sqlite")，与 astra.sqlite 同目录；CryptoStore::open 的 5s
busy timeout 与宿主共享文件）。CLI 获得 astra-crypto-store 路径依赖（+ testing
dev-feature）。账户解析复用 account_id（AccountId newtype）。动词：

- keys list --account（读）：两标准合并，公开视图（无秘密材料）， shaped 子集
  （fingerprint、email、standard、key_type、origin、has_private、时间戳）。
- keys collected --account（读）：暂存行，两标准，最新优先。
- keys import --account (--file | --plain | stdin) [--standard]（变更）：
  import_public_cert / _as；默认 unverified trust 自带；携带秘密的输入被 store 拒
  绝并指向桌面私钥路径。
- keys promote --account --standard --fingerprint [--email] [--verified]（变更）：
  复制 app 的 email 回退（在 promote 排空暂存前预读）+ promote_collected_key +
  record_promote_trust。trust 写入承重（promote 本身使解析器 Undecided）。
- keys reject --account --id --standard --fingerprint [--email]（变更）：
  remove_collected_key + 带对端 email 回退的 rejected 决定。

每个动词答全脚手架：Verb 变体 + name() + mutating()（仅 import/promote/reject）+
解析臂 + SUBVERBS + 两个 execute 臂（引擎打开前早返回 + unreachable 臂）+
arg_preview（仅形状，绝不携带调用方文本）+ completions descriptor + PROBES。
--json 走既有信封（无 schema 提升）；三个变更动词带 --preview（动作构建器，不写
任何内容）。FK 边界：对 app 从未注册的账户 import/promote 以清晰运行时错误失败
（CLI 无账户 email 不能自注册）；list/collected/reject 不受影响（无 FK）。

## D2：sidecar 虚拟工具 + 宿主 gating

mailTools.ts 在上限（487/500）：新 sidecar/src/tools/keyTools.ts 容纳五个工具，共
享 runGated / execCli / buildCliArgv / defineMailTool 抽取到 tools/gated.ts（释放
mailTools.ts 余量）。每个工具 runGated -> buildCliArgv(['keys', subverb], flags)。
宿主 default_policy 增五行：list/collected auto（只读），import/promote/reject
confirm（变更，needs-approval；均非 send-class，promote/reject 改变发送解析器的未
来决定，正因如此 gating 在 confirm）。shaped_arg_preview 增五个形状臂。三个 pin
同动：policy_tests.rs（精确行）、permissions_tests.rs（预览形状）、sidecar map 测
试（精确工具名 + verb/subverb）。confirm_preview 留记录缺口（如既有六个写动词般
交付 confirm；预览上卡 leg 自成一组）。

## D3：契约

docs/cli.md 增五动词行 + gating map 行 + 矩阵计数（动词数与「ten tools」->
fifteen）；capabilities.md 行移动；落变更日志碎片（Platforms: desktop，Bump:
minor）。zh 镜像同提交移动。

## 已知缺口，记录而非静默

- 私钥导入经 CLI 被阻塞：vault 主密钥驻留 OS keyring（vault.rs），CLI 契约禁止打
  开 keyring（docs/cli.md）；无安全口令传输先例。私钥导入留桌面；keys import 喂
  TSK 拒绝并指向桌面。
- confirm_preview（预览上卡 leg）留记录缺口；本切片交付 confirm。
- CLI 级门控活实证是新领域（今日无）；离线电池是本切片的证明，import/promote/
  reject 机制已在 SP4d-1 / SP4d-2 经产品面活实证。spawn CLI 活实证是后续。
- token registry（cryptoki、openpgp-card）与原生安全 UI 是更重后续。

## 测试先行

每个行为都带着它的红落地。CLI 动词测试（knowledge_tests 模式：以临时数据目录 +
经 register_account 播种的 crypto.sqlite + 测试主密钥驱动 run()；断言退出码、信封
data、preview 不写任何内容、每次调用的审计行、携带秘密输入被拒、FK 未注册错误）。
sidecar 工具测试（stub-cli 模式：keys stub 副本、精确 argv 语法、批准信封
level/args、approve/reject/abort、map pin）。三个宿主/sidecar pin 同动。离线电池即
证明。