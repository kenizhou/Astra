# SP3b 计划：发送闸门与编写器变换

规格：docs/superpowers/specs/2026-10-21-sp3b-send-gate-design.md。在 PR
#31 合并前从 `sp3a-plugin-platform` 切出分支 `sp3b-send-gate`。每个任
务：先写红灯测试，再实现，跑测试组，按约定式提交。

### 任务 1：闸门契约（D1、D3）

**文件：** crates/astra-plugin（manifest 的 `before_send_mode`，声明
`before_send` 时必填；`SendMode` 线类型）；plugins/echo（`before_send`
的 allow 应答与变换回显）；crates/astra-app 新建的 `gate.rs`
（`BeforeSendGate` 与 `ComposeTransform` 端口、`GateComposition` 与
`GateVerdict` 类型）；crates/astra-viewmodel 的 sending.rs（`HeldSend`
行与 `SendingSnapshot.held` 腿，线形钉死）。

- [ ] 先写测试；提交 `feat: contract the send gate and its held rows`。

### 任务 2：提交接缝（D2、D4）

**文件：** crates/astra-app（submit.rs 各臂在 `submit_mail` 之前调用闸
门与变换；outbox.rs 的待决发送簿记；prompt 作答与待决重试/撤回
intent；快照发布）。

- [ ] 先写测试；提交 `feat: gate every send ahead of the queue`。

### 任务 3：桌面闸门（D5）

**文件：** client/desktop/src/plugin/gate.rs（在注册表 worker 上实现两
个端口：按插件超时与模式折叠、admin 授权检查）；boot 在激活服务插件时
安装闸门；`plugin_gate_answer` 命令；echo 经 duplex 的集成测试。

- [ ] 先写测试；提交 `feat: fold the plugin workers into the send gate`。

### 任务 4：实机证明 + 文档（D6）

**文件：** crates/astra-app/tests_live_harness（脚本化否决闸门：被阻断的
astra1 发送到不了 astra2，解除后到达）；docs/plugins.md + 中文副本（发
送闸门一节）；docs/capabilities.md 行 + 中文副本；changelog 片段。

- [ ] 提交 `feat: prove the send gate live and record it`。

### 任务 5：收尾

- [ ] 全量测试组；推送分支；提 PR（预期 squash）；台账清理。

## 自审记录

- 规格覆盖：端口与 manifest（T1）、接缝（T2）、宿主（T3）、实机与文
  档（T4）。每个 D 恰好落入一个任务。
- 接口链：契约类型（T1）-> 应用接缝消费（T2）-> 桌面闸门在真实
  worker 上实现（T3）-> 实机证明（T4）。
- 风险：闸门必须在草稿构建之后（变换改写的是草稿）且入队之前（绝不在
  之后）运行；prompt 作答必须回到同一提交尾路而非另起一路；待决行绝
  不得在其草稿被丢弃后存活；echo 集成不得泄漏 spawn 的进程（duplex
  spawn 由构造保证在进程内）；live 否决必须在 harness 上失败关闭（闸
  门出错即阻断，绝不放行）。
