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

## 发送闸门

每次提交发起的发送在**进入引擎队列之前**经过所有已注册的
`before_send` 处理器（引擎拥有队列的调度，入队之后的否决根本不成其为
否决）；每份草拟渲染出的正文在闸门之前经过所有已注册的
`composer.transform` 处理器。被阻断的发送永不入队：它以待决行的形态立
在发件箱界面（`SendingSnapshot.held`）中，点名插件与原因，由自动保存
已持久化的草稿承载。没有明文回退是由构造保证的。

manifest 声明模式（`before_send_mode`，声明处理器时必填）：

| 模式 | 违规时 | 处理器不可用或超时时 |
|---|---|---|
| `block`（默认） | 发送待决，阻断。 | 发送待决（失败关闭）。 |
| `soft_block` | 发送待决，阻断。 | 发送继续（失败开放）。 |
| `prompt_user` | 呈现 Send Anyway / Deny 问题由人作答。 | 发送继续（从未有问题被提出）。 |

超时沿用 Outlook 的形态：待决行从第一秒起点名所等待的插件，5 分钟上限
结束等待，按模式折叠。每个插件每份草拟一个处理器；自动激活需 `admin`
授权，仅有 send 授权而非 admin 的插件永远不会被调用。Send Anyway 不重跑
闸门直接入队，且绝不记忆；Deny 丢弃待决并保留草稿；重试对持有的草拟重
跑闸门；撤回丢弃待决。变换按注册表顺序运行，每个看到上一个的输出；失
败或不可用的变换使发送失败关闭（未变换的邮件绝不发出）。

## UI 贡献模型

描述符优先：manifest 的可选 `[ui]` 表声明静态形态，由 Astra chrome 渲
染。`composer_actions` 成为编写器 chrome 中的按钮；`settings_sections`
成为设置页 Plugins 详情中的分类（每个字段按其种类的控件：toggle、
text、select、password、action）；`pages` 在沙箱 frame 中打开。一次点
击或字段变更即一次对 worker 已声明自定义方法（`plugin.<id>.<name>`）的
`plugin_call`，应答为 worker 自己的 payload；动作的效果经应答呈现（例
如开关的 `pressed`），因为 worker 绝不直接触碰草稿。

阅读窗格的横幅槽位（`PluginBanner`）渲染 decode 应答的
`security_banner` 腿：插件的级别与措辞，挂在正文上方。

富页面运行在 `PluginFrame` 中：`astra-plugin://` scheme 上的 iframe，仅
在该插件自己的目录内解析 `astra-plugin://<plugin-id>/<path>`（拒绝穿越
与跨插件读取，缺失文件 404，HTML 携带 `default-src 'none'` 加仅包内
脚本/样式/图片/字体的 CSP）。页面经窄 postMessage 桥（`frame_call` /
`frame_result` / `frame_error`）与其 worker 通信，桥只服务 frame 自己窗
口的消息，并受与一切调用相同的已声明方法门控。

## 分发

`.astra-plugin` 是插件目录的未压缩 tar，外加 `publisher.pub` 与
`signature.bin`：签名是对规范摘要的 ed25519（排序后 (路径,
sha256(内容)) 对的 sha256，签名自身不参与，发布者密钥参与）。验证即防篡
改加身份：签名必须对包内发布者密钥验证通过，且该密钥必须等于 manifest
的 `publisher` 字段。同意对话框显示的指纹是发布者密钥 sha256 的前 8 字
节（hex）。这有意不是证书链：信任决定在安装时由人做出。

生产构建（无 `ASTRA_PLUGIN_DEV` 且为发布版）拒绝未签名或无法验证的包；
开发模式照旧接受未签名目录树。`cargo run -p xtask -- bundle-keygen
<key-file>` 铸造发布者密钥并打印 manifest 的 `publisher` 字段；
`bundle-plugin <dir> <key-file>` 打包并签名。

安装是先报价后同意：`plugin_install_offer` 验证并点名一切（id、名称、版
本、发布者指纹、所请权限、隐私政策 URL），同意对话框的复选框默认不勾
（失败关闭），`plugin_install` 重新验证、解入受管目录、恰好记录所勾授
权。安装完成的插件保持停用，直至手动启用。

授权与启用标记写穿到注册表根目录旁的 `grants.toml`；启动时读取并重新启
用已启用集合（如今失败的 worker 落入 Failed，绝不使启动失败）。生产构
建绝不启用声明了 proposed 方法的插件。`plugins_enabled` 偏好（设置快照
的块，默认开）即管理策略开关：关则启动不启用任何插件、`enable` 被拒、
闸门/解码器/调用端面视同没有任何插件在服务。

## Track B：内置加密

消息加密是产品功能而非插件：S/MIME 与 OpenPGP 内置于应用核心
（2026-10-20 决定），因此没有任何插件会看到它不需要的明文，发送闸门的
信任模型属于应用自身。SP4b 之后的状态：

