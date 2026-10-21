# SP3c 设计：decode 接缝与插件 UI 端面

**状态。** 2026-10-21 已定稿。从 main 切出 `sp3c-plugin-ui`（SP3a 已作为
#31 合并；SP3b 为在审的 #32；本分支携带其中尚未合并的部分）。来源：已批
准插件计划的 Track A 阶段 3，加上 `message.decode`（随其供给的横幅从
SP3b 移至本切片）与发件箱待决行 UI（SP3b 的端面欠账：快照腿已交付但尚
无渲染器）。

**范围。** 落地四个端面：阅读流水线的 decode 接缝及其安全横幅；
manifest 声明的 UI 描述符（横幅槽位、编写器动作、设置分区）由 Astra
chrome 渲染；富页面所在的沙箱 PluginFrame（`astra-plugin://` scheme 加窄
桥）；以及发件箱面板的待决发送行及其作答、重试与撤回。不在本切片：签
名包与安装对话框（SP3d）、由 astra-app 喂入的事件流、以及插件到宿主的
调用方向（仍无消费者；见 D3）。

## D1：decode 在 render_reading 中运行，失败开放，其 HTML 与其他正文一样被消毒

当有已激活插件服务 `message.decode` 时，阅读流水线在 resolve 之后读取原
始源（`engine.message_source`）并交给 decoder 端口（`MessageDecoder`，
桌面端 `PluginDecoder` 在注册表上实现，沿用闸门的逐次调用超时）。结果的
部件替换渲染腿：解出的 HTML 腿经过与所有其他正文完全相同的消毒 / 内联
cid / 远程门控流水线，解出的纯文本腿即文本视图。结果的横幅随阅读快照新
增的 `security_banner` 腿（viewmodel 类型：level info/warning/error 加
text；插件 crate 的 `SecurityBanner` 在宿主接缝映射）。无 decoder、出错
或超时：维持今天的渲染，无横幅。decode 只在插件的 `read_item` 授权下看
到原始字节，调用的邮件句柄由宿主铸造；不作答的插件改变不了任何东西。

## D2：描述符由 manifest 声明，随插件行呈现

manifest 新增可选的 `[ui]` 表，与其余字段一样严格解析：
`composer_actions`（`id`、`label`、`icon`）、`settings_sections`
（`id`、`title`，字段为 `toggle` / `text` / `select` / `password` /
`action`）、`pages`（`id`、`title`、`entry`）。桌面端的 `PluginRow` 把
它们带给 web，由 Astra chrome 渲染：编写器动作是 `ComposerActionsRow` /
`ComposeRibbon` 中的按钮，设置分区是设置页的一个分类，横幅槽位是阅读窗
格的 `PluginBanner`（其内容来自 decode 横幅或插件事件，绝不是 manifest
里的静态文本）。

## D3：描述符作答是自定义命名空间调用，即发即确认

编写器动作点击或设置字段变更即一次 `plugin_call` 命令：宿主经路由器调
用 worker 已声明的自定义方法（`plugin.<id>.<action>`、
`plugin.<id>.set_setting`，与任何调用一样需声明且授权），并把插件的结
果回给 webview。结果可携带新的描述符状态（例如 Encrypt 开关的
pressed），动作的效果以此呈现：插件到宿主方向（worker 在调用中改写草
稿）仍属已记录的缺口，待有消费者时落地；本切片的动作诚实地只做单向。

## D4：富页面经 astra-plugin:// 住在 PluginFrame 中，桥窄而类型化

桌面端注册 `astra-plugin` URI scheme：`astra-plugin://<plugin-id>/<path>`
只在该插件自己的目录内解析（拒绝穿越与跨插件读取，缺失文件 404，HTML
以不允许任何远程内容的 CSP 提供）。web 的 `PluginFrame` 在沙箱 iframe
中渲染页面，并经 `postMessage` envelope（`{ id, method, args }`，镜像
`object_call`/`object_result`）通过新的 `plugin_frame_call` 命令与
worker 往返。插件 JS 绝不在主 webview 运行；frame 即其全部触达范围。

## D5：发件箱面板渲染待决腿

`OutboxQueue` 在队列下方新增待决行：`checking` 显示所等待的插件，
`needs_answer` 显示原因及 Send Anyway / Deny（派发 `AnswerGate`），
`blocked` 显示原因及 Retry / Withdraw（`RetryHeld` / `WithdrawHeld`）。
web ipc 层新增待决类型与三个 intent 构造器。

## D6：证明

单元红灯钉住：decode 接缝（部件替换、HTML 被消毒、横幅随快照、无
decoder 零开销、错误失败开放）、manifest 的 `[ui]` 解析（严格、可选）、
scheme 处理器（拒绝穿越、拒绝跨插件、缺失 404、CSP 在位）、桥 envelope
（未声明方法在到达管道前被拒）。vitest 钉住每个渲染器（横幅级别、动作
点击派发、设置字段种类、待决行三态）。echo 插件新增 `message.decode`
应答（原始源回显为纯文本部件，无横幅），桌面 decode 测试经 duplex 驱动
它。阅读流水线的实机证明即既有的 harness 阅读套件：未安装 decoder 时，
decode 接缝保持逐字节不变。

## 已记录的已知缺口，不隐藏

- 插件到宿主的调用方向（worker 主动读取或编辑）尚无 envelope；本切片
  的端面全部由宿主发起，编写器动作只确认不编辑。
- 授权持久化、安装/同意对话框与签名包属 SP3d；在此之前插件仅经
  `ASTRA_PLUGIN_DEV` 进入。
- `ui.banner` 的运行时内容是 decode 横幅；事件驱动的横幅
  （plugin_event）随事件流落地。
- frame 的桥只允许已声明的自定义方法；固定对象树（读取、编辑）随插件
  到宿主方向到来。
