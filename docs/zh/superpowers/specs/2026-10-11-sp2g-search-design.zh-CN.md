# SP2g 搜索设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-11-sp2g-search-design.md`）。

日期：2026-10-11
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 5、15 节）
移植契约：mail-calendar `docs/search.md` 实质移植。

## 1. 目的

找邮件：查询 DSL（`from:`/`to:`）、线程分组最新优先结果、范围过滤、同步
深度披露、退回文件夹视图。引擎的搜索面服务；Astra 渲染。

## 2. 决策

- **D1**：两个 Intent 进协议：`Search { query: Option<String> }`（None 清
  除、返回文件夹视图）与 `SetSearchScope { scope: SearchScope }`（按 MC：
  默认除 Trash 外全部文件夹、当前文件夹、当前账户）。快照增量增长：
  `search: Option<SearchState>`，SearchState 携带结果行（线程分组、最新优
  先）、活动范围、深度注记（以整月计的已搜地平线）、查询回显。搜索模式
  的行复用消息行视图类型。
- **D2**：查询处理诚实分工：引擎的 `search_mail` 拥有匹配（协议与文本搜
  索归引擎）；APP 解析 `from:`/`to:` 前缀过滤（先读锁定 rev 的引擎搜索
  签名再适配：若引擎收结构化过滤则传入；若不收则过滤在 APP 侧收窄——
  落地哪种记录哪种）。自由文本剩余部分透传。
- **D3**：深度披露是诚实数据：已搜地平线（账户已同步窗口、整月量化，MC
  的三个月默认随既有同步深度）渲染进搜索状态；结果绝不声称超出它。
- **D4**：搜索框位于标题栏中央（kylins Ambient 模式适配）：输入去抖
  （500 ms）派发 Search；Escape 清除（派发 Search None）；范围过滤以
  下拉随框。消息列表渲染结果行带线程分组与深度注记；选中结果照常在阅
  读窗打开。
- **D5**：线程分组：引擎返回消息；APP 按线程 id 分组成带计数的行视图，
  最新线程优先，匹配 MC 的线程分组结果契约。

## 3. 测试

- 单元（astra-app，内存引擎 + 假件）：Search(Some) 发布搜索状态并抑制
  文件夹列表状态（快照字段增量、渲染器择取）；Search(None) 返回；
  SetSearchScope 重跑；DSL 分解（from:/to: 提取、自由文本剩余、畸形前
  缀容忍）；深度注记（同步窗口量化）；线程分组与排序。
- 线上：两个 Intent 与搜索状态字面量双侧钉死。
- 前端（vitest）：搜索框（去抖、Escape、范围下拉）、结果渲染（分组行、
  深度注记、计数）、选中打开阅读窗、退出返回文件夹列表。
- 活体（门控，harness）：经真实引擎对种子邮件的真实搜索（主题命中；
  from: 过滤命中；无命中态）。

## 4. 验收标准

1. gate、workspace 测试、cargo doc、desktop 构建、web 构建、vitest、
   sidecar 测试全绿。
2. 离线：完整搜索生命周期钉死（输入 -> 结果 -> 换范围 -> 选中 -> 退
  出）；DSL 分解钉死；深度注记钉死。
3. 活体：经 harness 的真实搜索命中种子邮件。
4. `docs/search.md`（+zh）实质移植（DSL、范围集、分组、深度披露、退
  出），capabilities 行随镜像移动，changelog fragment 落地。
5. 只允许增量线上变更；既有夹具与测试原样通过。
