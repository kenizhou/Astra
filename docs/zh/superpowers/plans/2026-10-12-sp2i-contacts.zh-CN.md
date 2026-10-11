> 非规范性译本，仅供审阅；以英文版为准。

# SP2i 联系人实施计划

> **给代理工作者：** 必须使用 superpowers:subagent-driven-development（推荐）
> 或 superpowers:executing-plans 按任务逐条执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 端到端交付联系人：带快照的 People 表面、经引擎人员/联系人读写
的意图族、合并披露、编辑器的命名卡片纪律、带药丸的撰写自动补全、头像、
启动时拨号两条腿的 DAV 账户扩展、CardDAV 实测证明、以及契约文档。

**架构：** astra-app 拥有 PeopleIntent 分发、经引擎人员查询的快照重建与写
入映射；astra-account 增加 DAV 腿；引擎拥有合并、排序、匹配与照片缓存。
**技术栈：** 现有工作区；68eab00 处的引擎联系人表面（先读）；实测用
Stalwart 的 CardDAV（待验证）。
**规范：** `docs/superpowers/specs/2026-10-12-sp2i-contacts-design.md`

## 全局约束

- 各切片通用的内部规则：SPDX、500 行上限、deny-everything lint、提交前跑
  gate（直接退出码）、约定式提交、无破折号、英式英语、zh 镜像同提交、
  线协议只增。
- 常设指令：PR 合并即自动继续；SQUASH AND MERGE；LLM 中继按需（实测需要）；
  10 分钟作者窗口。
- 引擎拥有合并、排序、匹配、分节与首字母；Astra 渲染与分发。其上绝不叠
  加客户端匹配。

---

### 任务 1：引擎调研 + 协议/快照类型
**文件：** 报告中的调研笔记（无代码）；`crates/astra-app/src/protocol/intent_people.rs`
+ 测试；`crates/astra-viewmodel/src/people.rs` + 测试（新增）。
**接口：** PeopleIntent 族（serde snake_case 钉住）；PeopleSnapshot 形态
（带披露事实与节字母的行；带来源、出处、可编辑卡片的详情；写入状态；
创建目标）。
- [ ] 步骤 0：调研（读 68eab00：people_page/PeopleQuery/PeoplePage、
  person/person_sources、contact_card、contact_targets/editable_cards 等
  价物、create/patch 形态、recipient_suggestions、contact_photo；harness
  中 Stalwart 的 CardDAV 可用性）。记录发现；它们绑定任务 2-3 与 5。
- [ ] 先写测试；实现；提交 `feat: add the people protocol and snapshot types`。

### 任务 2：经引擎的分发 + 快照
**文件：** `crates/astra-app/src/people/{mod.rs, snapshot.rs, write.rs, suggest.rs}`
+ 测试（分发臂；重建；带命名卡片与 patch 纪律的 CRUD 映射；建议排序透
  传）。
- [ ] 按 D2-D4 单元清单先写测试；实现；提交
  `feat: dispatch people intents over the engine`。

### 任务 3：DAV 账户扩展 + 宿主/ipc
**文件：** `crates/astra-account/src/dav.rs`（+ 测试；配置节、拨号两条腿
的连接路径）；`client/desktop/src/{boot.rs,commands/people.rs}`（+ 测试；
启动安装拨到的每条腿；people 拉取、人员详情、建议与 CRUD 命令）；
`client/web/src/services/people.ts`（+ 测试；线协议镜像）。
- [ ] 先写测试；实现；提交 `feat: dial the dav legs and host the people commands`。

### 任务 4：人员 UI + 自动补全
**文件：** `client/web/src/features/people/**`（视图切换入口、带分节与披
露的列表、详情、带卡片选择的编辑器）+ 测试；撰写自动补全（token 补全、
药丸、悬浮列表、插入符规则）+ 测试；头像（首字母 + 照片抓取）落在联系人
行与阅读头。
- [ ] 按 D1/D3-D5 UI 清单先写测试；实现；提交
  `feat: render people, autosuggest, and avatars`。

### 任务 5：实测证明 + 文档
**文件：** 门控的 live people 测试（CardDAV 同步读取、创建/编辑往返、带
实测 `In 2 accounts` 披露的跨账户合并人员、已播种已发历史上的自动补全）；
`docs/contacts.md` + zh；capabilities + zh（联系人、自动补全、头像、以及
日历宿主各行）；fragment；全量电池。
- [ ] 提交 `feat: prove people live and record the contracts`。

### 任务 6：收尾
- [ ] 全量电池；推送分支 `sp2i-contacts`；PR（预期 squash）；验收证据；
  分类账清理。

## 自评注记

- 规范覆盖：D1（T1 类型/T4 UI）、D2（T1/T2）、D3（T2/T4）、D4（T2/T4）、
  D5（T2 建议 + T4 撰写）、D6（T3）、D7（T4）、实测 + 文档（T5）。
- 接口链：调研（T1）绑定一切；类型（T1）-> 分发（T2）-> DAV/宿主（T3）->
  UI（T4）-> 实测 + 文档（T5）。
- 风险：引擎人员模型的广度（调研决定现在移植什么、延期什么，绝不静默）；
  撰写自动补全触碰既有收件人字段（药丸重构必须保住线协议钉住的一字符
  串契约）；DAV 扩展触碰启动（日历表面的解缺不得回归邮件拨号）；
  Stalwart CardDAV（待验证；回退是记录的缺口）。
