> 非规范性译本，仅供审阅；以英文版为准。

# 变更日志契约

用户可见的变更在 unreleased/ 中得到一个名为 `<slug>.md` 的片段：

    以用户视角用一到三句话描述该变更。

    Platforms: desktop
    Bump: none

`Bump` 是建议的 semver 动作（none、minor、major）。发布 PR 把片段移入
released/ 下即将到来的版本，最旧在前，并且是 VERSION 移动的唯一场所。
片段仅用英文。
