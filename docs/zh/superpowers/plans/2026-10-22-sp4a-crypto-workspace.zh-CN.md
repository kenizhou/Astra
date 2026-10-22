# SP4a 计划：加密工作区，整体移植

规格：docs/superpowers/specs/2026-10-22-sp4a-crypto-workspace-design.md。
从 main 切出分支 `sp4a-crypto-core`。每个任务：先机械移植，再跑电池，
按 crate 家族各一次约定式提交。

### 任务 1：工作区骨架与 core crate（D1、D3、D4）

**文件：** crates/astra-crypto-core/（逐字节源码、包名与内部依赖重命
名、crate 文档改名）；xtask 文件长度豁免清单；Cargo.lock 刷新。

- [ ] `cargo test -p astra-crypto-core` 绿；提交
  `feat: vendor the crypto core crate from kylins`。

### 任务 2：OpenPGP（D1、D4、D5）

**文件：** crates/astra-crypto-openpgp/（源码与测试、sequoia 标记、core
路径依赖改名）。

- [ ] lib + round_trip + interop（无 gpg/sq 时跳过）绿；提交
  `feat: vendor the openpgp crypto crate from kylins`。

### 任务 3：S/MIME 配已发布 cms（D1、D2、D4、D5）

**文件：** crates/astra-crypto-smime/（源码与测试；以
`cms = 0.3.0-rc.0` 取代路径依赖；双栈别名；profile 重命名）。

- [ ] lib（CLI 在时含 openssl 互操作）绿；提交
  `feat: vendor the smime crypto crate from kylins`。

### 任务 4：仓库规则与记录

**文件：** fmt/clippy/破折号卫生的调整（记录在案的偏差）；
docs/plugins.md 的 Track B 注记 + 中文副本；changelog 片段；
capabilities 行 + 中文副本。

- [ ] 全部门限绿；提交 `docs: record the crypto workspace port`。

### 任务 5：收尾

- [ ] 全量电池（`cargo test --workspace`，web 不动）；推送分支；PR。

## 自审记录

- 规格覆盖：core（T1）、openpgp（T2）、smime + cms 换装（T3）、规则与记
  录（T4）。每个 D 恰好落入一个任务。
- 接口链：豁免（T1）-> crate 落地（T1-T3）-> 门限接受（T4）。
- 风险：cms 换装是唯一的非机械变更；若任一 kylins 测试对 0.3.0-rc.0 失
  败，带回 vendored 副本并在 D2 记录失败调用点。豁免必须具名目录，绝不
  以通配覆盖整树。openpgp lib 电池在调试下很慢（上游约 6 分钟），不得
  误判为挂死。
