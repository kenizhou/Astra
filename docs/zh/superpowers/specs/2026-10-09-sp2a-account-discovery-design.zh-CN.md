# SP2a 账户发现设计（中文审阅版）

> 非规范性译本，仅供审阅；凡与英文版有出入之处，以英文版为准
> （`docs/superpowers/specs/2026-10-09-sp2a-account-discovery-design.md`）。

日期：2026-10-09
状态：proposed
父 spec：`docs/superpowers/specs/2026-10-08-astra-ai-driven-workflow-design.md`（第 5、15 节）
兑现：SP1 spec 第 6 节修订中记录的 detect_account_settings 推迟项

## 1. 目的

补全「手动 + 自动发现」路径的账户设置故事：移植 mail-calendar 的自动
探测（JMAP 探测、autoconfig、ISPDB、SRV、MX 兜底），经宿主暴露为
`detect_account_settings`，让向导呈现推荐设置并支持手动覆盖。OAuth
provider（Graph、Google、JMAP 密码流的探测之外部分）与账户删除留给
后续切片。

## 2. 决策

- **D1**：把 `mailcal-autodetect` 移植为 `crates/astra-autodetect`
  （策略：JMAP well-known 探测含 `_jmap._tcp` SRV、Thunderbird
  autoconfig、ISPDB、IMAP/SMTP SRV、主机 DNS MX 兜底；按
  `docs/account-autodetect.md` 的非 HTTPS 不可信审批门；邮箱地址绝不
  出现在 URL 里）。
- **D2**：从 `mailcal-account` 移植推荐层（`recommend(email, detected,
  routes) -> SetupRecommendation`、`SetupChoices`），向导呈现与
  mail-calendar 相同的选择集（推荐 provider 加手动输入开关）。
- **D3**：宿主新增 `detect_account_settings(email)` 返回 IPC 类型；向导
  先调它，任何错误或五秒超时回退手动 IMAP。
- **D4**：不新增 Intent（探测属于设置路径，同
  `imap_test_connection`）；SP1 spec 第 9 节修订的获准设置路径例外清单
  恰好增加这一条命令。
- **D5**：活体覆盖：Stalwart harness 在标准端口暴露 JMAP；`harness.sh
  env` 增加 JMAP URL，加一个门控测试断言 JMAP 策略为 harness 用户探出
  JMAP 账户；现有 slice check 保持绿。

## 3. 测试

- 单元：策略解析器连同 MC 测试用例移植（autoconfig XML、ISPDB 条目、
  SRV 记录、MX 兜底排序）。
- 活体（门控，harness）：为 `astra1@example.test` 探出 JMAP。
- 前端：向导测试覆盖 探测 -> 推荐渲染 -> 手动开关；探测失败回退手动。

## 4. 验收标准

1. gate、workspace 测试、desktop 构建、web 构建与 vitest 全绿。
2. `detect_account_settings` 对未知域名返回选择集与手动 IMAP 默认一致
   的推荐（单测钉死），对 harness 用户返回 JMAP 推荐（活体、门控）。
3. 向导先探测后手动回退；既有手动流程测试无回归。
4. `docs/capabilities.md` 对应行移动（auto-detect 行到 partial；wizard
   行注明发现能力），zh 镜像与 changelog fragment 同变更。
5. SP1 spec 的已知推迟项就此兑现：本 spec 合并即关闭。
