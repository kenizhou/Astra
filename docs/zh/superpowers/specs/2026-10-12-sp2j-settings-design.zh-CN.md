> 非规范性译本，仅供审阅；以英文版为准。

# SP2j 设置设计

日期：2026-10-12
状态：proposed
父规范：`docs/superpowers/specs/2026-08-...-workflow-design`（第 5、15 节）
移植契约：实质移植 mail-calendar 的 `docs/settings.md`（分类法的形态与各类
别的规则），按 Astra 今日交付的范围加上点名本切片的已记录缺口（向导的
dav 字段；日历的持久化形状与时区；外观持久化）。

## 1. 目的

设置作为一个表面：带 Astra 已有设置项的分类的统一屏幕，持久化在核心的
偏好文档中，并由核心在需要处消费（日历的显示时区、每周第一天、视界），
带 dav 节编辑器的账户页（关闭 contacts.md 的向导缺口），以及 About。

## 2. 决策

- **D1（表面）**：`Surface::Settings` 携带 `SettingsSnapshot`（持久化的设
  置块、账户行、About 块）。外壳标题栏增加入口（面板旁的齿轮）。屏幕按
  桌面形态呈现：分类源列表旁挂详情面板，模态覆盖外壳。
- **D2（意图族）**：`Settings(SettingsIntent)`：`Refresh`、`SetTheme {
  mode }`、`SetClock { twenty_four }`、`SetZone { zone }`、
  `SetFirstDayOfWeek { weekday }`、`SetHorizon { days }`、`SetQuoteStyle
  { below }`、`SetViewMode { mode }`（分组移入设置；既有的列表臂委
  托）、`SetDefaultCalendar { calendar }`、`SetDav { account, section }`
  （编辑器；`None` 清除）、`RemoveAccount { account }`。每个意图先持久
  化进偏好文档再重新发布。
- **D3（持久化与消费）**：设置存于偏好文档（TOML，只增字段带默认；旧
  文件干净加载）。核心消费属于它的：日历的显示时区（替换硬编码
  `Etc/UTC` 的待定注释）、每周第一天（网格布局与创建路径的 WKST）、视
  界默认（冷窗口）、默认日历（创建对话框预选的可写目标）。宿主 chrome
  消费其余（主题、时钟、引用样式、分组）。
- **D4（账户页）**：每账户一行（地址、类型、用途含已拨号的腿），各自打
  开自己的页面：dav 节编辑器（来源 + 用户名 + 密码；CardDAV 按拨号规则
  回退到 CalDAV 来源；保存即重拨号并安装两条腿，无需重启；重拨失败保留
  旧腿并如实说明），以及移除账户（破坏性，须点名地址确认，删除存储配置
  并拆除账户状态；邮件半边与腿一同离去）。
- **D5（About）**：版本（`VERSION` 文件与工作区版本，同一来源）与致谢
  （引擎、sidecar 的 pi SDK、web 栈），核心的答案而非 web 的猜测。
- **D6（外观）**：主题默认跟随宿主，Light 与 Dark 为显式覆盖，持久化；
  时钟 12 或 24 小时制，持久化，由网格标签与阅读时间戳消费。

## 3. 测试

- 单元（astra-app，fake）：设置意图持久化并重新发布；偏好带默认往返；
  日历的消费（设定时区流入快照的 `now_data.zone` 与展开；设定的第一天
  流入网格窗口与创建 WKST）；dav 编辑器（好节重拨并安装；失败保留旧腿
  并回答失败；清除即移除）；移除账户（存储配置离去，账户的行与腿去）。
- 线协议：SettingsIntent 族 + SettingsSnapshot 字面量钉住，双语言。
- 前端（vitest）：源列表与详情；选择器（主题、时钟、带浏览器 IANA 列表
  的时区、第一天、视界）；账户页（行、dav 编辑器的确认与失败状态、点名
  地址的移除确认）；About。
- 实测（门控，harness）：对运行中的 harness 跑 dav 编辑器（编辑节、重拨
  安装两腿、日历与联系人填充）；无其他实测需求。

## 4. 已知缺口（预记录）

- Astra 没有对应功能的分类保持缺席，绝不做桩：签名、通知、隐私/分析、
  诊断、滑动操作、默认发件账户、每账户抓取深度与消息大小、发件名称字
  段、本地化、Allodia 账户、默认邮件应用。
- Advanced（重置与 MCP 面板）等 MCP 切片。
- 时区列表是浏览器自带的（`Intl.supportedValuesOf`）；宿主无法解析的时
  区按类型拒绝。
- 设置屏幕是桌面形态（源列表旁挂详情）；不建 hub-and-spoke 变体。
