# v0.6.12 验证范围 / Validation scope

日期 / Date：2026-10-19。Windows 11 x64。所有自动化文件操作使用独立合成资料库，不修改用户原件或真实 AI 配置。

## 本轮 / This release

本版从当前修复整合代码重新构建，桌面与插件版本统一为 0.6.12。交付 EXE、安装器内嵌 EXE 与 UI 测试程序按 SHA256 对照，源码快照取自发布提交。具体哈希见发布附件 `SHA256SUMS.txt`。

The app and extension were rebuilt as 0.6.12. The tested EXE, installer payload and delivered EXE are checked for byte identity. The source ZIP comes from the release commit. See `SHA256SUMS.txt` for exact hashes.

| 检查 / Check | 范围 / Scope |
| --- | --- |
| 前端 / Frontend | 本地化文案和占位符、读取缩放 CSP、TypeScript、Vite 生产构建 / localization, zoom bridge CSP, TypeScript and production build |
| 核心 / Core | 216 通过、0 失败、3 忽略；索引、恢复、资料包、设置等；忽略项不计通过 / 216 passed, 0 failed, 3 ignored |
| 插件队列 / Extension worker | 离线、同 ID 重试、失败原因脱敏、容量、草稿、原图补救等；模拟 Chrome API / offline, idempotence, redaction, capacity, drafts and local-image recovery using mocked Chrome APIs |
| 品牌与实际 Chrome / Branding & real Chrome | 版本 0.6.12、默认群青、保存颜色保留、透明 Logo、英文深色、浮窗与真实 Chrome 加载／原 ID / version, default/saved accents, transparency, English dark mode, real extension loading and retained ID |
| 正文索引设置 / Index settings | 有效 Markdown 与损坏 DOCX 产生真实索引结果；16 px 间距、展开／收起、反向切换、Enter、减少动态效果，英文最大字号窄窗口 / real success/failure records, spacing, animation, keyboard, reduced motion and narrow English layout |
| 浮窗 / Palette | 5 种实际宽度下，中英文最大字号共 30 次收放；标题栏收起为 64 px，宽度与展开高度保持 / 30 collapse/expand cycles at 5 widths, Chinese/English, maximum font, width/height preservation |
| 打包 / Packaging | 程序／安装器版本、6 组图标帧、NSIS 实际载荷、ZIP 完整性、扩展 ID、源码提交、SHA256 / versions, icon frames, exact NSIS payload, ZIP integrity, extension ID, source commit and hashes |

UI 测试不调用真实模型服务或上传私人资料。安装包载荷检查不等于执行安装、卸载或覆盖升级。

UI checks neither call real model services nor upload private materials. Inspecting an installer payload is not an installer/upgrade/uninstall acceptance run.

## 沿用的历史证据 / Earlier evidence retained

O4 记录已核对真实 EXE SQLite 3.51.3，验证 v0.6.10／v0.6.11 合成库打开、重建索引、备份恢复和重启。WPS 12.1.0.29161 与 Microsoft Office 三组件合成文件 PDF 导出有既有实测记录。这些记录没有被重新包装成本轮覆盖所有真实文档的测试。

Earlier O4 evidence includes SQLite 3.51.3 runtime and old synthetic-library recovery. Earlier local Microsoft Office/WPS component tests cover synthetic Word/Excel/PowerPoint exports. They are not presented as new exhaustive document compatibility results.

## 发布范围 / Release boundaries

- 用户已报告第二台 Win11 验收暂未发现问题；具体机器构建、每项交接和长期运行记录仍有限。/ A second Win11 user report is positive, but detailed environment and endurance records remain limited.
- Windows 10、真实多屏／混合 DPI、断电／磁盘故障、全部 Office 版本和所有远程服务未穷尽验收。/ Windows 10, physical mixed-DPI setups, power/disk failure, all Office versions and remote providers are not exhaustively tested.
- Windows installer 未签名，插件以开发者模式交付。/ The Windows installer is unsigned; the extension is delivered unpacked.
- 用户明确指定本版作为正式版发布；正式发布标识不扩大上述测试范围。/ This version is published as stable at the user's request without broadening the validation claims above.
