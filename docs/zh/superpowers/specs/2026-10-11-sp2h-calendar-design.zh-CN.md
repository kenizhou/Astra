# SP2h 日历设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-11-sp2h-calendar-design.md`）。

日期：2026-10-11
状态：proposed
父 spec：`docs/superpowers/specs/2026-08-astra-ai-driven-workflow-design.md`（第 5、15 节）
移植契约：mail-calendar `docs/calendar.md` 与 `docs/invitations.md` 实质移植。

## 1. 目的

日历与事件：账户的日历、日程与日/工作周/月网格、事件建/改/移/缩放/删、
重复规则回读与单次/全部编辑、带 RSVP 与 iMIP 回退的会议邀请、can_write
写门控、以及自 A4 推迟回归的会议简报（它们因日历数据而推迟）。

## 2. 决策

- **D1（surface）**：`Surface::Calendar` 携带 `CalendarSnapshot`（日历
  管理集、可见范围的出现集、写状态、now 线数据）。壳的视图切换在 Mail/
  Contacts 旁增加 Calendar。SPA 渲染日程加日、工作周、月网格（kylins 布
  局栈适配；日历 chrome 为 Astra 自有）。
- **D2（Intent）**：家族 `Calendar(CalendarIntent)`：`Refresh`、
  `SetRange{start, days}`、`SetVisibleCalendars{ids}`、`CreateEvent{...}`、
  `UpdateEvent{event, edit: EventEdit}`、`MoveEvent{event, drag}`、
  `DeleteEvent{event, occurrence: Option}`、`RespondToInvitation{...}`、
  `SetDate{date}`（跳转今天/导航锚）。引擎的日历读写面服务（先读
  68eab00：calendars、occurrences_in、events_by_keys、提交路径、邀请回
  复）。
- **D3（重复）**：RRULE 回读经引擎的出现展开；单次/全部的范围编辑与
  删除（带服务器会丢弃单次编辑时的系列警告）；网格中单次准确的日期。
- **D4（邀请）**：阅读窗正文上方的邀请卡、经引擎回复路径的接受/待定/
  拒绝；拒绝的会议离开日历；未答复绘为 hold；陈旧邀请检测；服务器无法
  送达 RSVP 时的 iMIP 邮件回复回退；回复投递失败提示（MC 契约）。
- **D5（会议简报）**：`briefings(account, day)`（宿主侧；ECONOMY 档）
  基于日历事件与关联的与会者/消息上下文产出当日简报；日历视图增 Briefs
  动作。A4 的 capabilities 行移动。
- **D6（写门控）**：每个日历写咨询 can_write；surface 携带写状态（写
  入中、带重试的失败）按 MC；乐观网格移动在同步时对账。

## 3. 测试

- 单元（astra-app，内存引擎 + 假件）：范围上的快照；出现展开；事件
  CRUD Intent 到引擎调用的映射；拖拽/移动数学；单次/全部编辑拆分；邀请
  状态机（RSVP 结果、hold 绘制、陈旧、回退）；夹具事件上的简报服务；
  写门控 + 失败重试态。
- 线上：CalendarIntent 家族 + CalendarSnapshot 字面量钉死。
- 前端（vitest）：网格（日/工作周/月、now 线、横幅）、日程、事件建/改对
  话框、拖拽交互（诚实的 jsdom 近似）、邀请卡、简报面板。
- 活体（门控，harness）：Stalwart 提供 CalDAV（核实）；日历同步、经
  CalDAV 的事件往返（建 -> 改 -> 删）、RSVP 流程、种子事件上的简报。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建、vitest、
   sidecar 测试全绿。
2. 离线：网格/CRUD/重复/邀请生命周期钉死；简报服务钉死。
3. 活体：Stalwart 支持处经 harness 的 CalDAV 往返；否则诚实记录缺口并
   证明引擎支持的 provider 路径。
4. `docs/calendar.md` + `docs/invitations.md`（+zh）实质移植，
   capabilities 行移动（日历、邀请、会议简报），fragment 落地。
5. 只允许增量线上变更；既有夹具与测试原样通过。
