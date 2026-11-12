# 引用折叠设计：折叠被引用的原文

**状态。** 2026-11-12 定稿。切片自此提交从 main 分出。

**范围。** 下一个 kylins 增补：在阅读窗折叠被引用的原文，使回复或转发
打开时呈现新写的文字，被引用的尾部折叠在一个「Show quoted text」开关
之后。它移植 kylins 的引用折叠形态（一个消毒后的纯字符串变换，发出原生
details/summary）；是增补，不是 mail-calendar 的对齐债。它完全位于 web
阅读路径：无消毒器改动、无 wire 类型、无 intent、无面向模型改动，故不
评测、不提升 pipeline。

## D1：变换

一个新纯模块 `client/web/src/components/reading-pane/collapseQuoted.ts`
（kylins 的为三十行）：`collapseQuoted(html)` 返回把尾部引用链折叠进
`<details class="astra-quote"><summary>Show quoted text</summary>...`
的 html（无 `open` 属性，故默认状态为折叠；开关是浏览器原生
details/summary，无需脚本）。

检测，镜像 kylins 并为 Gmail 拓宽：

- 锚定在 html 最末尾的一个或多个 `<blockquote>...</blockquote>` 块的尾部
  链（大小写不敏感、非贪婪）。
- 紧接其前的归属行，一个持有 `On ... wrote:` 的尾部 `<p>` 或 `<div>`，
  与链一起折叠。
- 一个尾部 `.gmail_quote` 块作为替代引用形状（该 class 在 Astra 的消毒器
  下存活，且 Gmail 转发常见）。

两个安全空操作，kylins 自己的：无尾部引用则 html 原样通过；整篇正文即
一个引用则绝不折叠（没有新内容可读）。注入的标记是一个固定常量字符串：
无发件人派生的属性值进入其中，且片段是被包裹，绝不被重新解析或重新编码
（敌意逐字契约保持）。

## D2：接线

变换在 `MessageHtml` 的组合点运行，于 `wrapBodyDocument` 之前，作用于被
门控的 html（远程图像占位符完好）。`enableRemoteImages` 随后如今天在选择启用时运行；折叠只包裹，故
`data-astra-src` 与
`data-astra-style-url` 令牌不受影响，既有重写仍能在 details 内找到它们。
被拦截图像计数的提示不受影响。

它刻意不在 `wrapBodyDocument` 之内：`PrintWindow` 直接调用该包装器，必须
保持完整正文（kylins 也打印完整正文）。阅读窗与弹出窗都经
`ReadingView`/`MessageHtml` 免费获得折叠。

框架按契约保持无脚本（`sandbox="allow-same-origin"`、
`default-src 'none'`）；details/summary 是那里唯一可用的交互折叠，CSP 与
sandbox 常量不移动。Rust 消毒器（ammonia）无需改动：折叠是消毒后注入的
应用自有标记。（记录在案：ammonia 允许 details/summary 与 class 属性，但
不允许 `input` 也不允许 `open` 属性，故经 Rust 通道的复选框 hack 或默认
展开折叠都行不通；本切片两者都不依赖。）

## D3：契约

`docs/reading-zoom.md` 是决定正文变换的包装器契约文档；本切片在其中加入
一段简短的引用折叠节（检测、默认折叠的折叠、打印豁免、逐字保证），并镜像
到 zh。一个新的 Mail 区 capabilities 行落在阅读行旁，带其 zh 镜像。
porting-backlog 的「引用折叠」条目随切片解决。一个变更日志碎片落地。

## 已知缺口，记录而非隐藏

- 整个尾部链一个折叠（kylins 自己的形态）；多个分离块的逐引用折叠与
  正文中部引用不在内。
- Yahoo/Outlook 标记行与非英文归属不在内；检测是 blockquote、英文
  「On ... wrote:」行与 gmail_quote。
- 无持久化的展开状态：details 状态活在框架文档里，随邮件重置，如远程
  图像 opt-in。
- 打印保持完整正文（折叠仅限阅读）。
- 尚无禁用折叠的设置偏好。
- 一个既有的悬空引用：capabilities.md 引用 `rendering-security.md`，该文件
  从未创建；安全姿态活在 sanitise.rs 模块文档、MessageHtml 头部与
  reading-zoom.md。在此记录，不由本切片修复（一次文档卫生通行）。

## 测试先行

每个行为都带着它的红落地，基于纯变换钉子模式（MessageHtml.test.tsx 逐字节
钉住 wrapBodyDocument 与 enableRemoteImages）。红：尾部 blockquote 链被
折叠进一个折叠的 details；归属行折叠在内；gmail_quote 尾部被折叠；正文
中部 blockquote 与无引用正文原样通过；整篇引用绝不折叠；远程图像占位符
在折叠后存活且 opt-in 重写仍能在 details 内恢复 src；敌意片段逐字通过
（无重新编码）。一个 MessageHtml 组件测试钉住 srcdoc 携带 details 且打印
（仅 wrapBodyDocument）不折叠。无_live_模型证明（纯 UI）；离线电池即证明。