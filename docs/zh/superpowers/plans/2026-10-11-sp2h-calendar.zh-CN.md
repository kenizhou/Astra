# SP2h 日历实施计划（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/plans/2026-10-11-sp2h-calendar.md`）。
> 代码块、命令、文件路径与标识符保持英文原样。

> **执行者须知：** 必须配合 superpowers:subagent-driven-development（推荐）或
> superpowers:executing-plans 技能逐任务执行本计划。步骤使用复选框
> （`- [ ]`）语法跟踪。

**目标：** 端到端交付日历：带快照的 Calendar surface、引擎日历读写之上的
Intent 家族、重复处理、带 iMIP 回退的邀请流、网格/日程 UI、会议简报、
CalDAV 活体验证、契约文档。

**架构：** astra-app 拥有 CalendarIntent 派发、基于引擎日历读的快照重
建、邀请状态机与简报服务；引擎拥有日历、出现展开、事件持久化与回复提
交。
**技术栈：** 既有 workspace；68eab00 的引擎日历面（先读）；Stalwart 的
CalDAV（核实）作活体验证。
**Spec：** `docs/superpowers/specs/2026-10-11-sp2h-calendar-design.md`

## 全局约束

- 沿家规：SPDX、500 行上限、deny-everything lints、提交前 gate（直接退
  出码）、conventional commits、无破折号、英式英语、zh 镜像同提交、只允许
  增量线上变更。
- 常设：PR 合并即自动继续；SQUASH AND MERGE；LLM 转发按请求（简报活体
  测试需要）；作者 10 分钟窗口。
- 引擎拥有协议与重复展开；Astra 渲染与派发。

---

### 任务 1：引擎勘察 + 协议/快照类型
**文件：** 报告里的勘察记录（无代码）；`crates/astra-app/src/protocol/
intent_calendar.rs` + 测试；`crates/astra-viewmodel/src/calendar.rs` + 测
试（新建；独立于 viewmodel 行）。
**接口：** CalendarIntent 家族（serde snake_case 钉死）；
`CalendarSnapshot { calendars, occurrences, write_states, now_data }` 形状
（视图类型最小化：日历行、带重复标记的出现行、写状态）。
- [ ] 步骤 0：勘察（读 68eab00：calendars、occurrences_in、
  events_by_keys、提交/编辑路径、邀请回复、can_write 呈现；harness 镜像
  的 Stalwart CalDAV 可用性）。发现记入报告；绑定任务 2-3。
- [ ] 测试先行；实现；提交 `feat: add the calendar protocol and snapshot
  types`。

### 任务 2：派发 + 引擎上的快照
**文件：** `crates/astra-app/src/calendar/{mod.rs, snapshot.rs, edit.rs,
invite.rs}` + 测试（派发臂；快照重建；CRUD 映射；邀请状态机；写门控；
拖拽数学）。
- [ ] 按 D2-D4/D6 单元清单测试先行；实现；提交
  `feat: dispatch calendar intents over the engine`。

### 任务 3：简报 + 宿主/ipc
**文件：** `crates/astra-app/src/calendar/brief.rs` + 测试；
`client/desktop/src/commands/`（+ 日历拉取命令或按勘察的 surface 拉取）+
测试；`client/web/src/services/**`（+测试；ipc.ts 的拆分先行——它已在
500）。
- [ ] 测试先行；实现；提交 `feat: add calendar briefs and the host
  commands`。

### 任务 4：日历 UI
**文件：** `client/web/src/features/calendar/**`（视图切换入口、日程、三
种网格、事件对话框、邀请卡、简报面板）+ 阅读窗邀请卡挂载 + 测试。
- [ ] 按 D1/D4/D5 UI 清单测试先行；实现；提交
  `feat: render the calendar views and invitations`。

### 任务 5：活体验证 + 文档
**文件：** 门控活体日历测试（按勘察的 CalDAV 往返；RSVP；转发 LLM 的种
子事件简报）；`docs/calendar.md` + `docs/invitations.md` + zh；
capabilities + zh（日历、邀请、会议简报行移动）；fragment；全量验证。
- [ ] 提交 `feat: prove the calendar live and record the contracts`。

### 任务 6：收尾
- [ ] 全量验证；推 `sp2h-calendar` 分支；开 PR（预期 squash）；验收证
  据；清台账。

## 自审记录

- Spec 覆盖：D1（T1 类型/T4 UI）、D2（T1/T2）、D3（T2）、D4（T2/T4）、
  D5（T3/T4）、D6（T2/T4）、活体 + 文档（T5）。
- 接口链：勘察（T1）绑定一切；类型（T1）-> 派发/快照（T2）-> 简报/
  ipc（T3）-> UI（T4）-> 活体 + 文档（T5）。
- 风险：引擎日历面宽广（勘察决定什么现在移植什么推迟，绝不静默）；
  Stalwart CalDAV（核实；回退是记录的缺口）；网格复杂度（文件硬拆分）；
  ipc.ts 拆分必须在 T3 加线之前落地。
