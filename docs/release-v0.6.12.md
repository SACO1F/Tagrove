# Tagrove · 拾签 v0.6.12

**正式版 / Stable release · 2026-10-19 · Windows x64**

本版将 v0.6.11 后续修复整合为统一版本，桌面端与 Chrome 插件均为 **0.6.12**。这是正式发布编号；应用资料库标识、数据路径和插件 ID 保持兼容。

This stable release consolidates the fixes following 0.6.11. Desktop and Chrome extension both report **0.6.12**, while retaining the existing application identity, data location and extension ID.

## 主要内容 / Highlights

- **统一品牌与外观：**透明 Tagrove Logo 覆盖程序、安装器、浮窗与插件；默认群青，保留个人颜色；中英文与字号设置。
- **窗口与浮窗：**修复最窄尺寸无法收起、收起时变宽；支持全部普通标签、分类改名、排序和跨分类拖动。
- **索引与设置：**清除正文索引按钮留出安全间距；失败原因采用 AI 操作记录同款平滑展开，支持键盘和减少动态效果。
- **数据恢复基线：**整合 bundled SQLite 3.51.3 与旧库恢复、事务中断、索引取消及并发完整性回归。
- **Office 预览：**保留本机 Microsoft Office / WPS 组件检测、位数匹配和原排版导出修复；预览受可用自动化接口影响。
- **网页采集：**保留标签选择、手动标签跳过 AI、离线队列、失败诊断、图片 403 兼容重试和已下载原图补救。
- **文档：**重写双语 README，扩充桌面与插件用法、资料包交接、预览限制和升级步骤。

- **Branding:** transparent Tagrove icons across the app, installer, palette and extension; Ultramarine default with saved preferences preserved.
- **Palette:** minimum-width collapse and width-jump fixes, all ordinary tags, group renaming, sorting and cross-group dragging.
- **Index settings:** safer spacing and animated failure details matching AI history, including keyboard and reduced-motion support.
- **Recovery:** bundled SQLite 3.51.3 plus recovery, transaction-interruption, index-cancellation and concurrency regressions.
- **Office:** local Microsoft Office/WPS detection, architecture matching and original-layout conversion fixes.
- **Capture:** manual tag selection, optional AI, offline queue, diagnostics, safe HTTP 403 retry and downloaded-image recovery.
- **Documentation:** expanded bilingual product, extension, upgrade and handover instructions.

## 下载 / Downloads

- `Shiqian-v0.6.12-Windows-x64-setup.exe` — 安装包 / Installer
- `Shiqian-v0.6.12-Windows-x64.exe` — 直接运行程序 / Standalone EXE
- `Shiqian-v0.6.12-Chrome-extension.zip` — 开发者模式加载 / Unpacked Chrome extension
- `Shiqian-v0.6.12-source.zip` — 发布提交源码 / Release source snapshot
- `Shiqian-v0.6.12-manual.zh-CN.md` / `Shiqian-v0.6.12-manual.en.md` — 使用说明 / User guides
- `SHA256SUMS.txt` — 下载完整性校验 / Download integrity checks

## 安装与升级 / Install & upgrade

1. 导出标注备份，重要原件另行备份；退出旧实例。安装时保留用户资料。只发送 EXE 不会携带文件和标注，跨电脑用含原件资料包。
2. 桌面安装或运行新版后核对数量、标签与备注；本版仍使用原资料库身份，不要求重建全部标签。
3. 插件 ZIP 在固定目录解压；已有插件覆盖原目录后重新加载，不要卸载或清空队列。桌面／插件均更新到 0.6.12；EXE 移位后修复浏览器连接。
4. 需要 AI 或向量服务时单独配置并确认发送范围；不配置仍可本地整理和搜索。

1. Back up annotations and important originals, then exit the old instance. Keep user data during setup. Use a transfer package with originals for another computer.
2. Install/run the new desktop build and verify counts, tags and notes. Application identity and library location are retained.
3. Unpack the extension into a fixed directory; replace the existing folder and reload it. Do not clear the queue. Update both components to 0.6.12 and repair the browser connection after moving the EXE.
4. Configure optional AI/embedding services separately and review submission scope. Local organization works without them.

## 验证范围与已知限制 / Validation & limits

Windows 11 x64 为已有构建和桌面测试环境，用户亦反馈第二台 Win11 验收暂未发现问题；没有把笼统反馈写成所有安装、交接和长期场景逐项通过。Windows 10 尚未完成同等验收。具体本轮自动化结果见 `docs/validation-v0.6.12.md`。

Windows 11 x64 is the tested build/desktop environment; the user also reported no issues so far on a second Win11 computer. This does not claim exhaustive installer, transfer or endurance acceptance. Equivalent Windows 10 checks are incomplete. See `docs/validation-v0.6.12.md` for the exact automated scope.

安装器未做商业代码签名，可能出现 Windows 未知发布者提示。WebView2 必需。Office 原排版依赖本机可用引擎，复杂字体、SmartArt、打印区域或不同 WPS 版本可能影响结果；在线网页不共享 Chrome 登录。插件目前中文界面、开发者模式加载，未在商店发布。没有实时云同步。

The installer is unsigned and requires WebView2. Office output depends on a working local engine, fonts and document layout; online pages do not share Chrome login state. The extension has a Chinese interface and is loaded in developer mode, not a store listing. Real-time cloud sync is not implemented.
