# SP4c-3b 设计：CRL 抓取

**状态。** 2026-11-13 定稿。切片自此提交从 main 分出。

**范围。** SP4c-3 的第二条腿：填充 CRL 缓存，使 S/MIME 接收验证能检查吊销。
缓存（upsert_crl / get_crl / list_crls / prune_stale_crls）与链的 CRL 消费
（AstraCrlChecker 对吊销硬失败、对 stale/unchecked 软失败、验证 CRL 签名）
已交付；缺的是分发点提取与抓取器。这关闭已记录的吊销缺口
（crypto-receive.md）。无引擎改动；不评测/不提升 pipeline（面向模型面
不变）。W9 密码流程（SP4c-3c）作为独立切片跟进。

## D1：分发点提取

一个位于 astra-crypto-smime/src/cert.rs、cert_summary_of_der 旁的助手
（SP4c-2 规则：CMS/证书 DER 解析留在 smime crate，绝不在 store）：从证书
提取 CRL 分发点 URL（x509-parser 的 crl_distribution_points →
GeneralName::URI）。从签名者叶与中间证书提取。仅 http(s) URL 可接受
（DP URL 受攻击者影响，它来自证书）。

## D2：抓取器端口 + 接收钩子

一个位于 astra-crypto-store 的 `CrlFetcher` 端口（SendCryptoPolicy 先例：
store 定义端口，宿主实现；store 绝不依赖 astra-app）。宿主实现
（client/desktop）做一个有界 HTTP GET（reqwest、短超时、响应大小上限、仅
http(s)）并 upsert DER。store 用假抓取器保持可离线测试。

receive_smime 验证路径的钩子：verify_with_context 之前，从叶 + 中间证书提取
DP URL；对每个，若缓存新鲜则用缓存，否则抓取（失败即软）并 upsert；然后
list_crls 供给链验证。抓取失败退化为 unchecked，绝不使邮件打开失败（链已
软失败）。抓取有界，故不会拖延阅读管线；链按签名时刻验证，故新抓的 CRL
仍对签名瞬间检查。

安全：DP URL 是不可信输入（仅 http(s)、超时、大小上限）；来自恶意 URL 的
伪造 CRL  failing 链的 CRL 签名检查（AstraCrlChecker 对颁发 CA 验证）。
隐私：抓取 CRL 向 CA 暴露正在验证哪些证书（跟踪向量）——记录为已知考量。

## D3：契约

docs/crypto-receive.md 新增 CRL 抓取注记（DP 提取、失败即软的抓取器端口、
如今可达的吊销状态、隐私考量）并镜像到 zh；已记录的"CRL 缓存空、吊销
unchecked"缺口更新。capabilities S/MIME 行的注记命名吊销检查落地。一个
变更日志碎片落地。

## 已知缺口，记录而非静默

- 抓取失败退化为 unchecked（失败即软），绝不阻塞打开。
- CRL 新鲜窗口与清理节奏是缓存既有策略；后台预取是后续（此处抓取有界且
  内联）。
- 隐私考量（CRL 抓取向 CA 暴露验证）已记录；ocsp/stapling 替代是后续。
- W9 密码流程（SP4c-3c）是独立切片。

## 测试先行

每个行为都带着它的红落地。提取测试（带 DP 扩展的证书产出 URL；无 DP 产出
空；非 http(s) URL 被拒）。抓取器测试（假抓取器：新鲜缓存不抓取直接用；
stale/缺失 CRL 触发抓取 + upsert；抓取失败退化为 unchecked，绝不失败）。
接收钩子测试（带已服务 CRL 的吊销证书 → chain_valid=false、
revocation_state=revoked；抓取失败 → unchecked）。一个门控的活 harness
实证：手工构造的 CRL fixture（testing.rs 的新构造器；CA 构造器已带
cRLSign）由本地 HTTP 测试服务器服务；播下 CA 链、吊销发送者叶序列号、服务
CRL、打开邮件；断言 revocation_state=revoked、chain_valid=false、吊销
banner；抓取失败否定 → unchecked；撤回副本。无活模型证明；离线电池即
证明。