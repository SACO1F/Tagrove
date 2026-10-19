# Edge 商店公钥身份验收 / Store public-key identity verification

日期 / Date: 2026-10-19

用户从 Partner Center 复制的公钥经 RSA SPKI 解析和 SHA256 扩展身份算法计算，得到 CRX ID：`oeeppgdhbgppkjgmjdaebffdhcfgeidi`，与用户确认的商店 ID 完全一致。公钥是公开的身份材料，不是私钥；测试未修改源开发版公钥或已上传的商店 ZIP。

The public key supplied from Partner Center derives the exact confirmed CRX ID above. It is public identity material, not a private key. The development source key and submitted store ZIP were not modified.

## 实际验证 / Observed checks

在独立 Microsoft Edge 配置中，将公钥仅写入 QA 扩展副本的 manifest，浏览器实际报告的 runtime ID 与商店 CRX ID 一致。该扩展经唯一 QA Native Messaging 主机与公开交付 EXE 通信；未使用伪造的扩展 API 回执，也未仅通过命令行模拟来源。

An isolated Edge profile loaded a QA copy with the public key in its manifest. Its actual runtime ID matched the store CRX ID. The extension communicated through a unique QA native messaging host to the publicly delivered EXE, without mocked extension API responses or relying solely on command-line origin simulation.

- 实际 ID / Actual ID: `oeeppgdhbgppkjgmjdaebffdhcfgeidi`
- 桌面构建 / Desktop: `v0.6.13-edge-store.1`，内部版本 / internal version `0.6.13`
- EXE SHA256: `9b15fa3d44356f4e66706477286003e3a207037033a1336c42aac2177ff08534`
- 连接、网页保存、文字摘录、手选标签保留及跳过 AI / Connection, page capture, excerpts, retained manual tags and skipped automatic AI: **通过 / passed**
- 断连请求进入队列、恢复后重试、完成后重复重试不重复入库 / Offline retention, recovery retry and idempotence: **通过 / passed**
- 正常浏览器配置和生产注册表 / User browser profile and production registrations: **未改动 / untouched**

## 边界与提交 / Scope and submission

这验证了真实 Edge 中与商店相同身份的扩展到桌面的链路。扩展通过开发者模式加载，不是商店下载的签名 CRX，因此商店安装、更新、企业策略和干净系统验收仍须分别确认。本次未改动桌面运行代码或发布二进制，已有安装包、独立 EXE 和商店草稿 ZIP 均可继续使用；无须重新构建或重新上传相同 ZIP。

This verifies the real Edge-to-desktop path for the store identity. The extension was sideloaded, not installed as a store-signed CRX. Store installation, updates, managed-browser policies and clean-system acceptance remain separate. No runtime desktop code or published binary changed; the existing installer, EXE and draft store ZIP remain valid.

完成商店元数据、隐私政策、数据声明和审核说明后，可提交商店审核。在商店提供相应安装渠道后完成最终签名包验收，再决定正式公开发行。审核说明不应称已经完成商店签名安装测试。

After completing the listing, privacy declarations and reviewer instructions, the publisher can submit for store certification. Verify the signed package when a store installation channel is available before deciding final public distribution. Reviewer notes must not claim that signed-store installation was already tested.
