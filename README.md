<div align="center">
  <img src="public/icon.png" alt="Tagrove · 拾签" width="112" />
  <h1>Tagrove · 拾签</h1>
  <p><strong>顺手贴上标签，随时找回资料。<br/>Tag it now. Find it when it matters.</strong></p>
  <p>整理本地文件、图片与网页，让参考资料重新派上用场。<br/>A local workspace for files, images and web references.</p>
  <p>
    <a href="https://github.com/SACO1F/Tagrove/releases/tag/v0.6.12"><img alt="Release 0.6.12" src="https://img.shields.io/badge/release-0.6.12-315c9c" /></a>
    <img alt="Windows 11 x64 verified" src="https://img.shields.io/badge/verified-Windows%2011%20x64-56667a" />
    <img alt="Tauri 2 and React" src="https://img.shields.io/badge/Tauri%202-React-315c9c" />
    <img alt="Local library" src="https://img.shields.io/badge/library-local-596579" />
  </p>
  <p>
    <a href="https://github.com/SACO1F/Tagrove/releases/tag/v0.6.12"><strong>下载 / Download v0.6.12</strong></a> ·
    <a href="docs/user-manual-v0.6.12.zh-CN.md">中文说明</a> ·
    <a href="docs/user-manual-v0.6.12.en.md">English guide</a> ·
    <a href="browser-extension/README.md">插件 / Extension</a>
  </p>
</div>

---

