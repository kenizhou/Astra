# SPA0 + SP-CLI 设计：关闭两个占位行

**状态。** 2026-11-11 定稿。切片自此提交从 main 分出。

**范围。** 矩阵中最后两个未启动方块，均在 CLI 一节：`Virtual permission
tools mapping`（SPA0，capabilities.md:152）与 `Shell completions`
（SP-CLI，capabilities.md:153）。前者把 sidecar 隐式的工具到权限映射变成
显式、穷尽、由测试钉住的契约工件；后者交付一个从 CLI 自身词表生成的
`completions <shell>` 动词。两者都不改动面向模型的工具面，因此都不触发
AI 评测规则（该判断记录在 D1 下）。

## D1：权限门控映射（SPA0）

所谓「虚拟权限工具」，是一个被门控的工具面条目，其执行悬挂在人工审批
卡片之后；创始规格（2026-10-08-astra-ai-driven-workflow-design.md:211,226）
命名了该映射，并要求 `docs/cli.md` 以「权限门控映射」承载它。如今映射是
隐式且分散的：工具名到 CLI 动词在 sidecar 的 `mailTools.ts` 执行体里，
工具名到策略级别在宿主的 `default_policy()`（agent/mod.rs:329），工具名
到脱敏预览形状在 `shaped_arg_preview`（agent/permissions.rs:207）。桥接
器对任何不在策略表中的工具已然失败即关闭；缺的是显式契约，以及让三个
面都为每个工具作答的钉子。

本切片交付：

- **映射本身。** `docs/cli.md` 新增「The permission gating map」一节：
  每个 sidecar 工具一行，穷尽列出其 CLI 动词与子动词、策略级别、脱敏
  预览形状。十个工具：四个读直接运行（`list_folders`、`list_messages`、
  `read_message`、`search_messages` 为 `auto`），六个写需询问
  （`save_draft`、`discard_draft`、`mark_read`、`set_flagged`、
  `archive_message`、`move_to_trash` 为 `confirm`）。该节陈述常驻规则：
  不在映射中的工具在桥接器里失败即关闭，新增工具会迫使映射、策略表与
  预览词表都为它作答（docs/plugins.md:66-81 的穷尽匹配先例）。
- **宿主钉子。** 一个测试把 `default_policy()` 钉为恰好十个工具、处于
  文档所述级别。`shaped_arg_preview` 为如今落入通用 `fields=N` 默认的
  五个工具（`search_messages`、`mark_read`、`set_flagged`、
  `archive_message`、`move_to_trash`）补上显式分支；通用回退保留给真正
  未知的工具（失败即关闭）。一个测试钉住全部十个形状。
- **sidecar 钉子。** 一个测试钉住 `createMailTools` 恰好产出这十个名字，
  且每个工具的 `buildCliArgv` 动词与子动词与映射一致。

**不评测的判断，记录在案。** 五个新预览分支改动的是宿主审批卡片的预览
字符串与权限审计行的预览，二者皆在宿主侧。面向模型的工具面（十个工具
定义、其参数、其 argv）不变，也没有策略级别移动。AI 规则约束的是
「prompt、pipeline、工具面」的改动；此切片三者皆非，故不附带评测运行，
也不提升 pipeline 版本。sidecar 与 CLI 的钉子新增的是测试，而非行为。

**该行移为已交付。** 映射（工具到动词到级别到预览形状）在每个面上都
完整、显式、被钉住，故 capabilities.md:152 由 ⬜ 移为 ✅，Notes 指向
cli.md 的门控映射。预览的「生产」腿是另一回事，作为已知缺口记录在下，
并非映射本身的短板。

## D2：shell 补全（SP-CLI）

新增 `completions <shell>` 动词，从 CLI 自身词表为 bash、zsh、fish、pwsh
生成补全脚本：动词词来自 `Verb::name()`，子动词词来自 `SUBVERBS`
（parse.rs:398），再加一份逐动词的标志描述符。该动词不打开引擎即可作答
（store 动词的纪律），非 mutating（`mutating()` 为 false，故共享门控
拒绝 `--preview`），并如每次调用一样被审计。

**两处偏离，记录而非沉默（规则书，上游同步）。**

- 创始规格（2026-10-08:221）说补全「用 clap_complete 生成」。已交付的
  解析器是手写、无 clap 的；引入 clap 会重写解析器。本切片改为从现有
  词表生成，保持代码库以测试钉住的单一事实源纪律。该偏离记录于此、
  记录于 cli.md、记录于 PR。
- 信封规则（cli.md:153）说每次调用打印一个 JSON 信封。补全是一个面向
  人类、sidecar 永不调用的生成器，故默认打印原始脚本（`eval "$(astra-cli
  completions bash)"` 惯用法），在 `--json` 下包进标准信封（`data` 携带
  `shell` 与 `script`，schema 仍为 1）。这是信封规则唯一的生成器例外；
  cli.md 的信封一节修订以陈述它。面向代理的统一信封完好，因为 sidecar
  永不调用 completions。

**标志词表一式两份，出于设计。** 动词与子动词词有单一来源
（`Verb::name()`、`SUBVERBS`）；逐动词标志内联在 parse 的匹配分支里，
故补全描述符第二次拼写它们，由一个漂移测试把两者钉在一起，即
`CLOSED_ACTIONS` 先例（cli.md:145-149）。对 `Verb` 的穷尽匹配迫使每个
动词都有一个描述符条目，故新动词不可能不带其补全词表就交付。

**该行移为已交付。** capabilities.md:153 由 ⬜ 移为 ✅；cli.md 的动词表
新增 completions 行（计数由三十一移为三十二），矩阵新增 completions 行，
信封一节注明生成器例外。

## 已知缺口，记录而非隐藏

- **预览生产腿（SPA0）。** 桥接器与_wire_协议端到端携带 `confirm_preview`
  与审批 `preview` 载荷（protocol.ts:89-97、protocol.rs:56、
  permissions.rs:31），但没有任何东西生产它们：`runGated` 不向
  `requestApproval` 传预览，sidecar 从不拼 `--preview`，`default_policy()`
  不用 `ConfirmPreview`，审批卡片渲染的是脱敏参数而非动作预览。把
  `--preview` 输出接到卡片上是一个更深的切片（confirm_preview 簇）；
  本切片记录映射所承载的脱敏预览形状，并把生产腿记为缺口。
- **SP-CLI 的更宽范围。** 创始规格的 SP-CLI 行命名「补全、帮助质量、
  性能」（2026-10-08:505）。本切片只关闭「Shell completions」行；帮助
  质量与性能留给后续 SP-CLI 切片。
- **标志描述符的重复成本。** 逐动词标志在 parse 与描述符中各拼一次，
  由漂移测试钉住；其成本即封闭动作集自身的成本，记录于 docs/ai-rules.md。

## 测试先行

每个行为都带着它的红落地。SPA0：策略表钉子（恰好十个工具、文档所述
级别）、十个预览形状（五个新分支与五个既有）、sidecar 的十名与 argv
钉子；离线代理 e2e 与策略测试保持绿（e2e 演练 `save_draft`，其形状不
移动）。SP-CLI：四个 shell 各产出一份命名每个动词的非空脚本；未知 shell
是用法错误（exit 2）；`--json` 作答 schema-1 信封，带 data.shell 与
data.script；`--preview` 是共享的非 mutating 用法错误；原始形式打印脚本
而非信封；描述符覆盖每个动词并对代表性动词与 parse 一致；审计行带着
_turn_ 的相关 id 落地。本切片不附带_live_模型证明（没有面向模型的行为
移动）；离线电池即证明。