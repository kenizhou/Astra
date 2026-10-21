# SP3c 计划：decode 接缝与插件 UI 端面

规格：docs/superpowers/specs/2026-10-21-sp3c-plugin-ui-design.md。分支
`sp3c-plugin-ui`。每个任务：先写红灯测试，再实现，跑测试组，按约定式
提交。

### 任务 1：decode 接缝（D1、D6 的 decode 半）

**文件：** crates/astra-viewmodel（ReadingSnapshot 的
`SecurityBannerView` 腿，线形钉死）；crates/astra-app（
`MessageDecoder` 端口，render_reading 中源读取与渲染之间的 decode 调
用，解出 HTML 走同一消毒流水线）；crates/astra-plugin（
`message.decode` 维持 ReadItem + 邮件句柄）；plugins/echo（decode 应
答）；client/desktop（注册表上的 `PluginDecoder`，沿用闸门超时，boot 安
装，echo 经 duplex 的 decode 测试）。

- [ ] 先写测试；提交 `feat: decode messages through the plugin seam`。

### 任务 2：描述符与调用命令（D2、D3）

**文件：** crates/astra-plugin（manifest 的 `[ui]` 表，严格解析）；
client/desktop（PluginRow 携带描述符；`plugin_call` 与
`plugin_frame_call` 命令经路由器）；client/web 的 services/plugins.ts
（拉取与调用）。

- [ ] 先写测试；提交 `feat: declare and route the plugin descriptors`。

### 任务 3：chrome 渲染器与待决行（D2、D5）

**文件：** client/web（阅读窗格的 PluginBanner；ComposerActionsRow 与
ComposeRibbon 的 PluginComposerAction；设置页的 PluginSettingsSection 分
类；OutboxQueue 的待决行及其作答/重试/撤回；ipc 类型与 intent 构造
器）。

- [ ] 先写测试；提交 `feat: render the plugin surfaces in the chrome`。

### 任务 4：frame 与 scheme（D4）

**文件：** client/desktop（astra-plugin:// scheme 处理器：按插件根目
录、拒绝穿越、CSP）；client/web（PluginFrame + pluginBridge.ts）；
tauri.conf.json 的 CSP 点名该 scheme。

- [ ] 先写测试；提交 `feat: frame the plugin's rich pages behind the scheme`。

### 任务 5：文档与收尾

**文件：** docs/plugins.md + 中文副本（UI 模型一节）；capabilities 行 +
中文副本；changelog 片段；全量测试组；推送；PR。

- [ ] 提交 `docs: record the plugin UI slice`；随后收尾。

## 自审记录

- 规格覆盖：decode（T1）、描述符与调用（T2）、chrome（T3）、frame
 （T4）、文档（T5）。每个 D 恰好落入一个任务。
- 接口链：快照腿与端口（T1）-> manifest 与命令（T2）-> 渲染器消费两者
 （T3）-> frame 扩展调用面（T4）。
- 风险：解出的 HTML 必须走完全相同的消毒流水线（不开第二条正文路
  径）；scheme 处理器绝不在插件自身目录之外解析（`..`、绝对路径与跨插
  件 id 一律拒绝）；桥必须在到达管道前拒绝未声明方法；待决行必须跟随
  快照，绝不用可能与快照不一致的本地状态。
