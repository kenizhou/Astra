> 非规范性译本，仅供审阅；以英文版为准。

# SP2r 计划：投递批次

规格：docs/superpowers/specs/2026-10-23-sp2r-delivery-design.md。分支
`sp2r-delivery` 自文档提交处的 main 切出。每个任务：先红灯、实现、全量
电池、一个约定式提交。

### 任务 1：核心（D2 的队列、D3 的 warm 趟、D4 的设置、D5 的中断态）

**文件：** crates/astra-app/src（SyncObserver 的 committed 钩子以
新/收件箱/非自发规则供给通知队列界面；趟后经引擎 warm/drop 动词的
预热与上限；偏好上的按账户同步设置（深度/上限/节奏，加性），由趟窗口
与搜索深度报告器读取；来自趟结果的按账户中断态）；设置快照行；协议
形状。

- [ ] 先写测试；提交 `feat: feed the delivery core`。

### 任务 2：宿主循环（D1、D2 的桥、D5 的横幅供给）

**文件：** crates/astra-account/src（IMAP 与 JMAP 具体 watcher 之上的
watch 包装器、引擎的余量守卫）；client/desktop/src（按账户驱动器：同步、
watch 或轮询、带退避重连；经 tauri-plugin-notification 排空核心队列的
通知桥（聚焦与汇总规则）；发往 web 的离线横幅与中断事件）。

- [ ] 先写测试；提交 `feat: run the background delivery loop`。

### 任务 3：界面

**文件：** client/web/src（离线横幅；账户面板行上的中断徽章；设置模态
的按账户同步小节：深度、上限、节奏、通知开关；指向设置的搜索深度
链接）。

- [ ] 先写测试；提交 `feat: surface the delivery batch`。

### 任务 4：实测证明 + 文档

**文件：** 带门禁的实测套件扩展（watch 站立时投递使 IMAP IDLE Changed
到达、循环同步它且队列持有它；短节奏轮询趟同样落地；上限下 warm 队列
排空且正文自存储服务）；docs/background-sync.md + zh（新建；各行引用的
参考）；若行 114 的引用该由本切片解决则 docs/sync-progress.md + zh
（评估）；capabilities 各行；fragment。

- [ ] 提交 `feat: prove the delivery batch live and record it`。

### 任务 5：收尾

- [ ] 全量电池；推送分支 `sp2r-delivery`；PR（预期 squash）；分类账
  清理。

## 自审记录

- 规格覆盖：队列、warm、设置与中断态（T1），循环与桥（T2），界面
  （T3），实测与文档（T4）。
- 接口链：核心态（T1）-> 驱动器（T2）-> 界面（T3）-> 实测与文档
  （T4）。
- 风险：observer 的 committed 钩子不得使其共享的同步进度界面退化；
  watch 包装器必须尊重引擎余量（绝不取走账户最后一条同步连接）；通知
  规则不得为发件归档或用户触发的刷新搅动触发；warm 上限不得与缓存
  相斗（只在 warm 落定后 drop）；按账户设置是签名之外第一批按账户偏好
  （形状必须在账户移除后存活）。
