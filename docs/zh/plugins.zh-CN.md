# 插件

插件平台的契约，在此一次性决定并对所有端面生效。设计的完整论证
（Outlook 风格对象模型、评审记录、以及本切片之后的分期）见 SP3a 规格
（`docs/superpowers/specs/2026-10-21-sp3a-plugin-platform-design.md`）；
本文档是代码所回应的规范性接缝。

## 形态

插件 = 一个进程外 worker 可执行文件 + 一份 `manifest.toml`，经 JSONL
stdio 通信。插件绝不在进程内运行，绝不持有引擎句柄，也绝不持有 DOM
句柄。它能做的一切都在 manifest 中点名、经路由器校验、由人批准。

## Envelope

每个方向每条消息一个紧凑 JSON 对象加一个换行，`type` 标签、
snake_case 字段、stderr 用于诊断。

宿主到插件：`plugin_init`（manifest、已授权限、`api_level`、
`aom_version`、worker 私有 `data_dir`）、`object_call`（`id`、可选
`handle`、`method`、`args`）、`object_subscribe`（`id`、`stream`）、
`plugin_shutdown`。

插件到宿主：`plugin_ready`（`api_level`、`aom_version`、所服务的
`methods`，必须是 manifest 声明集的子集）、`object_result`（`id`、
`value`）、`object_error`（`id`、`message`）、`object_event`
（`stream`、`event`）、`permission_request`（`id`、`capability`、
`reason`）、`plugin_event`（`event`）。

init 之后的首条消息必须是 `plugin_ready`；其他任何首条消息、畸形行或
握手之后的 ready 都是协议违规，连接即死。大 payload 以 blob 引用
（`{ blob_id, size, sha256 }`）传递，绝不整拷贝。

## 句柄

插件只能以宿主签发的句柄指名对象，形如
`{kind}.{plugin}.{generation}.{serial}`（例如
`msg.dev.astra.echo.3.7`）。宿主只为插件授权范围内的对象铸造句柄；路
由器只按自己的句柄表解析，拒绝任何非宿主铸造的引用。原始引擎 key
（例如 `imap:v1:u1`）绝不跨越边界。拒绝是类型化且互相区分的：未知
（从未铸造、畸形或客户端拼装）、过期（较早代际的）、外来（其他插件
的）、越权（方法所名的类别不符）。句柄随 supervisor 代际过期：重启后
的 worker 从空句柄表开始。

## 路由器的三道闸

每个 `object_call` 依次通过：

1. **Manifest**：方法已在 manifest 中声明。自定义
   `plugin.<id>.<name>` 方法跳过注册表查找，但不跳过声明。
2. **授权**：注册方法的权限已被授予。自定义方法没有注册表权限，仅凭
   声明即可通过此闸。
3. **作用域**：调用的句柄在方法期望时恰好存在、在插件自己的句柄表中
   可解析、且类别与方法所名相符。

proposed 方法（尚无支撑 intent）只在开发版路由。路由器本身不执行任何
东西：worker 宿主把已路由的调用变成 envelope，后续切片把已路由的调
用变成 intent。

## 权限

累积式、manifest 声明、逐授权由人批准。每个权限由穷尽匹配默认为需要
批准，新增权限会迫使权限表先回答它。

| 权限 | 能力 |
|---|---|
| `read_item` | 只读当前邮件；宿主仅为该邮件铸造句柄。 |
| `read_mailbox` | 读整个邮箱并可改变条目状态（已读标记、旗标、归档、回收站）。 |
| `compose_edit` | 经编写器写入（文档、收件人、选项、变换）。绝不含发送权。 |
| `send` | `message.send()` 与 `before_send` 闸门。每个新草拟首次发送按次批准，除非已为该账户预批准。 |
| `admin` | 密钥管理面、事件订阅、自动激活的 `before_send`。 |
| `ai.infer` | 仅宿主中介的推理：绝无提供方密钥，绝无直连模型调用，不被任何其他层级隐含。 |

`restricted` 不是一种权限：它是没有任何授权的状态。

## 监督

每个已激活插件一个进程，镜像 agent sidecar 的纪律：状态 down /
starting / ready / restarting / lost、代际号、5 秒 init 超时、3 次重启
预算（1-4 秒线性退避）、死亡时 fail-closed 排空：未决调用报错返回，
订阅沿审批排空的同一路径排空。无法启动的 spawn、版本不匹配、ready 点
名未声明方法都是致命的（绝不重试）。调用中途退出按失败关闭。关闭是
先请后杀：`plugin_shutdown` 之后总是跟随 kill-and-reap，无响应的
worker 不会比宿主活得更久。启动是惰性的：首次 start，而非应用启动时。

## 桌面注册表

桌面宿主扫描数据存储旁受管理的 `plugins/` 目录，以及
`ASTRA_PLUGIN_DEV` 所名的目录（分号分隔，每个直接指向一个插件目
录）。插件目录内含 `manifest.toml` 与 worker 二进制（manifest 的
`entrypoint`，或末段 id 加操作系统可执行后缀）。四条命令驱动注册表：
`plugin_list`、`plugin_enable`、`plugin_disable`、`plugin_approve`。
本切片中授权表是会话状态；持久化与同意对话框随分发切片落地。

## 已记录的已知缺口

- 已路由的调用会转发给 worker，但尚无宿主 intent 执行它们；stable 方
  法的接线是 SP3b 的核心工作。
- 事件流仅以 envelope 类型与订阅簿记存在；尚无 astra-app 事件喂入。
- blob 缓存的配额与淘汰已在契约中点名；缓存本身随第一个需要它的
  payload 落地。
- 授权尚不跨重启持久化。
- UI 贡献模型（描述符、沙箱 iframe）是 SP3c；签名包与安装对话框是
  SP3d。
