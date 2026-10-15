> 非规范性译本，仅供审阅；以英文版为准。

# SP2m Microsoft Graph 设计

日期：2026-10-14
状态：proposed
父规范：`docs/superpowers/specs/2026-08-...-workflow-design`（第 5、15 节）
移植契约：mail-calendar 的 `docs/provider-oauth.md`（Microsoft 半边：规则已随
SP2l 落地；本切片移植账户接线）与 MC 的
`crates/mailcal-account/src/graph/` 作为代码参考。

## 1. 目的

踏在 OAuth 地基上的第一个提供商：Microsoft 账户经浏览器流程登录，其邮
件与日历经引擎的 `provider-graph` 适配器拨号，令牌经单一令牌源按需刷
新、轮换落进 OS 存储，Microsoft 账户所欠的表面（权限缺失时的重连横
幅、过期登录行）随之一同到达。

## 2. 决策

- **D1（令牌源）**：`GraphTokenSource` 移植自 MC 的
  `graph/token_source.rs`（468 行）：每账户一个，由登录播种，在偏斜规则
  下于过期前铸造 access token，经 `astra-oauth` 的 `refresh` 刷新，按
  移植的 `GrantRefusal` 分类拒绝（Dead/Underscoped 停止重试并浮现；
  Indeterminate 不改变任何判断），经 SP2l `CredentialTransaction` 的
  `rotate` 路径轮换存储的 refresh token（地基记录的 SP2m 锚点）。每个
  凭据一个刷新者：令牌源在拨号前注册进账户条目，拨号期间的轮换有处可
  落。
- **D2（账户拨号）**：引擎的 `GraphClient::for_mailbox` 骑令牌源铸造的
  令牌，邮件文件夹用 `GraphProvider`，默认日历用
  `GraphCalendarProvider`（principal 为 `Me`；共享邮箱延后），各自藏
  在引擎的 `Provider` trait 之后，按 SP2h 确立的模式装配进宿主账户
  （能力作用域的选择已能挑中日历）。
- **D3（登录完成为账户）**：SP2l 向导的成功句变为真实的添加：令牌交换
  的地址查找命名账户；授权经事务的 `add` 持久化（拒绝即回滚）；注册
  条目、拨号、首次同步；重新授权（为已存在的账户再次登录）保留存储形
  状并走 `rotate`，绝不走 `add`。
- **D4（拒绝表面）**：重连横幅（Graph 账户缺失所需权限，如服务器侧撤
  销同意；一键重跑登录重新授予完整作用域集）与过期登录行（授权已逝：
  如实报告并提供再次登录，绝不记为服务器故障）。两者读令牌源的分
  类；两者浮现在账户页与文件夹窗格的每账户徽标。通道已定：常驻拒绝
  状态是**快照数据**，设置快照账户行上新增 `grant` 字段（增量线型），
  经宿主 account-info 缝自启动拨号与登录完成处取得（分类生于宿主
  侧）；SP2l 的完成事件仍是向导的一次性通道，因为晚挂载的表面或重启
  后必须拉取常驻状态，而非追赶一次发射。
- **D5（作用域集）**：`MICROSOFT_GRAPH_SCOPES`（已随 SP2l 的
  provider.rs 移植）在授权时按账户用途请求（先邮件 + 日历；联系人与目
  录随各自切片）；refresh token 要 `offline_access`；已授予缓存可在启动
  与离线时作答。

## 3. 测试

- 单元（astra-account，脚本化令牌端点）：令牌源的刷新周期（偏斜、单
  飞、经事务的轮换写入、拒绝分类的三种应答）；拨号（播种的令牌；过期
  重拨）。
- 登录完成为账户（桌面套件骑会话的喂送回调缝）：添加路径、存储拒绝的
  回滚、经 rotate 的重新授权。
- 拒绝表面（web）：横幅、行、一键重授。
- 实测（门控，所有者的 outlook.com 账户 + Azure 注册）：真实浏览器上的
  登录、邮件同步、日历读、一次写、刷新轮换。harness 无法服务 Graph；
  实测证明需要注册与账户，其缺席是记录在案的缺口，绝不伪造通过。

## 4. 已知缺口（预记录）

- 实测证明依赖 Azure 应用注册（所有者的）；没有它，切片离线证明交付，
  实测证明随注册落地。
- 共享/他人邮箱（`MailboxPrincipal::user`）延后：今天一个账户就是它自
  己的邮箱。
- Graph 上的联系人与同事目录延至各自切片（联系人表面已骑人员索引；
  目录需要 `colleagues` 选择）。
- 发件名称写入（`sendAs.patch`）是 Graph 自己的；随拥有该字段的账户页
  切片。
- 注册的客户端秘密：Microsoft 的公共客户端不带；若注册实际需要一个，
  与 SP2l credentials 形态的偏差在 PR 中升级。
