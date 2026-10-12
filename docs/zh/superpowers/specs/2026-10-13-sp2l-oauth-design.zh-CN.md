> 非规范性译本，仅供审阅；以英文版为准。

# SP2l OAuth 地基设计

日期：2026-10-13
状态：proposed
父规范：`docs/superpowers/specs/2026-08-...-workflow-design`（第 5、15 节）
移植契约：实质移植 mail-calendar 的 `docs/provider-oauth.md`（规则一节整
体搬入）与 `mailcal-oauth` crate 作为代码参考；`mail-oauth.md` 与
`jmap.md` 的半边分别延至 SP2v/SP2o。

## 1. 目的

提供商阶梯所踏的 OAuth 地基：纯的 Authorization-Code-with-PKCE 客户端
（begin、complete、refresh）、授权拒绝分类、为 Astra 尚未注册客户端的
服务器准备的动态发现与注册、带深链重定向的系统浏览器登录会话，以及规则
所述"核心拥有何时、宿主拥有何处"的凭据事务。此处尚不登录任何邮件账
户：Graph 与 Google 切片（SP2m、SP2n）在此地基上接线提供商；现在交付
的是地基。

## 2. 决策

- **D1（crate）**：`astra-oauth`，忠实移植 `mailcal-oauth` 的契约与测
  试：`Pkce`（S256）+ `random_state`；`exchange_code`/`refresh`/
  `TokenSet`；`OAuthProviderConfig`（`microsoft()`/`google()`）、
  `AuthStyle`、作用域清单；`GrantedScopes` 与 `GrantRefusal`（Dead /
  Underscoped / Indeterminate，按分类绝不越出两个拒绝码做文本匹配）；
  `discover_protected_resource` / `discover_auth_server` /
  `register_client`（RFC 9728 -> 8414 -> 7591）；`credentials.rs` 作
  为预注册客户端 id 的唯一持有处（未给的构建仍能连邮件：需要注册的路
  由即不提供）；`reach`（令牌请求的传输分类）；Debug 自行脱敏的
  `Secret`。refresh 绝不携带作用域（不可协商的规则）；已授予集合的缓
  存跟随令牌响应的原话。
- **D2（浏览器会话）**：经 Tauri shell 打开系统浏览器，重定向由
  `tauri-plugin-deep-link` 在自定义 scheme 上捕获；绝不用内嵌
  WebView。客户端把原始回调 URL 原样交回核心；绝不自行解析令牌。CSRF
  的 `state` 校验站在 `complete` 里。
- **D3（凭据事务）**：令牌只存 OS 密钥库（refresh token 是唯一的静态
  秘密；access token 按需铸造、绝不持久化）；Secret 自行脱敏。一个
  `AccountCredentialStore` 端口（`persist`、`delete`）服务所有家族；
  两者都报告成败，核心按规则作答：添加失败即回滚、轮换记录下次启动
  将面对什么、移除返回错误而非回退。
- **D4（向导表面）**：账户设置获得 OAuth 路径的骨架：提供商登录存在
  之处，设置在密码表单旁提供它（检测的推荐已路由）；拒绝状态浮现
  （Dead 与 Underscoped 都请用户再次授权；Indeterminate 不改变登录的
  任何既有判断）。Microsoft 与 Google 路由仅在 `credentials` 携带其注
  册时渲染。
- **D5（重放安全规则）**：四条规则（单飞 refresh、绝不存储被超越的令
  牌、经端口的轮换写入、死授权即停）连同测试移植；把重放 refresh
  token 视为盗窃的服务器绝不会收到一枚。

## 3. 测试

- 单元（astra-oauth，纯函数，无网络）：移植的测试组（pkce 的 S256 向
  量、令牌端点的请求/响应形态、拒绝分类含绝不文本匹配的边界、作用域
  规则含 refresh 无作用域钉、发现的 RFC 文档、注册的客户端铸造、
  Secret 的脱敏）。
- 宿主会话（桌面测试）：深链 scheme 捕获、原始 URL 交回、state 不匹
  配被拒。
- 凭据事务：三条应答路径（添加回滚、轮换记录、移除报错）以脚本化存
  储驱动。
- 向导表面（vitest）：注册存在时密码表单旁的 OAuth 提议；不存在时不
  渲染；拒绝语句。
- 实测（门控，harness）：发现半边对 Stalwart JMAP 的 `/.well-known`
  （真实的 RFC 9728 文档）；harness 内无真实令牌交换（记录：真实登录
  属 SP2m/SP2n，用所有者的账户）。

## 4. 已知缺口（预记录）

- 尚无邮件账户类型消费此地基：Graph（SP2m）与 Google（SP2n）接线提供
  商；JMAP OAuth（SP2o）与 IMAP XOAUTH2（SP2v）随各自切片。
- 重连横幅与过期登录行随第一个能触发它们的 OAuth 账户类型浮现
  （SP2m）；它们读取的分类在此交付。
- RFC 7591 注册对公测服务器的滥用无实测（无 harness 服务器实现它；记
  录）。
- Windows 的浏览器选择是系统默认；无按浏览器偏好。
