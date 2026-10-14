# 设计系统参考(评审副本,非规范性)

UI1 切片铸造的规则的常驻家园。`theme.css` 持有令牌;本页持有治理令
牌的规则。两者冲突时,令牌是错的,直到该规则在评审中被驳倒。

## 密度轴

文档根上的 `data-density`,由 `appearance.ts` 与 `data-skin`、
`data-contrast`、`data-font-size` 并列持有。两档:comfortable(行高
28px、点击下限 28px、窗格内边距 12px)与 compact(24px、24px、
8px)。comfortable 是默认值;属性缺省等同于 comfortable;持久化的未
知值回退到 comfortable。设置 General 页在 Font size 旁暴露该轴。

行高是下限:内容高于档位时把行撑高,绝不截断。行以 `min-height`
定高;任何地方不得有固定高度的文本行。消息列表自身的三档密度
(compact、normal、comfortable;44/56/72px 含预览行)仍是每个表面
的内容选择;密度轴治理的是外壳。

## 质感轴

文档根上的 `data-texture`,由 `appearance.ts` 与其余外观属性并列持
有。三档:warm(默认值;已批准的 A3 执行,深色为 hue 60 的暖石墨,
浅色为羊皮纸)、calm(hue 260 的姊妹档:真中性石墨、冷瓷白)与
classic(质感之前的冷石板色,即基础文件自身的值)。数值以属性作用
域覆盖块的形式住在 `theme.css`,位于高对比度章节之前,因此高对比
度始终压平质感。`--paper` 令牌让阅读窗格比画布轻一个安静的台阶;
阅读窗格直接消费它。设置 General 页在 Accent 旁暴露该轴。数值及其
证据记录于 `docs/designs/evolved-iris-texture.md`。

## 动效

时长标尺为 100/120/140/160ms(`--dur-fast`、`--dur-row`,
`--dur-view`、`--dur-panel`)。`--ease-spring` 只属于命令面板;其他
任何地方不得用它做缓动。`prefers-reduced-motion` 下一切降为即时,
骨架微光也不例外。

## 字体排印

表格数字(`font-feature-settings: 'tnum'`)用于每一处计数、时间戳
与时长。打包字体(UI1 D2)随 380/450/560 字重档位发布:细预览、中
等发件人、半粗主题。

## 状态族

单一 `EmptyState`(图形块、消息、可选主操作)替换所有裸空状态;各
表面文案集中在内容图(`components/ui/stateContent.ts`),即唯一事
实源。`SkeletonRows` 每窗格渲染三条,行高跟随密度轴。内联错误文案
点名问题、说明原因(如已知)、以恢复动作收尾;设置保存错误即范例。
设置在保存成功时保持静默。

## AI 语法

每个 AI 纸片带出处("from this thread"、"from your draft"、"from
today's inbox")、状态与始终显式的批准动作。生命周期五态:
pending(虚线)、generating(微光)、approve(强调色边框)、
approved(普通确认文字)、failed(琥珀边框加重试)。纸片只出现在
其行为已上线的表面。这与 sidecar 的失败即关闭姿态一致:没有纸片暗
示宿主未授予的能力。

## 账户调色板

八个默认色(`theme.css` 的 `--account-1` 至 `--account-8`),按账户
顺序轮转,两种主题同组色。八个账户以内的冲突可接受。扩展或重映射:
在 `theme.css` 中增改一个 `--account-N` 令牌;调色板没有设置界面。

## 外观属性命名

属性按概念命名、取词值(`data-density`、`data-contrast`)。
`data-font-size` 是既往不咎的例外,不是第二个先例:新属性不得仿效
其属性式命名。
