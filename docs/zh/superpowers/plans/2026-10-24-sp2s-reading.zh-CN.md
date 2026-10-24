> 非规范性译本，仅供审阅；以英文版为准。

# SP2s 计划：阅读批次

规格：docs/superpowers/specs/2026-10-24-sp2s-reading-design.md。分支
`sp2s-reading` 自文档提交处的 main 切出。每个任务：先红灯、实现、
全量电池、一个约定式提交。

### 任务 1：核心（D4 的举报意图、D1 的窗口供给）

**文件：** crates/astra-app/src（ReportMessage 意图 onto 引擎的发件
箱中介动词，带能力门与判定映射；窗口拉取的阅读快照字段；not-spam
回归路径）；协议形状；伪件的举报腿。

- [ ] 先写测试；提交 `feat: feed the reading core`。

### 任务 2：宿主腿（D2 的保存与打印、D1 的窗口）

**文件：** client/desktop/src（eml_save 命令建立在引擎源读取与
SP2p 保存缝上；打印命令或你判断诚实的打印窗口方案——记录选择）；
编写器旁的邮件窗口能力条目。

- [ ] 先写测试；提交 `feat: land the reading host legs`。

### 任务 3：界面

**文件：** client/web/src（MessageWindow 根 + 行菜单的 Open in New
Window；溢出菜单的 Save as .eml 与 Print；缩放控件与带重排钳制的
srcdoc 包裹层；Junk 与 Not spam 动词；滑动处理器 + 方向设置 + 撤销
提示）；阅读与设置界面的测试。

- [ ] 先写测试；提交 `feat: surface the reading batch`。

### 任务 4：实测证明 + 文档

**文件：** 带门禁的实测套件扩展（.eml 导出与收到的源一致；垃圾举报
真实落地并被观察；固定宽度在两个宽度下重排）；docs/reading-window.md
+ reading-zoom.md + reporting.md + zh 副本（新建）；docs/
reading-actions.md 扩展 + zh；capabilities 各行；fragment。

- [ ] 提交 `feat: prove the reading batch live and record it`。

### 任务 5：收尾

- [ ] 全量电池；推送分支 `sp2s-reading`；PR（预期 squash）；分类账
  清理。

## 自审记录

- 规格覆盖：D4 的意图 + D1 的供给（T1）、D2 + D1 的宿主半（T2）、
  全部界面（T3）、实测与文档（T4）。
- 接口链：核心形状（T1）-> 宿主腿（T2）-> 界面（T3）-> 实测与文档
  （T4）。
- 风险：窗口的两读者一存储已读标记（钉住单次写入）；srcdoc 包裹层
  不得削弱 CSP 或远程图片门；举报的幂等键按意图；滑动/滚动/拖拽的
  轴冲突（触摸指针拥有水平、虚拟器拥有垂直、鼠标拥有拖拽）；打印
  文档复用净化（绝不第二套实现）。
