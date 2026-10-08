> 非规范性译本，仅供审阅；以英文版为准。

# AGENTS.md

Astra 的唯一规则手册，同时面向人类与编码代理。每条规则只存在于此处，
或存在于此处指名的那一份文档中；其余场合一律链接。

## 产品

Astra Mail 是一个桌面邮件客户端。产品名为 "Astra Mail"，用户可见文案中
绝不单独使用 "Astra"。名称与应用 id 在构建时取自 branding/default.env。

## 写作

- 简短具体。只说一次：一条规则只活在一个地方。
- 英式英语（`colour`，名词 `licence`，动词 `license`）。`dialog`、
  `artifact`、`catalog` 保持美式。
- 禁止以破折号作标点：任何地方不得出现 em 或 en dash，Markdown 中
  不得以带空格的连字符作标点。
- 代码注释只描述现状；不写历史，不写计划阶段，从简。日志行绝不提及
  仓库路径。
- 文档、代码、注释与文件夹名仅用英文。`docs/zh/**` 存放非规范性中文
  审阅副本；文档变更时，其审阅副本随同一提交变更。

## 硬性规则

- 源文件不超过 500 行。门禁对其扫描的源语言强制执行；Markdown 正文
  豁免。
- 身份一律用 newtype；跨模块边界不得出现裸 `String` id。
- 行为变更先写测试。缺陷修复随回归测试落地，测试放在能观察到该
  契约的最低共享层。
- 引擎只经 `engine-api` 消费。协议解析属于引擎，绝不在 Astra 中。
- Intent 保持可被 serde 序列化。Surface 是不可变快照。
- `docs/capabilities.md` 随其描述的功能在同一个变更中移动。
- `VERSION` 只在发布 PR 中移动，且始终与工作区包版本一起移动。

## 契约变更

一份契约在 `docs/` 下属于它的文档中一次性决定，并约束整个产品。
变更一份契约需要：(1) 文档规则与矩阵更新，(2) 每个交付它的 surface
上的实现，(3) `docs/capabilities.md` 的行，(4) `docs/changelog/unreleased/<slug>.md`
中的变更日志片段，带 `Platforms:` 与 `Bump:` 标签。缺口记录为已知
差距，绝不沉默。

## 变更日志片段

每个用户可见变更一个文件：`docs/changelog/unreleased/<slug>.md`，
内容为说明文字，随后是 `Platforms: desktop` 与 `Bump: none|minor|major`。
片段在发布 PR 中移入 `docs/changelog/released/`。片段仅用英文，
没有中文镜像。

## 安全

凡 kylins 实现占优之处，安全机制以 kylins 为准；功能契约以
mail-calendar 为准。sidecar 失败即关闭：每个受门控的工具初始为
needs-approval，发送类动词对代理一律拒绝，由人类决定并发送。每次
LLM 调用、工具调用与权限决定都带关联 id 落入其台账。

## AI 规则

- AI 行为变更（提示词、流水线、工具面）要求在同一个 PR 中运行评估，
  并提升流水线版本。
- 模型 API 密钥只存在于 sidecar 内存。后台分层从宿主运行，使用共享
  分层与用例注册表。

## CLI 规则

`astra-cli` 是代理的工具调用入口，按代理优先设计。每个动词都随附
`--json`（版本化 schema），变更类动词还随附 `--preview`。新动词随其
暴露的功能在同一个变更中落地，连同任何新的虚拟权限工具。

## 插件规则

插件在进程外依 `docs/plugins.md` 中的版本化钩子 API 运行。钩子绝不
触碰引擎；发送纪律不可绕过。钩子 API 变更遵循该文档中的版本化流程。

## 上游同步

参考项目按 `refs/refs-lock.toml` 锁定；流程与 22:00 任务契约见
`docs/upstream-sync.md`。禁止对参考决策沉默偏离：分歧在 PR 中上报。

## 构建与验证

- Windows 上以 Git Bash 为 shell；bash 5 或更新。
- 每次推送前运行 `cargo run -p xtask -- gate`。CI 镜像门禁。
- `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo test --workspace` 是底线。
- 工作中发现的坑逐行追加到本节。
- rust-toolchain.toml 会覆盖 CI 安装的工具链：锁定的频道必须同时声明所需
  组件，否则 fmt 和 clippy 在运行器上缺失。
- 门禁遍历的是工作树而非 git 索引；.superpowers 被跳过，因此被跟踪的
  文件一律不得存放在那里。
- rustc 的 dead_code 分析不看仅测试使用：运行时消费者晚到的辅助类型在私有模块里保持 pub、绝不用 pub(crate)，否则 deny(warnings) 会在中间构建失败。
- tauri-build 会把能力 schema 写入工作树的 client/desktop/gen 而非 OUT_DIR；门禁跳过该目录，且它保持不被跟踪。
- npm 11 会拦截 esbuild 的 postinstall 直至批准：全新安装后在 client/web 运行 `npm install-scripts approve esbuild`，否则开发服务器无法构建。
