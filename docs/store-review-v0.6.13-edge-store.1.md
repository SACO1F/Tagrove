# Tagrove Edge 商店审核测试说明 / Reviewer testing notes

日期 / Date: 2026-10-19

## 配套桌面程序 / Companion desktop application

- 发布页 / Release: https://github.com/SACO1F/Tagrove/releases/tag/v0.6.13-edge-store.1
- Windows x64 安装包 / Installer: https://github.com/SACO1F/Tagrove/releases/download/v0.6.13-edge-store.1/Shiqian-v0.6.13-edge-store.1-Windows-x64-setup.exe
- 直接运行 / Standalone EXE: https://github.com/SACO1F/Tagrove/releases/download/v0.6.13-edge-store.1/Shiqian-v0.6.13-edge-store.1-Windows-x64.exe
- 独立 EXE SHA256: `9b15fa3d44356f4e66706477286003e3a207037033a1336c42aac2177ff08534`
- 隐私政策 / Privacy: https://github.com/SACO1F/Tagrove/blob/main/docs/privacy-policy.md
- 支持 / Support: https://github.com/SACO1F/Tagrove/issues

扩展版本 / Extension version: 0.6.13。桌面内部版本 / Desktop internal version: 0.6.13；交付构建 / Delivery build: edge-store.1。

此预发布版是 Edge 商店审核的配套程序，包含 CRX ID `oeeppgdhbgppkjgmjdaebffdhcfgeidi`，同时保留开发版 ID。Chrome 商店 ID 尚未提供。旧公开 v0.6.12 不包含此商店连接身份，请使用上面的专用构建。

This preview is the desktop companion for reviewing the Edge extension with CRX ID `oeeppgdhbgppkjgmjdaebffdhcfgeidi`. It also retains the development ID. The Chrome store ID is not yet configured. The older stable v0.6.12 does not include this Edge store identity; use the build linked above.

## 测试步骤 / Testing steps

1. 在 Windows x64 上安装并运行桌面端，或从固定位置运行独立 EXE。系统需有 Microsoft Edge WebView2 Runtime。无需 Tagrove 账号、示例网站登录或 AI 密钥。桌面包目前未做代码签名。
   Install and run the desktop app on Windows x64, or run the standalone EXE from a fixed location. Microsoft Edge WebView2 Runtime is required. No Tagrove account, sample website login or AI key is needed. The desktop binaries are currently unsigned.
2. 在桌面“偏好设置 → 浏览器采集”选择 Microsoft Edge，点击“连接”或“修复连接”。旧版配置需修复，以更新原生主机路径和扩展身份。
   Open Preferences → Browser capture, select Microsoft Edge and connect or repair the connection. Repair upgrades an older native host configuration and executable path.
3. 桌面创建普通标签“资料”。Edge 打开 https://example.com ，点击扩展图标和“检查连接”，确认本机连接可用。
   Create an ordinary tag in the desktop app. Visit https://example.com in Edge, open the extension and check its connection.
4. 核对标题与来源 URL，勾选标签后保存。桌面库应出现网页资料及手选标签。手选标签的采集不会自动请求 AI。
   Review the title and source URL, select the tag and save. Confirm the source record and chosen tag in the desktop library. Manually tagged captures skip automatic AI tagging.
5. 选中网页文字后通过右键采集，核对摘录再保存。如浏览器阻止自动打开浮层，点击带徽标的工具栏扩展继续。
   Select page text and capture it through the context menu. Review and save the excerpt. If the browser blocks automatic popup opening, click the extension toolbar icon with its badge to continue.
6. 对一张公开可访问图片使用右键采集。服务器登录、防盗链或网络限制可能导致失败；界面应显示详细诊断并提供合适的补救入口，而非绕过访问控制。
   Capture a publicly accessible image using its context menu. Authentication, hotlink restrictions or network failures may block a download; inspect diagnostics and available recovery options rather than bypassing access controls.
7. 退出桌面后尝试保存，查看浏览器采集队列；重新运行桌面后重试，应只保存一次。浏览器重启后队列仍应保留。
   Stop the desktop app and attempt a capture, then inspect the browser queue. Restart the app and retry; the capture should be saved once. Queue records should survive a browser restart.

## 验证边界 / Validation scope

核心测试 226 通过、0 失败、3 忽略。实际原生主机已验证商店来源请求、开发来源请求、旧配置迁移、未知来源拒绝、手选标签与幂等；真实 Edge 开发版扩展连接新桌面程序及保存通过。这些验证不等同于商店签名包的实际安装；后者仍待商店提供相应安装渠道后验收。本次安装包已校验包含同一测试 EXE，但未重跑干净系统或覆盖安装。

Core tests: 226 passed, 0 failed, 3 ignored. Real native-host tests cover the store and development origins, migration, rejection of unknown origins, manual tags and idempotence. A real Edge development extension connected and saved to this desktop build. This does not constitute installation testing of the store-signed package, which remains pending an available store installation channel. The installer is checked to embed the same tested EXE; clean-system and upgrade installation were not rerun.

补充验收：用户提供的商店公钥已推导为上述 CRX ID，且已在真实 Edge 以该公钥身份加载，完成连接、网页及摘录采集、手选标签、离线重试和幂等验证。它仍是开发者模式加载，不是商店签名安装。[完整记录](edge-store-public-key-verification.md)。

Additional acceptance: the store public key derives the stated CRX ID. A real Edge instance loaded a QA copy under that exact identity and passed connection, page/excerpt capture, manual tags, offline retry and idempotence. This was sideloading, not store-signed installation. [Full verification record](edge-store-public-key-verification.md).
