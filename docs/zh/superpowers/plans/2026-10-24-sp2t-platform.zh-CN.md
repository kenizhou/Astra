> 非规范性译本，仅供审阅；以英文版为准。

# SP2t 计划：平台批次

规格：docs/superpowers/specs/2026-10-24-sp2t-platform-design.md。分支
`sp2t-platform` 自文档提交处的 main 切出。每个任务：先红灯、实现、
全量电池、一个约定式提交。

### 任务 1：核心（D1 的开关、D2 的同意、D5 的语言、D6 的闩锁）

**文件：** crates/astra-app/src（偏好：debug_enabled、
analytics_consented、language、zone_offered；设置块与行；仅在同意下
写 AI 账本的分析事件记录器；设备时区提议的闩锁与接受进设置路径）；
协议形状；伪件。

- [ ] 先写测试；提交 `feat: feed the platform core`。

### 任务 2：宿主腿（D1 的 logger 与命令、D4 的登记与状态、D4 的分享）

**文件：** client/desktop/src（启动时安装的轮转文件 logger；log_tail
与 log_export 命令；提议背后的默认邮件运行时登记；应用数据目录之上
的窗口状态持久化；把文件交给编写预填通道的分享目标入口）。

- [ ] 先写测试；提交 `feat: land the platform host legs`。

### 任务 3：界面

**文件：** client/web/src（Diagnostics 分类：查看器、copy for
support、导出、调试开关；带声明与开关的 Privacy 分类；语言选择器与
跟随它的日期；设备时区提议 UI；默认邮件设置；作为第一批本地化集合
的设置与 About 文案；若 D7 交付则演示旗标的门控）。

- [ ] 先写测试；提交 `feat: surface the platform batch`。

### 任务 4：实测证明 + 文档

**文件：** 证明（headless 测试架上突发写入下的日志轮转；仅在同意下
被观察的账本行；导出的字节；驱动日期的语言）；docs/logging.md +
analytics.md + os-integration.md + timestamps.md + zh 副本（新建）；
capabilities 各行（62、63、120、121、122、123、124、125）；fragment。

- [ ] 提交 `feat: prove the platform batch live and record it`。

### 任务 5：收尾

- [ ] 全量电池；推送分支 `sp2t-platform`；PR（预期 squash）；分类账
  清理。

## 自审记录

- 规格覆盖：偏好与记录器（T1），logger 与操作系统腿（T2），分类与
  本地化开端（T3），证明与文档（T4）。
- 接口链：核心态（T1）-> 宿主腿（T2）-> 界面（T3）-> 证明与文档
  （T4）。
- 风险：文件 logger 从第一行起遵守日志契约（无密钥、无内容、无路径
  ——测试对文件 grep 禁类）；无同意时分析记录器不可达（钉住反例）；
  语言偏好不得触碰日历时区；设备时区提议绝不覆盖已挑选时区（钉住）；
  窗口状态文件不是偏好（核心偏好仍是唯一文档；宿主拥有自己的状态
  文件）；若 D7 腿超出批次则如实记录为缺口。