- **存储**（`crates/astra-crypto-store`）：AI 存储旁的一个
  `crypto.sqlite`，kylins schema 的最终形态（`crypto_keys` 含
  harvested/intermediate、仅追加的 `trust_decisions`、`collected_keys`、
  `contact_pinned_keys`、`crl_cache`、`message_crypto_results`、
  `send_as_aliases`）。软私钥在 OS 钥匙串中的主密钥下以 AAD 绑定的
  AES-256-GCM 封装静存。`SqliteKeyStore` 桥实现核心的 `KeyStore`（按账
  户绑定、`standard|fingerprint` 的 KeyId、OpenPGP 子钥回退、令牌句
  柄、可取回性过滤）；信任决策与收件人解析阶梯（NoKey 带已收集提示、
  过期先行、自有密钥即 Ready、verified/personal/unverified 即 Ready 模
  型、Rejected、Undecided、AcceptedOtherEmail、钉选优先）镜像 kylins 的
  `resolve_recipient_status_at`。
- **发送闸门**（`crates/astra-crypto-core::apply`）：kylins
  `apply_crypto` 语义逐字。直通、`NoSigningKey`、失败关闭的信任闸（每
  个收件人必须解析为 Ready；无明文回退、拒绝时不产生密文）、钉选感知
  的选择使闸门与加密使用同一密钥、仅 S/MIME 的对照已存锚点的链闸、默
  认开启且读错安全默认开的受保护头主题隐藏、仅 PGP 且在加密部分内的
  Autocrypt-Gossip、`EncryptOp.sign_with` 组合（绝不先明文签名再加密），
  以及 W9 密码臂（SKESK 对称，`PasswordConflict`/`PasswordRequired`）。
- **引擎的 MIME-wrap 接缝**（ECS 规格，取代 SP4b 的 D3）：vendored 引擎
  携带 provider 中立的 `MimeWrapper` trait，绑定存储的包装器
  （`astra-crypto-store::wrap`）在拨号时安装到每个启用了方法的 IMAP 账
  户。加密发送骑乘**引擎自己的发件箱**：drain 组装归档变体（带
  `Bcc`）并交给包装器一次，线上变体是同一密文减去 `Bcc` 头，Sent 副
  本由引擎自己的归档逻辑落盘。信任拒绝在任何 socket 拨号前将 op 落为
  `Failed`，原因随行展示；密钥修好后用队列自己的 Send Now 重跑整个包
  装。一台发送引擎、一个发件箱、一条 drain：SP4b 的 Astra 侧加密发件
  箱及其绕行传输已退役（`crypto_outbox` 表删除，未发布）。
- **开关**：每个账户在其存储配置中的 `[crypto] method`
  （`none | openpgp | smime`），随设置行回显并可在账户页编辑。`none`
  是零开销默认：每封发送明文发出，标记惰性。编写器的 sign/encrypt 标
  记在提交时盖到引擎的 `Draft` 上，默认同开。写入在持久化**之前**重
  拨账户，因此新方法随下一封发送生效；对没有 IMAP/SMTP 提交设置的账
  户（今天是 Graph/Google/JMAP：没有可挂包装器的接缝）失败关闭地拒绝
  ——方法存在而发送明文的情况不会发生。

后续切片：SP4c 是接收半（经阅读流水线的解密/验签、静存加密草稿、结果
行与安全横幅、CRL 拉取、Autocrypt 消费半）；SP4d 是令牌注册表
（cryptoki、openpgp-card）与原生安全 UI。无已注册 provider 的令牌密钥
如今即得到类型化失败（`TokenNotPresent`），绝不回退软件密钥。

## 已记录的已知缺口

- 在 `before_send` 处理器激活前已入队的 op 不做追溯过闸；drain 内的逐
  op 扣留需要当前钉定版本引擎不具备的支持。
- 待决行是会话状态；其背后的草稿是持久的，待决条目经重新发送重建。
- 插件主动发起的调用（worker 自行读取 AOM）尚无 envelope；发送路径所
  需的调用都是宿主发起的，该缺口今天零成本，在有消费者时落地。
- 已路由的调用会转发给 worker，但尚无宿主 intent 执行它们；stable 方
  法的接线是后续切片的核心工作。
- 事件流仅以 envelope 类型与订阅簿记存在；尚无 astra-app 事件喂入。
- blob 缓存的配额与淘汰已在契约中点名；缓存本身随第一个需要它的
  payload 落地。
- decode 接缝已在阅读流水线中落地（`MessageDecoder` 端口、原始源交给服
  务中的插件、解出的腿走完全相同的消毒流水线、横幅进入阅读快照）；尚
  缺的是一个真正执行解码的已发布插件（加密模块为内置，Track B）。
- 发布者密钥是自我声明的（侧载信任）；带发布者身份验证的策划商店、密
  钥轮换、吊销与自动更新是计划中的"以后"。
