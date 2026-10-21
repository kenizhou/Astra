# SP3a 计划：插件平台的契约与 worker 宿主

规格：docs/superpowers/specs/2026-10-21-sp3a-plugin-platform-design.md。
从 main 切出分支 `sp3a-plugin-platform`。每个任务：先写红灯测试，再实
现，跑测试组，按约定式提交。

### 任务 1：契约（D1、D2、D6）

**文件：** crates/astra-plugin/src（envelope 家族在
`object_model/envelope.rs`；句柄类型及其句柄表在
`object_model/refs.rs`；快照结构在 `object_model/snapshots.rs`；方法注
册表在 `object_model/methods.rs`；manifest 以 `methods` 字段取代
`hooks`；扩展后的 Permission 表；hook_api.rs 瘦身为 v1 payload 类型；
protocol.rs 及其测试删除）。

- [ ] 先写测试；提交 `feat: contract the plugin platform's object model`。

### 任务 2：路由器（D3）

**文件：** crates/astra-plugin/src/router.rs（三道闸：manifest 已声明、
权限已授予、句柄在作用域内；类型化拒绝枚举；proposed 方法的开发版门
控）。

- [ ] 先写测试；提交 `feat: route plugin calls through the three gates`。

### 任务 3：worker 宿主与 echo 插件（D4、D5）

**文件：** crates/astra-plugin/src/worker.rs（spawn、JSONL 泵、带代际
号的监督机、init 超时、重启预算、fail-closed 排空）；plugins/echo/
（符合性二进制）。

- [ ] 先写测试；提交 `feat: host worker plugins with sidecar discipline`。

### 任务 4：桌面注册表（D7）

**文件：** client/desktop/src/plugin/mod.rs（受管理状态：发现、授权
表、worker 宿主句柄）以及在 lib.rs 注册的精简命令集（list / enable /
disable / approve）。

- [ ] 先写测试；提交 `feat: hold the plugin registry in the desktop host`。

### 任务 5：文档与记录

**文件：** docs/capabilities.md 行 + 中文副本；changelog 片段；新建
docs/plugins.md（契约：envelope、句柄、权限、监督）+ 中文副本。

- [ ] 提交 `docs: record the plugin platform slice`。

### 任务 6：收尾

- [ ] 全量测试组；推送分支；提 PR（预期 squash）；台账清理。

## 自审记录

- 规格覆盖：契约（T1）、路由器（T2）、worker + echo（T3）、注册表
 （T4）、文档（T5）。每个 D 恰好落入一个任务。
- 接口链：envelope 与句柄（T1）-> 路由器（T2）-> worker 与 echo 插件
  说 envelope（T3）-> 宿主管理整体（T4）。
- 风险：manifest 的 hooks 到 methods 切换必须让旧 manifest 响亮地失
  败，绝不静默接受；句柄表不得跨代际泄漏条目（worker 死亡时排空）；
  监督移植必须保持 agent supervisor 的 drop 顺序（AppState 的 drop 顺
  序是关键约束）；worker.rs 通过把泵与状态机拆进兄弟模块来守住 500
  行上限。
