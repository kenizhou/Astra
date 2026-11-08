# 发布打包设计：把安装包放到发布上

**状态。** 2026-11-08 决定。分片自此提交切出主干。

**范围。** 一个由标签驱动的打包工作流，为 Windows、macOS 与 Linux 构建
**未签名**的安装包并附到 GitHub Release 上，外加该工作流暴露出的 tauri
配置修复。发布契约本身（VERSION、工作区版本、片段折叠）不变：本分片补上
契约从未有过的打包那一半。

## 配置修复

`client/desktop/tauri.conf.json` 的 `build` 对象带有**重复的**
`beforeDevCommand` 与 `beforeBuildCommand` 键：CLI 暂存一对，web 构建一对。
JSON 后键胜出的规则使暂存链成为死配置，干净检出上的 `cargo tauri build`
会在 tauri-build 的 externalBin 检查处失败。修复后只留一对规范键：
`beforeBuildCommand` 串起 web 构建、CLI release 构建、真实暂存（占位符只
用于引导）与 sidecar 的 `ci` 加 `build`；`beforeDevCommand` 串起占位符暂存、
sidecar 构建与 web 开发服务器。工作流不依赖这两者：它显式执行每一步，
与 `ci.yml` 已有的纪律一致。

## 工作流

`.github/workflows/release.yml`，在 `v*` 标签与 `workflow_dispatch` 上触发：

- 三操作系统矩阵（windows-latest、ubuntu-latest、macos-latest），与 nightly
  相同的运行器，Linux 侧用 nightly 自己的 Tauri 系统依赖清单，Rust 工具链
  按 `rust-toolchain.toml`（钉住频道所需的组件照列）。
- 显式链条：web 依赖（`npm ci`，已提交的 allowScripts 记录让 esbuild 的
  postinstall 保持获准）、web 构建、sidecar 的 `ci` 加 `build`、
  `cargo build --release --bin astra-cli`、真实暂存（不带占位符标志的
  `stage-cli.mjs`），再经 tauri CLI 打包。
- 按操作系统的包清单，取稳定集合：Windows 上 `nsis,msi`，macOS 上
  `dmg,app`，Linux 上 `deb,appimage`。打不出的包让任务失败（按操作系统
  快速失败；坏安装包不是警告）。
- 标签模式把矩阵构建的一切附到指名的 GitHub Release 上。手动触发模式
  （分支的验证路径）把安装包作为工作流工件上传，不触碰任何发布；`tag`
  输入让稍后的手动触发可以附到既有的发布上（v0.2.0 已存在且没有资产，
  就这样补上）。
- 未签名，且发布正文说明这一点：Windows 安装包触发 SmartScreen，macOS
  门禁要求右键打开，直到所有者提供证书。签名是具名余项，与 Azure 注册
  同类的外部事项；工作流不加任何签名配置，也不暗示任何签名。

## 已知缺口，记录而非隐藏

- 安装包未签名（无 Windows 代码签名证书、无 macOS 公证身份）；发布说明
  带有告示。
- AppImage 一腿跑在 Ubuntu 构建器上；将来可能抖动，按操作系统的任务会
  大声失败，绝不发坏工件。
- nightly 工作流不动（它构建并过门，不打包）；两条流水线的合并是以后的
  整理，不在本分片。

## 测试先行

配置修复以生效配置读取来钉住（一对规范键，暂存链活）。工作流的验证是
分支上的手动触发运行：三个操作系统全绿、工件在列（运行工件清单里有安装
包名）；合并后的第一次标签模式运行把 v0.2.0 的资产附到既有发布上。变更
日志片段随工作流落地（仅英文，`Platforms: desktop`，`Bump: none`）。
