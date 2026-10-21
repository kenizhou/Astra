# SP3a 设计：插件平台的契约与 worker 宿主

**状态。** 2026-10-21 已定稿。来源：已批准的插件计划（Outlook 风格的
Astra 对象模型，经 /plan-ceo-review 与 /plan-eng-review 评审；加密已决
定为内置而非插件）。从 main 切出分支 `sp3a-plugin-platform`。

**范围。** 已批准计划的 Track A 阶段 1：取代临时 `protocol.rs` 的版本化
envelope 家族；宿主签发的句柄类型及其句柄表；按 manifest、授权与句柄
作用域逐一校验每次调用的路由器；为每个插件 spawn 并监督一个进程的
worker 宿主；作为协议符合性夹具的 echo 桩插件；以及桌面宿主受管理的
插件状态与其精简命令集。权限表在本切片即扩展为评审后的七级，因为后续
每个切片都要据此校验。不在本切片：发送闸门与 `before_send`（SP3b）、
UI 描述符与 iframe（SP3c）、签名包与安装对话框（SP3d），以及整个
Track B（内置 S/MIME + OpenPGP，独立的切片系列）。

## D1：envelope 家族取代 protocol.rs

每条消息一个 JSON 对象加一个换行，`type` 标签、snake_case、stderr 用
于诊断：完全复用 sidecar 的纪律。宿主到插件：`plugin_init`（manifest、
已授权限、api_level、AOM 版本、worker 私有 data_dir）、`object_call`
（id、句柄、方法、参数）、`object_subscribe`、`plugin_shutdown`。插件到
宿主：`plugin_ready`（api_level、AOM 版本、所服务的方法）、
`object_result`、`object_error`、`object_event`、`permission_request`、
`plugin_event`。临时的 `protocol.rs`（PluginInit/HookCall）被删除而非并
行保留；hook_api v1 的两个钩子变成方法名 `compose.transform` 与
`message.decode`，其请求/响应类型保留为 SP3b 要接线的 payload 模式。

## D2：宿主签发句柄，绝不接受客户端拼装的 key

插件指名的每个对象都是宿主铸造的不透明句柄：`MessageRef`、
`FolderRef`、`AccountRef`、`DraftRef`，以及后续切片需要的其余对象树。
句柄绑定插件 id + 账户 + supervisor 代际，并随代际过期；路由器只按自
己的句柄表解析，拒绝任何非宿主铸造的引用。原始引擎 key
（`imap:v1:u1`）绝不跨越边界。类型化拒绝各有其名（`UnknownHandle`、
`StaleHandle`、`ForeignPlugin`、`OutOfScope`），使插件能区分 worker 死亡
造成的失效与越权。这回应了工程评审的可伪造引用发现：`read_item` 授权
只允许宿主为当前邮件铸造句柄，猜测不等于查找。

## D3：路由器按 manifest、授权、作用域的顺序设三道闸

每个 `object_call` 在执行前过三道闸：方法已在插件 manifest 中声明（未
声明即类型化拒绝；closed-Intent 问题因此从不出现：该 envelope 不是
astra-app 的 Intent 枚举）；插件持有该方法所需的权限（D6 表）；调用中
的每个句柄都解析在插件作用域内。方法注册表
（`object_model/methods.rs`）为每个方法标明节点、所需权限与稳定性
（`stable` 映射到既有 intent，`proposed` 在对应 intent 落地前仅限开发
版）。本切片中路由器除 echo 回路外不执行任何方法：stable 方法的执行
随 SP3b 的 intent 接线到来。

## D4：worker 宿主复用 sidecar 的监督形态

`crates/astra-plugin/src/worker.rs` 为每个已激活插件 spawn 一个进程，
并通过 stdio 驱动 D1 envelope。监督机镜像 agent supervisor：down /
starting / ready / restarting / lost、代际号、5 秒 init 超时、3 次重启
预算（1-4 秒退避），以及死亡时的 fail-closed 排空：死亡 worker 的未决
调用报错返回，其订阅沿审批排空的同一路径排空。启动为惰性（首次调用
时，而非应用启动时）；`plugin_shutdown` 之后总是跟随 kill-and-reap，
无响应的 worker 不会比宿主活得更久。

## D5：echo 插件即符合性夹具

`plugins/echo/`：一个极小的 Rust 二进制，应答 `plugin_ready`、回显每
个 `object_call` 的 payload、按要求发出 `plugin_event`。它是 worker 宿
主的回路测试替身，随后也充当发送闸门的 harness 冒烟桩。它随仓库提
供，绝不进入发布构建。

## D6：权限表扩展为评审后的七级

`Permission` 变为 `read_item`、`read_mailbox`、`compose_edit`、`send`、
`admin`、`ai.infer`（restricted 是没有任何授权的状态，而非一个枚举
值）。`compose_edit` 与 `send` 按评审结论立即拆分：写作辅助插件绝不
能获得发送权。每个权限仍由穷尽匹配默认 NeedsApproval；manifest 仍拒
绝声明了方法却未声明其权限的插件。旧的 `transform_outgoing` /
`decode_incoming` 名称随钩子枚举退役；钩子 payload 类型保留给 SP3b。

## D7：桌面宿主以瘦注册表持有插件

`client/desktop/src/plugin/mod.rs` 是受管理状态：已发现的插件（插件目
录加上 `ASTRA_PLUGIN_DEV=1` 的开发路径）、授权表、worker 宿主句柄。
一组精简的 `generate_handler!` 命令（list / enable / disable /
approve）将其暴露；从包安装随 SP3d 到来。插件永远不能自行安装或激
活：发现由宿主完成，激活是人的决定，manifest 解析的 fail-closed 姿态
不变。

## 已记录的已知缺口，不隐藏

- 本切片中路由器的 stable 方法执行是桩；只有 echo 回路完整往返。
  intent 接线是 SP3b 的核心工作。
- 事件流仅以 envelope 类型与订阅簿记存在；尚无 astra-app 事件喂入。
- blob 引用只命名形态（`blob_id`、`size`、`sha256`）与配额规则（每插
  件 256MB、LRU）；缓存本身随第一个需要它的 payload（SP3b 的
  decode）落地。
- AOM 版本是双侧钉死的常量；协商式版本化是后续改进，不是隐藏缺口。
