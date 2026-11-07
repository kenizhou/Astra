# SP2y 计划：带工具预算的助手聊天

规范：docs/superpowers/specs/2026-11-07-sp2y-assistant-chat-design.md。分支
`sp2y-assistant-chat` 自文档提交处切出主干。每个任务：先红灯，再实现，再跑
全套，一个约定式提交。

### 任务 1：工具预算（C1）

**文件：** sidecar/src（提示词运行器的时钟、门控工具运行器的期限视图、看
门狗、budget-spent 状态事件）；client/web/src/features/agent（状态行与咬合
说明）；client/desktop/src/agent（事件折叠）；仿真相手；evals/（预算场
景）。

- [ ] 先测试；提交 `feat: bound the assistant's tools by the clock`。

### 任务 2：搜索与操作工具（C2）

**文件：** cli/src/verbs/messages.rs（mark-read、set-flagged、archive、
trash、search；各带 --json 与 --preview）；sidecar/src/tools（新工具、确认
门控、截断上限）；运行时适配器的允许表与技能索引措辞；桌面宿主的策略行。

- [ ] 先测试；提交 `feat: give the assistant the search and act tools`。

### 任务 3：持久化与线程（C3）

**文件：** crates/astra-ai/src/store.rs（聊天迁移、行、读取）；
client/desktop/src/agent（中继消息层的按聊天会话 id，载入、列出、重命名、
删除命令）；client/web/src/features/agent（线程列表，存储为唯一事实来
源）。

- [ ] 先测试；提交 `feat: keep the assistant's threads`。

### 任务 4：卡片、markdown、版本、评测（C4、C5）

**文件：** client/web/src/features/agent（结构化工具卡片、markdown 渲染、
依赖裁定）；运行元数据戳；evals/ 加流水线版本递增；文档（AI 规则的流水线
钉住小节、聊天的文档）加中文镜像。

- [ ] 先测试；提交 `feat: show the assistant's tools and stamp the run`。

### 任务 5：实证 + 行 + 片段

**文件：** 真件上的门控实证（一个回合搜索播种的邮件、经确认门操作一行、
在武装的短预算处失去工具、以 done 落地）；能力矩阵行加中文镜像；变更日志
片段。

- [ ] 提交 `feat: prove the chat live and record it`。

### 任务 6：收尾

- [ ] 全套检查；推送分支；PR（预期 squash）；台账清理。

## 自审笔记

- 规范覆盖：预算（T1）、工具（T2）、线程（T3）、卡片与版本（T4）、实证与
  行（T5）。
- 风险：完成信封保证必须在看门狗路径上成立（硬上限中止以 done 落地，绝不
  挂起）；超预算的工具调用要落审计行（绝不静默丢弃）；两个持久化存储不能
  都上屏（存储是唯一事实）；markdown 依赖是新的（沙箱化、无原始 HTML）；
  发送拒绝保持结构性（无动词、无工具、技能索引写明由人完成）。
