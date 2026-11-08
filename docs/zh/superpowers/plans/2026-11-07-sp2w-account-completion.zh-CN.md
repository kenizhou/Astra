> 非规范性译本，仅供审阅；以英文版为准。

# SP2w 计划：账户补完批次

规格：docs/superpowers/specs/2026-11-07-sp2w-account-completion-design.md。分支
`sp2w-account-completion` 自文档提交处的 main 切出。每个任务：先红灯、实现、
全量电池、一个约定式提交。

### 任务 1：核心（D1 的身份半、D4 的解决）

**文件：** crates/astra-app/src（引擎身份动词之上的发件名称服务：加入时
采纳、可写推送、IMAP 的本地偏好；设置快照的身份字段；名称询问状态）；
供应商三族在 docs/capabilities.md + zh 与待办清单决定节的解决；伪件的
身份腿。

- [ ] 先写测试；提交 `feat: feed the account completion core`。

### 任务 2：宿主与界面（D1 的询问、D2 的重授权、D3 的切换器）

**文件：** client/desktop/src（账户层臂之上的 JMAP 重授权命令；身份
读取）；client/web/src（向导的一次询问名称步；设置卡的三种控件；携带
名称的 From 选择器与账户行；过期行的重授权动作；文件夹面板的账户头
切换器与列表头部的谁的邮件标签）。

- [ ] 先写测试；提交 `feat: surface the account completion`。

### 任务 3：实测证明 + 文档

**文件：** 带门禁的实测（测试服务器上的 JMAP 身份采纳与名称写入；
脚本化形状之上的重授权授权交换）；docs/sending.md 扩展（发件人名称
契约）+ zh；docs/jmap.md + provider-oauth.md + zh 解决；capabilities
各行（名称、JMAP 重授权、切换器最终；供应商三族解决）；fragment。

- [ ] 提交 `feat: prove the account completion live and record it`。

### 任务 4：收尾

- [ ] 全量电池；推送分支 `sp2w-account-completion`；PR（预期 squash）；
  分类账清理。

## 自审记录

- 规格覆盖：D1 的核心半 + D4（T1）、询问 + 重授权 + 切换器界面（T2）、
  实测与文档（T3）。
- 风险：身份推送按家族（Google 可写、JMAP 经刷新包装可写、Graph 只读、
  IMAP 仅本地）——逐一钉住；重授权搭乘凭据事务（先注册后拨号的顺序）；
  切换器不得与文件夹选择相斗（两个作用域、一个动词族）。
