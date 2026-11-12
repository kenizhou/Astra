# SPA0 + SP-CLI 计划：关闭两个占位行

规格：docs/superpowers/specs/2026-11-11-spa0-sp-cli-closure-design.md。分支
`spa0-sp-cli-closure` 自文档提交从 main 分出。每个任务：先红、实现、
电池、一个约定式提交。两个特性都是契约变更：cli.md 与 capabilities.md
连同其 zh 镜像在同一提交移动，各带自己的变更日志碎片。

### 任务 1：权限门控映射（SPA0，D1）

**文件：** docs/cli.md（新增「The permission gating map」一节：穷尽的
十行表，工具 / 动词+子动词 / 级别 / 预览形状，失败即关闭规则，穷尽匹配
规则；把 251 处的矩阵行改为指向该节）+ docs/zh/cli.zh-CN.md；
client/desktop/src/agent/permissions.rs（五个新 `shaped_arg_preview`
分支：search_messages、mark_read、set_flagged、archive_message、
move_to_trash；通用回退保留；测试模块里的十形状钉子）；
client/desktop/src/agent/policy_tests.rs（`default_policy()` 钉子：恰好
十个工具、处于文档所述级别）；sidecar/src/tools/mailTools.test.ts（十名
钉子与逐工具 `buildCliArgv` 动词/子动词钉子）；docs/capabilities.md:152
⬜ 到 ✅ + zh；docs/changelog/unreleased/spa0-permission-gating-map.md
（Platforms: desktop，Bump: none）。

- [ ] 测试先行（策略钉子、十形状、sidecar 钉子红）；随后五个分支与文档
      映射；离线代理 e2e 与策略测试保持绿。提交
      `feat: pin the virtual permission tools mapping`。
- [ ] 行数警戒：permissions.rs 现 384 行；若分支加测试越过 500，把测试
      模块移到兄弟文件（自由函数，无 E0119 约束）。mailTools.ts 不动
      （钉子在其测试文件里）。无面向模型改动：不评测、不提升 pipeline。

### 任务 2：shell 补全（SP-CLI，D2）

**文件：** cli/src/verbs/verb.rs（`Completions { shell }` 变体、其
`name()` 分支 "completions"；`mutating()` 保持 false）；
cli/src/verbs/parse.rs（`completions <shell>` 匹配分支；未知 shell 为
用法错误；`--preview` 由共享门控拒绝）；cli/src/verbs/completions.rs
（生成器：从 `Verb::name()`、`SUBVERBS` 与逐动词标志描述符生成四个 shell
脚本；对 `Verb` 的穷尽匹配）；cli/src/verbs/completions_tests.rs
（run() 层的红）；cli/src/verbs/mod.rs（声明模块；在引擎打开前作答的
execute 分支，store 动词的纪律；run 结果新增原始脚本字段）；
cli/src/main.rs（存在原始脚本且无 `--json` 时打印它，否则打印信封）；
docs/cli.md（动词表行、计数三十一到三十二、矩阵 completions 行、信封
一节的生成器例外、记录在案的 clap 偏离）+ zh；docs/capabilities.md:153
⬜ 到 ✅ + zh；docs/changelog/unreleased/sp-cli-shell-completions.md
（Platforms: desktop，Bump: minor）。

- [ ] 测试先行（四个 shell 命名每个动词；未知 shell exit 2；`--json`
      schema-1 信封带 data.shell/data.script；`--preview` 用法错误；原始
      形式是脚本而非信封；描述符覆盖每个动词并对代表性动词与 parse 一致；
      审计行落地）；随后生成器与接线。提交
      `feat: ship shell completions from the verb vocabulary`。
- [ ] 行数警戒：parse.rs 现 438、verbs/mod.rs 现 429，加上小分支仍低于
      500；若四个 shell 模板加描述符在 completions.rs 越过 500，把模板
      拆到子模块。标志描述符一式两份（parse + 描述符），由漂移测试钉住，
      即 CLOSED_ACTIONS 先例。

### 任务 3：收尾

- [ ] 完整电池（直接退出码）：cargo fmt --all；cargo fmt --all -- --check；
      cargo clippy --workspace --all-targets -- -D warnings；cargo test
      --workspace；npm --prefix sidecar run build && npm --prefix sidecar
      test；npx tsc --noEmit && npx vitest run（client/web）；cargo run -p
      xtask -- gate。无_live_模型证明（没有面向模型的行为移动）。
- [ ] 分支推送；PR（预期 squash）；CI 观察（ci-ok、desktop、sidecar）；
      squash 合并、删分支、同步 main；ledger 清理。

## 自审笔记

- 规格覆盖：D1 的映射 + 宿主钉子 + sidecar 钉子 + 行（T1）；D2 的动词 +
  生成器 + 描述符 + 输出例外 + 行（T2）；电池 + PR（T3）。
- 风险：五个预览分支不得扰动离线 e2e（它钉 save_draft，其形状不动），
  也不得扰动策略测试；原始输出字段不得改动任何其他动词的信封（envelope_tests.rs 的
  字节钉子保持绿）；completions 动词必须不打开引擎即可作答（冷数据目录
  可用）；标志漂移钉子必须在 parse 与描述符不一致时失败。
- 偏离记录于规格、cli.md 与 PR：clap_complete 句（改为从词表生成）与信封
  规则唯一的生成器例外。不评测的判断（仅宿主侧预览，面向模型面不变）
  记录于规格。