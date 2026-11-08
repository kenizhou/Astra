# 发布打包计划：把安装包放到发布上

规范：docs/superpowers/specs/2026-11-08-release-bundles-design.md。分支
`release-bundles` 自文档提交处切出主干。一个任务：能红则红，再实现，再跑
全套，一个约定式提交。

### 任务 1：配置修复与工作流

**文件：** client/desktop/tauri.conf.json（重复的 beforeDevCommand 与
beforeBuildCommand 键收敛为一对规范键，暂存链活）；
.github/workflows/release.yml（三操作系统矩阵、显式链条、按操作系统的
包清单、标签模式附带、手动触发工件、tag 输入）；变更日志片段。

- [ ] 编辑前以生效配置读取钉住修复（一对键，暂存链活）。
- [ ] 分支上的手动触发运行即验证：三个操作系统全绿、工件清单里有安装
      包名。
- [ ] 提交 `feat: bundle the installers onto the release`。

### 任务 2：收尾

- [ ] 全套检查；推送分支；PR（预期 squash）；合并后用 tag 输入的手动触发
      把 v0.2.0 的资产附到既有发布上。

## 自审笔记

- 规范覆盖：配置修复与工作流同为一任务；收尾补既有发布的资产。
- 风险：工作流在合并前无法完全验证（分支上的手动触发即验证，标签模式
  的附并在合并后于既有发布上验证）；AppImage 一腿在 Ubuntu 上可能抖动
  （坏安装包大声失败，绝不上船）；未签名是具名余项，发布正文带告示。
