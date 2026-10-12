> 非规范性译本，仅供审阅；以英文版为准。

# SP2j 设置实施计划

> **给代理工作者：** 必须使用 superpowers:subagent-driven-development（推荐）
> 或 superpowers:executing-plans 按任务逐条执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 端到端交付设置：带快照的 Settings 表面、持久化进偏好文档的意
图族、核心对其设置项的消费（时区、第一天、视界、默认日历）、带 dav 编
辑器与移除账户的账户页、About、dav 编辑器的实测证明、以及契约文档。

**架构：** astra-app 拥有 SettingsIntent 分发与持久化；偏好文档是唯一存
储（TOML，只增）；账户页读取宿主的账户配置；web 渲染屏幕。
**技术栈：** 现有工作区；无新依赖。
**规范：** `docs/superpowers/specs/2026-10-12-sp2j-settings-design.md`

## 全局约束

- 各切片通用的内部规则：SPDX、500 行上限、deny-everything lint、提交前跑
  gate（直接退出码）、约定式提交、无破折号、英式英语、zh 镜像同提交、
  线协议只增。
- 常设指令：PR 合并即自动继续；SQUASH AND MERGE；10 分钟作者窗口。
- 核心不消费的设置属于宿主 chrome；核心消费的经应用状态流动，绝无第二
  来源。

---

### 任务 1：类型 + 持久化
**文件：** `crates/astra-app/src/protocol/intent_settings.rs` + 测试；
`crates/astra-app/src/preferences.rs`（+ 测试；设置字段，只增带默认）；
`crates/astra-viewmodel/src/settings.rs` + 测试（SettingsSnapshot 形态：
设置块、账户行、About 块）。
- [ ] 先写测试；实现；提交 `feat: add the settings protocol and preferences fields`。

### 任务 2：分发 + 消费
**文件：** `crates/astra-app/src/settings/{mod.rs, accounts.rs}` + 测试
（经持久化偏好的分发臂；日历的时区/第一天/视界/默认日历消费；view_mode
归并；dav 编辑器的重拨安装与失败保留旧腿契约；移除账户）。
- [ ] 按 D2-D4 单元清单先写测试；实现；提交
  `feat: dispatch settings intents and consume them`。

### 任务 3：宿主/ipc
**文件：** `client/desktop/src/commands/settings.rs`（+ 测试；设置拉取、
About 块、时区校验缝）；`client/web/src/services/settings.ts`（+ 测试 +
双语言钉住的共享夹具）。
- [ ] 先写测试；实现；提交 `feat: host the settings commands and wire`。

### 任务 4：UI
**文件：** `client/web/src/features/settings/**`（模态屏幕、源列表、
General/Calendar/Reading/Composing/Accounts/About 详情、选择器、dav 编
辑器、移除确认）+ 测试；TitleBar 入口；chrome 的主题/时钟消费。
- [ ] 按 D1/D4/D6 UI 清单先写测试；实现；提交
  `feat: render the settings screen and the accounts page`。

### 任务 5：实测 + 文档
**文件：** 门控的 live 设置测试（对 harness 的 dav 编辑器：编辑、重拨、
两腿安装、日历与联系人填充；失败重拨保留旧腿）；`docs/settings.md` +
zh；capabilities + zh；fragment；全量电池。
- [ ] 提交 `feat: prove the dav editor live and record the contract`。

### 任务 6：收尾
- [ ] 全量电池；推送分支 `sp2j-settings`；PR（预期 squash）；验收证据；
  分类账清理。

## 自评注记

- 规范覆盖：D1（T1 类型/T4 UI）、D2（T1/T2）、D3（T2）、D4（T2/T4）、
  D5（T1/T3/T4）、D6（T2/T4）、实测 + 文档（T5）。
- 接口链：类型 + 持久化（T1）-> 分发/消费（T2）-> 宿主（T3）-> UI
  （T4）-> 实测 + 文档（T5）。
- 风险：偏好文件格式（只增带默认；旧文件必须干净加载——钉住）；dav 重拨
  触碰运行中的账户状态（失败保留旧腿的契约绝不可拆掉能用的腿）；
  view_mode 归并（既有列表臂与其线协议不可破坏——只增委托）；移除账户
  的拆除广度（存储、腿、偏好的账户条目、凭据库）。
