# Tagrove 网页采集隐私政策 / Web Capture Privacy Policy

更新日期 / Last updated: 2026-10-19

本政策适用于拾签 Tagrove 的 Chrome、Microsoft Edge 网页采集扩展及其与本机桌面端的采集连接。项目由 Tagrove 项目维护者维护，项目主页：https://github.com/SACO1F/Tagrove 。联系与反馈：https://github.com/SACO1F/Tagrove/issues 。请勿在公开反馈中填写私人正文、密钥或个人文件。

This policy covers the Tagrove Chrome and Microsoft Edge web capture extensions and their capture connection to the local desktop app. The project is maintained by the Tagrove project maintainers. Project: https://github.com/SACO1F/Tagrove . Contact: https://github.com/SACO1F/Tagrove/issues . Do not include private content, keys or personal files in public issues.

## 主动采集的内容 / Content you choose to capture

你使用工具栏或右键菜单采集时，扩展会处理当前网页标题、地址、选中文字、可读取的正文摘录、选中图片的地址、所选标签及采集状态。网页或图片可能包含你选择的个人或敏感信息，请先核对。扩展不持续监视所有访问页面，不使用浏览历史 API，不索取密码、Cookie、登录凭据或位置。

When you initiate capture through the toolbar or a context menu, the extension processes the page title, URL, selected text, a readable page excerpt, the selected image URL, chosen tags and capture status. Content you choose may contain personal or sensitive information; review it before saving. The extension does not continuously monitor all visited pages, use the browsing-history API, or request passwords, cookies, login credentials or location.

## 保存位置与传输 / Storage and transmission

队列和草稿保存在当前浏览器配置的本地存储中。队列最多保留 200 条记录，未提交草稿最多 20 条并按 24 小时有效期清理；完成记录可能为腾出队列空间而移除。采集通过 Native Messaging 发送给本机原生主机，并通过受验证的本机连接交给桌面端，写入用户的本地资料库。扩展不向开发者运营的云端采集服务器上传内容，也不提供跨浏览器队列同步。

Queues and drafts are stored locally in the current browser profile. The queue holds up to 200 records; unsubmitted drafts are limited to 20 with a 24-hour lifetime. Completed queue records may be removed to make room. Captures are sent through Native Messaging to a local native host and then through an authenticated local connection to the desktop library. The extension does not upload captures to a developer-operated cloud capture server or synchronize queues across browsers.

桌面端下载图片时会请求所选图片服务器；图片服务可能看到 IP 地址、所请求资源以及必要的来源 Referer。网站访问限制仍然适用，不转交浏览器 Cookie。用户打开来源网页时，也会直接访问该网站并受其政策约束。

The desktop app requests the chosen image from its source server. That server may receive your IP address, the requested resource and a source Referer when needed. Website restrictions still apply, and browser cookies are not forwarded. Opening a source website makes a direct request to that website and is subject to its policies.

## 可选 AI 与语义功能 / Optional AI and semantic features

基础采集无需 AI。你在桌面端配置并启用 AI 自动标注后，未手动选择标签的采集内容可能交给已配置的模型服务分析；已选标签的采集跳过自动 AI 标注。启用并生成语义索引时，相应分析或索引内容也可能发送给配置的向量服务。第三方服务按其政策处理请求，可能产生费用。你可以在桌面端禁用这些功能；禁用不会撤回已经发出的请求。

Basic capture does not require AI. If you configure and enable desktop AI auto-tagging, captures without manually selected tags may be analyzed by your configured model provider; captures with selected tags skip automatic AI tagging. When semantic indexing is enabled and generated, relevant analysis or indexing content may also be sent to the configured embedding provider. These third-party providers process requests under their policies and may charge fees. You can disable these features in the desktop app; disabling them cannot recall requests already sent.

## 用途、保留与删除 / Use, retention and deletion

采集内容仅用于保存、整理、搜索、预览、重试及提供你启用的相关功能。我们不出售这些内容，不用于广告、信用评估或无关用户画像。诊断不会自动发送给开发者；你可自行查看并分享已删减报告，分享前仍应检查。

Captured content is used for saving, organizing, searching, previewing, retries and related features you enable. We do not sell it or use it for advertising, credit assessment or unrelated profiling. Diagnostics are not automatically sent to developers. You may inspect and share a redacted report, but should review it before sharing.

你可以在采集队列中移除可删除记录，在桌面端移除资料或通过相应界面管理本地数据。移除资料库记录不一定删除原始文件、下载的文件、备份或你导出的资料包；这些副本须自行管理。卸载扩展通常会移除该浏览器中的扩展存储，不会删除桌面资料库；由浏览器及备份保留的副本以其实际行为为准。队列中的记录会保留到你移除、达到清理条件或卸载扩展；桌面资料保留到你自行管理和删除。

You can remove eligible queue records and manage library records in the desktop app. Removing a library record does not necessarily delete original files, downloaded files, backups or exported packages; manage those copies separately. Uninstalling the extension normally removes its storage from that browser, but does not delete the desktop library. Browser and backup copies depend on those systems. Queue records remain until removed, cleaned up under the limits above or uninstalled; desktop data remains under your control until you manage or delete it.

## 政策更新 / Updates

采集行为或数据用途变化时，我们会更新本政策和日期。你可通过项目页面查看更新或联系维护者。

We will update this policy and its date when capture behavior or data use changes. Use the project page to review updates or contact the maintainers.
