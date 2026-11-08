> 非规范性译本，仅供审阅；以英文版为准。

# SP2v 计划：IMAP/SMTP OAuth 登录

规格：docs/superpowers/specs/2026-10-25-sp2v-mail-oauth-design.md。分支
`sp2v-mail-oauth` 自文档提交处的 main 切出。每个任务：先红灯、实现、
全量电池、一个约定式提交。

### 任务 1：核心（D2 的链、D3 的授权与源）

**文件：** crates/astra-account/src（imap 节旁的邮件 OAuth 授权形状；
共享 GraphTokenSource 之上承载 token 的 CredentialSource；签发者链：
autoconfig 路线（自动检测 crate 已解析字段的跨线）、两个 well-known
候选、软失败；注册缓存；引擎动词之上的探测包装）；配置加载/保存臂；
伪件。

- [ ] 先写测试；提交 `feat: feed the mail oauth core`。

### 任务 2：宿主与界面（D1 的屏幕、D3 的拨号、D4）

**文件：** client/desktop/src（引导的 imap 授权臂带拨号表与轮换汇聚
点；完成；向导询问所乘的探测命令）；client/web/src（探测回答之上
IMAP 步骤的三屏；登录按钮与降级的密码；过期行的 imap_smtp 形状）；
线路。

- [ ] 先写测试；提交 `feat: surface the mail oauth sign-in`。

### 任务 3：实测证明 + 文档

**文件：** 带门禁的实测（测试服务器的真实能力行钉在两条腿上；发现在
测试服务器上的软失败）；docs/mail-oauth.md + zh（新建：十一条规则
按交付范围移植）；docs/provider-oauth.md 的延期句子解决；capabilities
行 26 最终；fragment。

- [ ] 提交 `feat: prove the mail oauth leg and record it`。

### 任务 4：收尾

- [ ] 全量电池；推送分支 `sp2v-mail-oauth`；PR（预期 squash）；分类账
  清理。

## 自审记录

- 规格覆盖：链 + 授权 + 源（T1），拨号 + 屏幕（T2），实测与文档
  （T3）。
- 接口链：核心形状（T1）-> 宿主与屏幕（T2）-> 实测与文档（T3）。
- 风险：探测必须门在所拨服务器上（测试服务器的 TLS 接受行走，不是
  第二次拨号）；token 源不得与引擎的一次续期相斗（引擎询问、源每次
  拒绝最多刷新一次）；签发者候选必须尊重可注册域名规则（无公共后缀
  泄漏）；静态表交付为空且无代码路径可以发明 id；密码降级绝不移除
  字段（参考规则）。
