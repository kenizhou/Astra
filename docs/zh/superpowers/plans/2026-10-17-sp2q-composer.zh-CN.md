> 非规范性译本，仅供审阅；以英文版为准。

# SP2q 计划：编写器收尾

规格：docs/superpowers/specs/2026-10-17-sp2q-composer-design.md。分支
`sp2q-composer` 自文档提交处的 main 切出。每个任务：先红灯、实现、
全量电池、一个约定式提交。

### 任务 1：核心与线路（D1 的线路半、D3 的播种、D4 的偏好、D5 的
预填形状）

**文件：** documentWire.ts 的可编辑 run 规则与标记（三个标记双向，
web）；crates/astra-app（签名存储：设置/偏好里的按账户指派、打开时
的播种规则、From 变更时的未改动切换规则；默认发送账户偏好；外壳
快照中账户行的发送地址；基于核心解析器的 mailto 预填形状，线路仅
可加性）。

- [ ] 先写测试；提交 `feat: seed the composer finishing in the core`。

### 任务 2：宿主腿（D5 的入口）

**文件：** client/desktop/src（OAuth 监听器旁的 mailto 深度链接监听
器、单实例参数转发、打开编写器的发射；阅读面板的 mailto 点击改为
内部路由，web）。

- [ ] 先写测试；提交 `feat: open the composer from a mailto link`。

### 任务 3：界面（D1 的工具栏、D2、D3 的编辑器、D4 的选择器、D5 的
粘贴）

**文件：** client/web/src/features/composer/（工具栏的三个控件；
带编辑、移除与输入地址识别器的链接弹层；发送时的退化句子；From
选择器；粘贴处理器）；设置功能（带按账户指派的签名编辑器；默认
发送账户控件）。

- [ ] 先写测试；提交 `feat: finish the composer surfaces`。

### 任务 4：实测证明 + 文档

**文件：** 带门禁的实测套件扩展（带格式的发送往返；带签名的发送
往返；链接往返含被拒方案退化）；docs/signatures.md + zh（新建）；
docs/composer-security.md 扩展（链接、粘贴、mailto）+ zh；
capabilities 各行；fragment。

- [ ] 提交 `feat: prove the composer finishing live and record it`。

### 任务 5：收尾

- [ ] 全量电池；推送分支 `sp2q-composer`；PR（预期 squash）；分类账
  清理。

## 自审记录

- 规格覆盖：核心与存储半（T1）、入口（T2）、工具栏、弹层、选择器、
  编辑器与粘贴（T3）、实测与文档（T4）。
- 接口链：核心形状（T1）-> 入口（T2）-> 界面（T3）-> 实测与文档
  （T4）。
- 风险：可编辑 run 的放宽不得解冻 Table/Quote/Signature 块（它们
  保持保留）；未改动切换规则需要播种签名区间上的脏标记；识别器绝
  不得在链接 run 内触发；默认账户偏好不得捕获回复/转发请求；
  mailto 路径不得让 OAuth 流程所依赖的单实例参数转发退化。
