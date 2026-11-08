> 非规范性译本，仅供审阅；以英文版为准。

# SP2u 计划：日历与联系人收尾

规格：docs/superpowers/specs/2026-10-24-sp2u-finishers-design.md。分支
`sp2u-finishers` 自文档提交处的 main 切出。每个任务：先红灯、实现、
全量电池、一个约定式提交。

### 任务 1：核心（D1 的参与者跨线、D2 的规则跨线 + 格式化器、D4 的
形状偏好）

**文件：** crates/astra-app/src（详情读取增长参与者与完整规则；带
诚实回退的应用侧规则格式化器；日历形状偏好：视图种类 + 锚定，加性，
由导航写入，引导播种时读取）；astra-viewmodel（详情的线路形状）；
协议；伪件。

- [ ] 先写测试；提交 `feat: feed the finisher core`。

### 任务 2：界面

**文件：** client/web/src（详情坞的与会者列表 + 编辑器的只读副本；
替换最小行的规则句子；rowLayout/TimeGrid 的横幅几何 + 月网格的横幅
感知溢出；calendarStore 的形状恢复 + 导航写入；MessageRow 上的
Avatar 列及其响应规则）。

- [ ] 先写测试；提交 `feat: surface the finishers`。

### 任务 3：实测证明 + 文档

**文件：** 带门禁的实测套件扩展（一个带与会者与规则的种子事件在
测试服务器上渲染两者；一个种子联系人的照片站在发件人的行上）；
docs/avatars.md + zh（新建）；docs/calendar.md + contacts.md 扩展 +
zh；capabilities 各行（74、77、82、85、106）最终；fragment。

- [ ] 提交 `feat: prove the finishers live and record it`。

### 任务 4：收尾

- [ ] 全量电池；推送分支 `sp2u-finishers`；PR（预期 squash）；分类账
  清理。

## 自审记录

- 规格覆盖：跨线 + 格式化器 + 偏好（T1），全部界面（T2），实测与
  文档（T3）。
- 接口链：核心形状（T1）-> 界面（T2）-> 实测与文档（T3）。
- 风险：参与者的线路体积（诚实地为列表设上限；数百人的会议以计数
  折叠）；规则格式化器绝不为其未完全理解的规则措辞（钉住回退）；
  横幅几何对道既有重叠 packing；形状偏好的锚定陈旧（过去启动的日期
  精确恢复，诚实的形状）；巨列表上头像列的成本（既有缓存 + 响应
  隐藏）。
