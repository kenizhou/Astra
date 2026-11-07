> 非规范性译本，仅供审阅；以英文版为准。

# SP2x 计划：表面收尾

规格：docs/superpowers/specs/2026-11-07-sp2x-finishers-design.md。分支
`sp2x-finishers` 自文档提交处的 main 切出。每个任务：先红灯、实现、
全量电池、一个约定式提交。

### 任务 1：小型核心行（D1 的动词、D2 的抓取、D3 的计时器）

**文件：** crates/astra-app/src（经 edit_row 的 MailEdit::delete 之上的
delete_message 动词、意图；引导的每账户 spawn 直接 join 日历刷新体）；
client/desktop/src（投递循环旁的周期计时器：五分钟一跳、已连接门控、
带限度的环境覆盖）；伪件。

- [ ] 先写测试；提交 `feat: feed the finisher core`。

### 任务 2：草稿重新打开（D4）

**文件：** crates/astra-app/src/drafts（最新条目旁带三道门的按键
resume_draft；DraftResume 以转发自己的形状增长携带部件字段）；
client/desktop/src/commands/drafts.rs（按键命令）；client/web/src
（命令之上的两个行菜单动词、以转发形状渲染的预附行）；线路。

- [ ] 先写测试；提交 `feat: reopen a draft row into the composer`。

### 任务 3：缩放与演示（D5、D6）

**文件：** client/web/src/features/composer（渲染的内联图片；写
width_px 的缩放覆盖层指针手势）；client/desktop/src（调试门控的演示
旗标：应答已记录快照的命令层、屏幕选择器环境变量）；夹具的冻结
日期。

- [ ] 先写测试；提交 `feat: resize the picture and boot the showcase`。

### 任务 4：实测证明 + 文档

**文件：** 带门禁的实测（测试服务器上观察到的删除动词；启动抓取
填充日历；草稿重新打开往返）；docs（drafts.md、缩放的 sending.md 或
composer-security.md、debugging.md 或新的演示文档）+ zh；
capabilities 各行（六行）；fragment。

- [ ] 提交 `feat: prove the finishers live and record it`。

### 任务 5：收尾

- [ ] 全量电池；推送分支 `sp2x-finishers`；PR（预期 squash）；分类账
  清理。

## 自审记录

- 规格覆盖：动词 + 抓取 + 计时器（T1）、重新打开（T2）、缩放 + 演示
  （T3）、实测与文档（T4）。
- 风险：永久删除不可逆（确认的句子必须如此说、动词绝不乘批量循环）；
  启动抓取不得触发日历表面信号（调体不调意图）；周期计时器加入进程
  生命周期（AppState 释放顺序陷阱）；按键恢复的门必须指名普通邮件
  （行键可以指名它）；携带行绝不再下载字节（部件 id 载体）；缩放
  覆盖层必须活在内容之外（铬件绝不序列化进消息）；演示门限必须仅
  调试构建且绝不触碰已交付的别名。