> **Edge 商店适配预发布版 / Edge store compatibility preview:** [v0.6.13-edge-store.1](https://github.com/SACO1F/Tagrove/releases/tag/v0.6.13-edge-store.1) 新增 Edge 独立连接，并兼容已确认的 Edge 商店 CRX ID；提供独立 EXE 和安装包，商店签名安装尚待验收。当前正式版仍为下方 **v0.6.12**。 / Adds independent Edge connections and the confirmed Edge store identity. Standalone EXE and installer are available; store-signed installation remains unverified. The stable release below remains v0.6.12.

[网页采集隐私政策 / Web capture privacy policy](docs/privacy-policy.md) · [商店审核测试说明 / Store review testing](docs/store-review-v0.6.13-edge-store.1.md)

**文件留在熟悉的位置，标签把它们重新联系起来。** 同一张效果图可以同时属于“项目参考”“建筑外墙”和“待讨论”；一份 PDF 可以与网页摘录、图片放在同一组搜索结果中。Tagrove 让整理与找回围绕用途展开，减少为了分类反复复制文件。

**Keep files where they belong; connect them with tags.** An image can belong to a project, a topic and a task at once. Find a PDF alongside related images and web excerpts, using the way you remember the material rather than a single folder hierarchy.

![Tagrove v0.6.12 群青主题工作台 / Ultramarine workspace](docs/images/tagrove-v0.6.12-workspace.png)

## 下载与安装 · Download & install

| 文件 / Asset | 用途 / Purpose |
| --- | --- |
| **[Windows 安装包 / Installer](https://github.com/SACO1F/Tagrove/releases/download/v0.6.12/Shiqian-v0.6.12-Windows-x64-setup.exe)** | 日常使用与同事安装。/ Recommended for installation and colleagues. |
| **[直接运行 EXE / Standalone executable](https://github.com/SACO1F/Tagrove/releases/download/v0.6.12/Shiqian-v0.6.12-Windows-x64.exe)** | 无需安装步骤；仍需 WebView2，沿用当前用户资料库。/ Run directly; requires WebView2 and uses the current user's library. |
| **[Chrome 插件 / Extension ZIP](https://github.com/SACO1F/Tagrove/releases/download/v0.6.12/Shiqian-v0.6.12-Chrome-extension.zip)** | 解压后通过开发者模式加载，详见下方步骤。/ Unpack and load in developer mode. |
| [源码 / Source ZIP](https://github.com/SACO1F/Tagrove/releases/download/v0.6.12/Shiqian-v0.6.12-source.zip) | 对应发布提交的源码。/ Source snapshot of the release commit. |
| [SHA256 校验 / Checksums](https://github.com/SACO1F/Tagrove/releases/download/v0.6.12/SHA256SUMS.txt) | 检查下载完整性。/ Verify download integrity. |

**已验证环境：Windows 11 x64。** Windows 10 尚未完成同等实机验证；不提供 macOS / Linux 安装包。本版未做商业代码签名，Windows 可能显示未知发布者提示。WebView2 是运行要求；安装流程可能需要联网准备该运行环境。

**Verified environment: Windows 11 x64.** Equivalent Windows 10 acceptance is not complete. No macOS/Linux packages are provided. This release is not commercially code-signed, so Windows may show an unknown-publisher notice. WebView2 is required; setup may need a network connection to prepare it.

升级前导出标注备份，重要原件另行备份；完全退出旧版，再安装或运行新版。安装器要求卸载旧版时保留用户信息，不要删除资料库。独立 EXE 的含义是可直接运行，**不是把资料存到 EXE 旁边的便携资料库**。不要同时运行两个版本写入同一资料库。

Before upgrading, export an annotation backup and back up important originals separately. Exit the old application completely. Keep user data if the installer asks to remove an old installation. The standalone EXE is **not a portable library stored next to the executable**. Avoid simultaneous versions writing to the same library.

## 产品能力 · What Tagrove can do

| 功能 / Feature | 细节 / Details |
| --- | --- |
| **资料工作台 / Workspace** | 大图瀑布流和列表布局；调整列数、列宽、侧栏与详情宽度；收藏、备注、来源信息。/ Masonry and list views, adjustable layout, favorites, notes and source information. |
| **多标签与分类 / Tags & groups** | 普通标签、文件夹标签分区；标签重命名同步关联文件，支持别名、颜色、合并和批量标注。/ Multiple tags per file; separate folder tags, synchronized renames, aliases, colors, merges and bulk tagging. |
| **桌面标签浮窗 / Floating palette** | 默认显示全部普通标签；分类展开、搜索、九宫格拖动排序／跨分类整理；拖动标签到文件或把文件拖到标签。/ All ordinary tags, group navigation, search, handle-based sorting and regrouping, and drag-to-tag workflows. |
| **统一检索 / Unified search** | 名称、标签、别名、备注、已索引正文，可选语义补充；结果展示命中依据。/ Search names, tags, aliases, notes and indexed text, with optional semantic results and match evidence. |
| **筛选集合 / Saved collections** | 组合类型、状态、包含／排除标签、目录和日期条件，保存为命名集合。/ Combine type, status, tag, directory and date filters into named reusable collections. |
| **详情阅读 / Inspector** | 内容、标注、信息切换；图片缩放、PDF 连续阅读／文字层、Markdown 阅读与编辑、HTML 静态／源码。/ Content, annotation and info tabs; image zoom, continuous PDFs with text selection, Markdown reading/editing and static HTML/source. |
| **Office 预览 / Office previews** | DOCX、XLSX、CSV 等内容阅读；可用本机 Microsoft Office / WPS 引擎生成原排版，覆盖受支持的新旧格式。/ Content readers and optional original-layout conversion through installed Microsoft Office/WPS engines. |
| **网页资料 / Web references** | 链接、摘录、图片采集；在线网页与阅读副本；来源可追溯。/ Capture links, excerpts and images; open online pages or collected reading copies and revisit sources. |
| **AI 建议审核 / AI review** | 可选自动标注；建议先审核，再成为普通标签；操作记录和识别任务独立查看。/ Optional automatic tagging with review before acceptance, separate history and task views. |
| **交接与恢复 / Handover & recovery** | 含原件资料包、标注备份、路径修复、诊断；导入资料按原父文件夹整理。/ Transfer packages with originals, annotation backups, relinking and diagnostics; preserve parent-folder grouping during import. |
| **外观与语言 / Appearance & language** | 中文／英文、浅深色、12 种主题与字号调整；默认群青，个人标签配色独立保留。/ Chinese/English, light/dark modes, 12 accents and font scaling; Ultramarine by default, with independent tag colors. |

### 1. 导入和标注 · Import & tag

1. 点击“加入文件”选择文件或文件夹，也可拖入工作台。原文件通常保留原路径，移动原件后需重新关联。
2. 选中文件，在详情“标注”页或悬浮添加按钮中添加标签；Ctrl＋点击可多选后批量标注。
3. 开启文件夹自动标签后，以**直接父文件夹名称**标注。它们与普通标签分开，不进入浮窗可添加标签或 AI 候选池。
4. 打开标签浮窗，把标签拖到文件卡片，或把文件拖入标签。九宫格用于标签管理、排序与跨分类整理；标签主体用于文件标注。
5. 顶部箭头将浮窗收起为标题栏，置顶按钮控制它是否保持在其他窗口上方；底部保留最近三步操作。

1. Choose files/folders with **Add files**, or drag them into the workspace. Originals normally stay at their existing paths; relink them if moved.
2. Select a file and add tags in **Annotation**, or use the floating add button. Ctrl-click for bulk tagging.
3. Folder auto-tags use the **immediate parent folder's name**. They stay separate from ordinary palette tags and AI tag candidates.
4. Drag a palette tag onto a file, or drop files onto a tag. Use the nine-dot handle for management, sorting and regrouping; use the tag body to annotate files.
5. Collapse the palette to its title bar, choose whether to keep it on top, and inspect its last three operations.

### 2. 搜索和筛选 · Search & filter

点击标题旁的搜索图标，或按 **Ctrl＋F**。名称、标签、别名和备注可本地检索；正文需要先在偏好设置启用索引。不支持、损坏、加密或超限的文件会显示相应状态，扫描件暂不自动 OCR。

Open search beside the page title, or press **Ctrl+F**. Names, tags, aliases and notes are searchable locally. Enable the content index for text search. Unsupported, damaged, encrypted or oversized files have explicit limits; scanned pages do not automatically receive OCR.

搜索结果的命中片段说明当前匹配位置，清空搜索后消失。日期使用 `YYYY-MM-DD` 或内置日历。选好筛选条件后保存集合，只需命名，之后可在筛选栏快速复用。

Match snippets explain the active query and disappear when it is cleared. Use `YYYY-MM-DD` or the calendar for dates. Save the current filter conditions as a named collection for reuse.

### 3. 阅读、编辑与原排版 · Read, edit & preview

点击文件自动展开右侧详情，再次点击同一文件或右上角关闭按钮可收拢。拖动详情左边界调整宽度，双击恢复默认宽度。顶部支持打开原件、放大预览；文件名旁可打开所在文件夹或复制路径。

Click a file to open the inspector; click it again or close the inspector to collapse it. Drag its left divider to resize, or double-click to reset. Open the original, enlarge the preview, reveal the folder or copy the path using its controls.

| 内容 / Content | 操作与边界 / Behavior & limits |
| --- | --- |
| 图片 / Images | Ctrl＋滚轮以鼠标为中心缩放，选择适合页面／宽度或比例。/ Zoom around the pointer; choose fit-page, fit-width or a percentage. |
| PDF | 连续阅读；带文字层的文档可选中文字，扫描件不等于已识别文字。/ Continuous reading and text selection where a text layer exists; scans are not automatically recognized. |
| Markdown | 阅读／编辑切换；保存或 Ctrl＋S 写回原文件，留意外部修改冲突。/ Read/edit modes; Save or Ctrl+S writes to the original, with external-change checks. |
| Office | 默认尝试原排版；可用引擎、字体、打印区域与文档格式影响结果，必要时切换内容预览。/ Original-layout mode depends on the installed engine, fonts and document setup; fall back to content preview where available. |
| 网页 / Web | 在线网页通过独立 WebView2 打开，不共享 Chrome 登录；阅读副本是采集内容，不是完整网站镜像。/ Online pages use a separate WebView2 session; reading copies are collected content, not full-site mirrors. |
| 本地 HTML / Local HTML | 静态预览和源码查看，原文件脚本不执行。/ Static rendering and source view without executing the original scripts. |

### 4. AI 标签与语义检索 · AI tags & semantic search

这两项为**独立的可选服务**，不配置仍能使用本地整理与搜索。AI 标注需要兼容的对话／视觉模型；语义检索需要支持 **Embeddings** 的向量模型，不能直接拿对话模型替代。

These are **separate optional services**. Local organization and search work without them. AI tags use a compatible chat/vision model; semantic retrieval requires an **embeddings** model and endpoint, not an interchangeable chat model.

开启前核对服务地址、模型、密钥和发送范围。分析内容会发送到用户配置的服务，可能计费。AI 建议确认后才进入普通标签池；带手动标签导入的采集内容不再自动提交 AI 标注。新导入文件的语义生成提示仍需确认，普通导入并不等于已上传生成向量。

Check the endpoint, model, key and submission scope before enabling either service. Content goes to the configured provider and may incur charges. AI suggestions enter the ordinary tag pool only after acceptance. Captures imported with manually selected tags skip automatic AI tagging. Semantic generation for new imports requires confirmation; importing alone does not mean vectors have been generated remotely.

## Chrome 插件 · Chrome extension

插件当前通过 **开发者模式手动加载**，界面为中文；以下英文步骤供英文用户参考。它不是单独的云端资料库，桌面程序需要运行。

The extension is currently **manually loaded in developer mode** and its interface is Chinese. English instructions follow below. It works with the running desktop app rather than a separate cloud library.

### 安装与连接 · Install & connect

1. 下载本版扩展 ZIP，解压到长期保留的目录。
2. Chrome 打开 `chrome://extensions`，开启“开发者模式”，点击“加载已解压的扩展程序”，选择含 `manifest.json` 的文件夹。
3. 固定扩展到工具栏；运行桌面版 0.6.12，在“偏好设置 → 浏览器采集”连接 Chrome 扩展。
4. 回到普通 HTTP／HTTPS 网页，点击插件图标，检查已连接桌面版本。
5. 升级扩展时覆盖**原目录**后点击“重新加载”，保留原 Chrome 配置、扩展 ID 和待处理队列。移动 EXE 后在桌面修复浏览器连接。

1. Download the extension ZIP and unpack it into a permanent directory.
2. Open `chrome://extensions`, enable **Developer mode**, choose **Load unpacked**, and select the folder containing `manifest.json`.
3. Pin the extension. Run desktop 0.6.12, then connect Chrome under **Preferences → Browser capture**.
4. Open an ordinary HTTP/HTTPS page and check the desktop connection from the toolbar popup.
5. To upgrade, replace files in the **same folder** and click **Reload**. Keep the Chrome profile, extension ID and pending queue. Repair the desktop connection after moving the EXE.

### 采集和选标签 · Capture & choose tags

- 工具栏保存当前网页；网页右键菜单可保存链接、选中文字或图片。右键操作优先在插件弹窗展示，不新开采集窗口；浏览器阻止自动弹窗时点击工具栏图标继续。
- 预览标题、来源和内容，搜索并勾选已有普通标签，再点击保存。文件夹标签和未确认的 AI 建议不出现在选择列表。
- 网页可选择在线阅读或阅读副本；摘录保留选中内容与来源；图片保存为本地采集资料并保留来源信息。
- 未选标签且已启用 AI 自动标注时，支持的新增采集会排队生成建议；选过手动标签则跳过自动识别。

- Use the toolbar for the current page, or the page context menu for a link, selected text or image. Context capture uses the extension popup; click the toolbar icon if Chrome blocks automatic opening.
- Review the draft, search and select ordinary tags, then save. Folder tags and unconfirmed AI suggestions are excluded.
- Read web items online or as collected copies. Excerpts and downloaded images retain their source information.
- Supported new captures without manual tags may generate suggestions when AI auto-tagging is enabled. Manually tagged captures skip that recognition step.

### 队列、失败与恢复 · Queue, failures & recovery

桌面离线或保存失败时任务保留在“采集队列”，**已暂存不等于已入库**。恢复连接后重试；同一请求 ID 用于避免重复保存。队列支持查看失败环节、错误码、HTTP 状态和脱敏诊断，也可重新编辑未完成任务。不要用清空扩展数据解决连接问题。

When the desktop is offline or saving fails, the queue retains the task. **Queued does not mean saved to the library.** Retry after reconnecting; request IDs prevent duplicate captures. Inspect the failure stage, code, HTTP status and redacted diagnostics, or edit the pending draft. Do not clear extension data as a connection workaround.

图片 403 可能与访问限制有关。新版会在安全条件下尝试来源站点 Referer；仍失败时，在浏览器正常下载图片，再从队列选择“已下载原图”补救。不会读取 Chrome Cookie 或保证能采集登录图片。支持 PNG/JPEG/WebP/GIF/BMP，当前上限为 50 MiB 和 1 亿像素；GIF 预览首帧。

Image HTTP 403 may reflect access restrictions. A safe source-origin Referer retry may help. If it still fails, download the image normally in your browser and select the downloaded original from the queue. Tagrove does not read Chrome cookies or guarantee capture of login-protected images. Supported formats are PNG/JPEG/WebP/GIF/BMP, up to 50 MiB and 100 million pixels; GIF previews use the first frame.

详细安装、能力范围和诊断见 [插件说明 / Detailed extension guide](browser-extension/README.md)。

## 给同事交接资料 · Share materials with colleagues

| 交付 / Item | 内容与用途 / Contents & use |
| --- | --- |
| 程序或安装包 / App or setup | 安装软件，不包含你的文件或标签。/ Installs the app; does not carry your library. |
| 标注备份 / Annotation backup | 恢复资料库信息；不包含普通原文件。/ Restores library metadata; excludes ordinary original files. |
| **含原件资料包 / Transfer package with originals** | **跨电脑交接文件、标签、备注等资料。/ Transfers files and annotations between computers.** |

A 机选中文件 → 导出资料包并核对范围 → 将资料包发给 B 机 → B 机安装后导入 → 检查文件夹、数量、标签与原件可读。来自 `Pic` 和 `Desktop` 的文件分别按原父文件夹落地；处理同名冲突，避免直接覆盖。保留交接包直到接收者核对完成。

On A, select the files and export a transfer package. Send it to B, import it after installation, and check folders, counts, annotations and readable originals. Files from `Pic` and `Desktop` are grouped by their original parent folder, with filename conflicts handled without silent overwrites. Keep the package until handover is confirmed.

## 常用快捷操作 · Useful shortcuts

| 快捷键 / Shortcut | 作用 / Action |
| --- | --- |
| Ctrl＋F | 展开搜索 / Open search |
| Ctrl＋A | 全选当前结果／取消全选 / Select or deselect current results |
| Ctrl＋点击 / Ctrl-click | 加入／移出多选 / Add to or remove from selection |
| Shift＋点击 / Shift-click | 连续选择 / Range selection |
| Space | 放大预览 / Open enlarged preview |
| Ctrl＋Z | 撤销支持的上一步操作 / Undo a supported operation |
| Ctrl＋滚轮（文件区）/ wheel over gallery | 调整瀑布流列数 / Adjust masonry columns |
| Ctrl＋滚轮（预览区）/ wheel over preview | 缩放内容 / Zoom content |
| Ctrl＋S（Markdown 编辑）/ Markdown editing | 保存原文件 / Save the original file |

## 开发、验证与边界 · Development, validation & limits

环境 / Prerequisites：Windows x64、Node.js 22+、Rust 1.90+、Microsoft C++ Build Tools、WebView2。

```powershell
npm ci
npm run build
npm run desktop

# Core regressions / 核心回归
npm run test:auto-tags

# Build installer / 构建安装包
npm run package
```

React、TypeScript、Vite 构建界面；Tauri 2、Rust、SQLite 提供桌面能力。PDF 资源在前置构建步骤从依赖准备，无需提交生成目录。

The UI uses React, TypeScript and Vite; Tauri 2, Rust and SQLite power the desktop. Prebuild steps prepare PDF resources from dependencies; generated resources are not versioned.

```text
src/                    工作台、浮窗、阅读、搜索 / Workspace, palette, readers, search
src-tauri/src/          数据、引擎与本机桥接 / Storage, engines, native bridge
browser-extension/      Chrome 采集与队列 / Chrome capture and queue
public/icon.png         透明 Logo 源文件 / Transparent logo source
src-tauri/icons/        原生和安装器图标 / Native and installer icons
docs/                   说明、验证与计划 / Guides, validation, plans
```

本版合入 SQLite 3.51.3 与恢复回归；自动化、隔离资料库和用户实机反馈各有明确范围，详见[验证说明](docs/validation-v0.6.12.md)。Office 自动化受本机安装、位数、接口、字体和复杂排版影响；外部网站与 AI 服务也受各自网络／权限限制。未实现实时云同步，不能将活跃 SQLite/WAL 直接放入网盘并发使用。

This release includes SQLite 3.51.3 and recovery regressions. Automated checks, isolated-library tests and user feedback have distinct scopes; see [validation](docs/validation-v0.6.12.md). Office automation depends on the local installation, interfaces, fonts and layout. Websites and AI providers have their own network/access limits. Real-time cloud synchronization is not implemented; do not concurrently sync a live SQLite/WAL library through a cloud drive.

更多内容 / More：

- [中文使用说明](docs/user-manual-v0.6.12.zh-CN.md) · [English guide](docs/user-manual-v0.6.12.en.md)
- [v0.6.12 发布说明 / Release notes](docs/release-v0.6.12.md)
- [完整版本变更 / Changelog](CHANGELOG.md)
- [后续计划 / Development plan](docs/post-acceptance-plan-2026-10-18.zh-CN.md)
- [第三方声明 / Third-party notices](THIRD_PARTY_NOTICES.md)
