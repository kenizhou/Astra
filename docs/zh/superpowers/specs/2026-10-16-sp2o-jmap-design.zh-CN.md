> 非规范性译本，仅供审阅；以英文版为准。

# SP2o JMAP 账户设计

日期：2026-10-16
状态：proposed
父规范：`docs/superpowers/specs/2026-08-...-workflow-design`（第 5、15 节）
移植契约：mail-calendar 的 `docs/jmap.md`（规则一节整体搬入）与
`crates/mailcal-account/src/jmap/` 作为代码参考；OAuth 半边骑在 SP2l 地基
上；`mail-oauth.md` 的 XOAUTH2 半边留给 SP2v。

## 1. 目的

踏在 OAuth 地基上的第三个提供商，也是第一个密码腿实测无需所有者注册的
提供商：JMAP 账户是一个基础 URL 加一个秘密（密码或 API 令牌，存作
`password`，因为这个形态严格更能干），经粘贴的秘密连接，或在服务器宣
告之处经标准发现的 OAuth 登录连接。harness 原生说 JMAP，所以密码腿落地
当日即可实测。

## 2. 决策

- **D1（一个秘密，一个表单）**：存储的 `[jmap]` 节携带邮箱、基础 URL
  与一个 `password`（Secret）。设置的 JMAP 臂问一个字段，标“Password
  or API token”，绝不两个；遗留 `token` 键仍可读。引擎的传输按服务器自
  己的 401 挑战协商方案。
- **D2（URL 推导）**：空白服务器按 well-known 推导
  `https://<邮箱域>`；裸主机补 `https://`；显式 `http://` 仅为 harness
  夹具存活（检测已携带的信任规则）。
- **D3（账户身份与拨号）**：id 键于 `address@jmap:<host>`（与 IMAP 及
  OAuth 家族区分）；一个提供商覆盖整个账户（邮件作用域账户级；文件夹经
  它同步；文件夹绑定器重连同一账户级提供商）。拨号在共享 TLS 与重试策
  略之下装配引擎的 `JmapProvider`。
- **D4（OAuth 腿，仅当服务器宣告）**：SP2l 地基的发现（RFC 9728 ->
  8414 -> 7591）当且仅当服务器自己的 401 点名时提供；每一跳 HTTPS
  （环回例外）、issuer 匹配、必须 S256；作用域限于应用所用；发现一次持
  久化（端点、client id、refresh token 进 `[jmap.oauth]`）；OAuth 账户
  的 `RefreshingJmapProvider` 包装器（约每小时重铸；经 SP2l 事务的轮换
  写入在棘轮式 JMAP 服务器上绝不可省）。重新授权（过期授权自己的按
  钮）从持久化的授权重跑，不再注册。
- **D5（向导臂）**：检测已有的 JMAP 推荐路由到 JMAP 表单（一个字段、
  可编辑的推导 URL）；OAuth 提议只在服务器宣告处渲染（fail-soft：任何
  发现缺失都把字段留在原地）；OAuth 的四条约束按规则移植。
- **D6（表面）**：账户的行（类型、腿）、过期登录行（OAuth 账户自己的
  重授权；密码账户的行指向设置）、日历读骑提供商自己宣告的能力。

## 3. 测试

- 单元（astra-account）：配置节（一个秘密；遗留 token 读取；URL 推导矩
  阵）；脚本化会话之上的拨号；刷新包装器的铸造/重建纪律（Graph/Google
  形态）；OAuth 发现的四条约束（部分已在 astra-oauth 电池中证明）。
- 向导（vitest）：JMAP 表单、一个字段、OAuth 提议的宣告门控渲染、
  fail-soft。
- 实测（门控，harness 的 2080 JMAP）：密码腿端到端（连接、文件夹列
  表、消息列表、正文读取、一次发送）；harness 宣告之处的日历读；OAuth
  腿对 harness 401 的发现（RFC 9728 文档存在；Stalwart 是否宣告完整链
  决定诚实的钉或缺口）。

## 4. 已知缺口（预记录）

- JMAP 上的联系人随联系人切片（引擎适配器存在；MC 的
  refreshing_contacts.rs 随之延后）。
- 发件名称写入与按用途能力选择随各自切片。
- OAuth 腿的实测依赖 Stalwart 宣告什么；密码腿无论如何完全可实测。
- IMAP/SMTP XOAUTH2 留给 SP2v。
