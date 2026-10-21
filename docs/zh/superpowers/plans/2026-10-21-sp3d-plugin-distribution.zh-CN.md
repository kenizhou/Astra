# SP3d 计划：插件分发

规格：docs/superpowers/specs/2026-10-21-sp3d-plugin-distribution-design.md。
从 main 切出分支 `sp3d-plugin-distribution`。每个任务：先写红灯测试，再
实现，跑测试组，按约定式提交。

### 任务 1：包（D1、D2）

**文件：** crates/astra-plugin/src/bundle.rs（tar 构建/读取、规范摘要、
经 ring 的 ed25519 签名/验证、manifest 的 `publisher` 与
`privacy_policy_url` 字段）；xtask 的 `bundle-plugin` 命令（构建并签名一
个目录）；echo 插件的包夹具（测试内用生成的密钥构建）。

- [ ] 先写测试；提交 `feat: sign and verify plugin bundles`。

### 任务 2：持久化与生产门（D4、D6）

**文件：** client/desktop（注册表 grants.toml 的写穿与启动读取、启动时
兑现持久化的启用标记、非开发版对 proposed 方法的拒绝）。

- [ ] 先写测试；提交 `feat: persist plugin grants and gate proposed methods`。

### 任务 3：安装、同意与策略开关（D2、D3、D5）

**文件：** client/desktop（`plugin_install_offer` 与 `plugin_install`
命令、解包、在 enable 与 serving/call 接缝查询的策略开关）；
crates/astra-app（设置快照上的 `plugins_enabled` 偏好）；client/web
（Plugins 页的安装按钮与开关、同意对话框）。

- [ ] 先写测试；提交 `feat: install plugins behind consent and the policy switch`。

### 任务 4：文档与收尾（D7）

**文件：** docs/plugins.md + 中文副本（分发一节）；docs/capabilities.md
行 + 中文副本；changelog 片段；全量测试组；推送；PR。

- [ ] 提交 `docs: record the plugin distribution slice`；随后收尾。

## 自审记录

- 规格覆盖：包（T1）、持久化与生产门（T2）、流程（T3）、文档（T4）。
  每个 D 恰好落入一个任务。
- 接口链：包格式与验证（T1）-> 注册表持久化（T2）-> 安装与策略流程消
  费两者（T3）。
- 风险：摘要必须规范（排序、带路径标签、排除签名），否则验证形同虚
  设；解包绝不写出受管目录之外（SP3c scheme 处理器的穿越纪律适用于
  tar 条目名）；manifest id 与目录不符的包必须拒绝；worker 如今失败的
  已启用插件绝不能使启动失败。
