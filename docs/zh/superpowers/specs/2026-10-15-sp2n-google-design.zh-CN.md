> 非规范性译本，仅供审阅；以英文版为准。

# SP2n Google 原生 API 设计

日期：2026-10-15
状态：proposed
父规范：`docs/superpowers/specs/2026-08-...-workflow-design`（第 5、15 节）
移植契约：mail-calendar 的 `docs/provider-oauth.md`（Google 一节：Desktop
客户端的环回重定向、非机密秘密、作用域表、`access_type=offline` +
`prompt=consent`、无日历重授权）与 MC 的
`crates/mailcal-account/src/google/` 作为代码参考。

## 1. 目的

踏在 OAuth 地基上的第二个提供商：Google 账户经浏览器流程登录
（Desktop 客户端的环回重定向由一次性本地监听器捕获），其邮件与日历经
引擎的 `provider-google` 适配器拨号，令牌源形如 Graph 的，作用域集
随账户用途。

## 2. 决策

- **D1（环回重定向）**：Desktop 客户端不能用自定义 scheme；重定向是
  `http://127.0.0.1:<临时端口>/`，端口烙进重定向，所以监听器在授权
  URL 铸造之前绑定。一次性有界 `TcpListener`（SP2l 会话增长 Google
  臂：绑定端口、以环回重定向铸造 URL、打开浏览器、捕获唯一请求、以
  Google 自己的最小 HTML 作答、把原始 URL 交给核心的解析器）。
  Microsoft 保持自定义 scheme；会话按提供商路由。
- **D2（非机密秘密）**：Google 的令牌端点拒绝不带客户端秘密的
  Desktop PKCE 交换；秘密随交换与刷新（持久化进 `[google]` 文档，
  让下次启动的刷新仍有它）。它不是机密（Google 自己的指引），PKCE
  仍是保护，且绝不入日志。
- **D3（作用域集）**：`GOOGLE_SCOPES`（已随 SP2l 的 provider.rs 移
  植）加 `access_type=offline` + `prompt=consent`（refresh token 只
  在两者齐备时回来）；用途表按交付裁剪（先邮件 + 日历；联系人与目
  录随各自切片；`userinfo.email` 恒有，因为它命名账户）；缺某用途
  作用域的授予不开启该用途；Google 无日历重授权步骤（横幅仍只属
  Microsoft）。
- **D4（账户拨号）**：引擎的 `GoogleClient` 藏在形如 Graph 的刷新包
  装器之后（代理缓存、先铸后呼、拒绝分类复用）；邮件提供商与日历提
  供商；`[google]` 配置节只增。Google 的 refresh token 刷新时不轮换
  （轮换写入是极少触发但不可省略的后备）。
- **D5（账户地址）**：`userinfo.email` 命名账户（从令牌自己的档案读
  取）；派生 id 为 `email@google`。

## 3. 测试

- 单元（astra-account，脚本化令牌端点）：Google 令牌源的刷新（刷新带
  秘密；无轮换后备）；作用域表的用途映射；包装器的代理纪律（复用
  Graph 的测试形态）。
- 环回捕获（桌面套件）：监听器先于 URL 铸造绑定（端口在重定向里）；
  一次性捕获；最小 HTML 作答；原始 URL 整体交接。
- 完成（SP2m 生命周期模式）：登录、地址读取、add/rotate、启动重拨、
  死授权的行。
- 实测（门控，所有者的 Google Cloud 注册 + 测试账户）：真实浏览器上的
  登录、邮件同步、日历读写。与 Graph 相同：harness 无法服务
  Google；实测证明等待注册，其缺席是记录在案的缺口。

## 4. 已知缺口（预记录）

- 实测证明依赖注册（Early Access 测试用户允许列表是 Google 自己的门，
  应用未验证期间如此；MC 的通知加复选框表面随可达性一同移植）。
- 联系人与 Workspace 目录随各自切片；发件名称写入
  （`sendAs.patch`）随账户页切片。
- Google 无日历重授权（被拒的日历保持关闭）；横幅仍只属 Microsoft。
- 全邮件作用域授予永久删除；没有任何 Google 工具或表面提供它（MCP
  切片已裁剪）。
