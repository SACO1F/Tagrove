"""Build root-manifest store draft ZIPs and human-readable submission materials.

Does not publish, change native registrations, or alter the developer extension key.
Run after store-media.cjs; all output is under releases/v<version>/store-submission.
"""
from pathlib import Path
import hashlib
import html
import json
import shutil
import zipfile

ROOT = Path(__file__).resolve().parent.parent
VERSION = json.loads((ROOT / 'package.json').read_text(encoding='utf-8'))['version']
SOURCE = ROOT / 'browser-extension'
OUT = ROOT / 'releases' / f'v{VERSION}' / 'store-submission'
OUT.mkdir(parents=True, exist_ok=True)


def write(name, text):
    target = OUT / name
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text.strip() + '\n', encoding='utf-8')


runtime = ['background.js', 'popup.js', 'popup.html', 'queue.js', 'queue.html',
           'diagnostics.js', 'extension.css']
runtime += [f'icons/{size}x{size}.png' for size in [16, 32, 48, 128]]
manifest = json.loads((SOURCE / 'manifest.json').read_text(encoding='utf-8'))
assert manifest['version'] == VERSION and manifest['manifest_version'] == 3
assert set(manifest['permissions']) == {'activeTab', 'scripting', 'storage', 'contextMenus', 'nativeMessaging', 'alarms'}
assert not manifest.get('host_permissions')
assert 'local.shiqian.capture.qa' not in (SOURCE / 'background.js').read_text(encoding='utf-8')
manifest.pop('key', None)  # A development key does not establish a new store item's identity.
manifest['name'] = '拾签 Tagrove · 网页采集'
for browser in ['Chrome', 'Edge']:
    directory = OUT / browser
    directory.mkdir(exist_ok=True)
    package = directory / f'Tagrove-v{VERSION}-{browser}-STORE-DRAFT.zip'
    with zipfile.ZipFile(package, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        archive.writestr('manifest.json', json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')
        for name in runtime:
            archive.write(SOURCE / name, name)
    with zipfile.ZipFile(package) as archive:
        assert archive.testzip() is None
        assert set(archive.namelist()) == {'manifest.json', *runtime}
        assert 'key' not in json.loads(archive.read('manifest.json'))
    for name in ['128x128.png']:
        shutil.copy2(SOURCE / 'icons' / name, directory / f'logo-{name}')

write('00-先读我-上传顺序.md', f'''
# Tagrove 插件商店提交资料 · {VERSION}

准备日期：2026-10-19。状态：**可上传创建商品草稿，尚不可正式发布。**

你以前用“开发者模式 → 加载已解压的扩展”安装过；这不会创建商店商品，也不会给商品分配正式商店 ID。

## 你现在上传哪一个文件

| 后台 | 上传的 ZIP |
| --- | --- |
| Chrome Web Store | Chrome/Tagrove-v{VERSION}-Chrome-STORE-DRAFT.zip |
| Microsoft Edge Add-ons | Edge/Tagrove-v{VERSION}-Edge-STORE-DRAFT.zip |

这两个 ZIP 内的 manifest.json 均在根目录。ZIP 不含桌面 EXE、测试文件、截图、源码档案、密钥、开发版 ID 文件或额外父文件夹。两个浏览器复用同一份 MV3 实现，所以两个包的内容和校验值相同是正常的。商店图片、说明和隐私政策在 ZIP 外单独提交。不要上传最外层“全套资料 ZIP”。

## 正确顺序

1. 分别登录 Chrome Developer Dashboard 和 Microsoft Partner Center，完成开发者账户设置。身份信息、公开联系信息等由你本人填写。
2. 新建商品，上传对应 STORE-DRAFT.zip，保存草稿，**暂不提交审核或点击发布**。
3. 取得两个商品对应的 **32 位浏览器扩展 ID（a–p 字符）**。Chrome 可在 Package 页查看公钥；Edge 要取得浏览器扩展 ID，而不是仅提供 Partner Center 商品 GUID。如果后台暂不展示，提供商品后台链接或截图，继续确认，不要把测试版 ID 当正式 ID。
4. 将两个 ID（以及 Chrome 后台公钥，如可见）发给维护者。当前桌面 {VERSION} 仅接受测试 ID emppinjophbaookhjdigajhimbafcplg，必须同步修改本机原生主机 allowed_origins、请求来源校验及配置所有权判断，兼容两个商店 ID 后重新构建和测试。仅更改插件 ZIP、注册表或 allowed_origins 的其中一处都不够；不要放开为任意扩展来源。
5. 发布可公开下载的匹配桌面安装包，更新商店文案中的桌面下载链接。当前公开稳定版 v0.6.12 不应被宣称已兼容未来的商店 ID；本地 {VERSION} 测试版也尚未完成商店 ID 适配。
6. 复核 privacy-policy.md，按下面说明发布成无需登录即可打开的隐私政策网页，将真实公开 URL 填入两个后台。privacy-policy.html 是可直接托管的网页源文件，**本地路径不能作为隐私政策 URL**。
7. 根据“商店介绍”“权限与隐私填写”“审核测试说明”填写表单，上传 logo、宣传图和截图。
8. 用商店分配 ID 的测试构建验证新装、连接、网页/文字/图片、已有标签、离线重试、两个浏览器共存后，再提交审核。

## 资料位置

- Chrome/、Edge/：各自上传 ZIP、128 像素图标；Edge 另有 300 像素图标。
- assets/：1280×800 实机截图、440×280 小宣传图、1400×560 大宣传图。
- 01-商店介绍-中英.md：名称、简介、完整说明与搜索词。
- 02-权限与隐私填写.md：逐项用途、数据类别及审核声明建议。
- 03-审核测试说明-中英.md：供审核人员使用的测试步骤；补上匹配桌面版本链接后再提交。
- privacy-policy.md / .html：中英文隐私政策草案，项目 Issues 用于联系，未包含虚构的邮箱或公司信息。
- 04-验证结果.md：本次包检查和已知边界。
- SHA256SUMS.txt：所有交付文件的 SHA256 校验值。

## 隐私政策怎么发布

可以把 privacy-policy.md 上传到你拥有的 GitHub 仓库，通过公开、无需登录可访问的页面提供；也可以把 privacy-policy.html 放到自己的站点或 GitHub Pages。发布后用未登录浏览器确认可访问，再填入商店后台。本次只准备文件，没有替你发布网页、创建商品或提交审核。项目主页与支持地址可分别使用 https://github.com/SACO1F/Tagrove 和 https://github.com/SACO1F/Tagrove/issues；不要把尚未存在的政策地址填进去。

## 官方参考（2026-10-19 核对）

- [Chrome 打包要求](https://developer.chrome.com/docs/webstore/prepare)：根目录 manifest。
- [Chrome 商店图片](https://developer.chrome.com/docs/webstore/images)：128 像素图标、440×280 宣传图、1280×800 截图。
- [Chrome 商品 ID 与公钥](https://developer.chrome.com/docs/extensions/reference/manifest/key)。
- [Edge 发布流程](https://learn.microsoft.com/en-us/microsoft-edge/extensions/publish/publish-extension)。
- [Edge 原生连接的双商店 ID](https://learn.microsoft.com/en-us/microsoft-edge/extensions/developer-guide/native-messaging)。
''')

write('01-商店介绍-中英.md', '''
# 商店介绍（中文 / English）

## 建议名称

拾签 Tagrove · 网页采集

## 简短介绍

主动保存网页链接、选中文字与所选图片到本机拾签资料库。

Save web links, selected text and individual images to your local Tagrove library. Requires the Windows desktop app.

## 中文完整介绍（可粘贴至两个商店）

把浏览时发现的资料，带回自己的本地资料库。

拾签 Tagrove 网页采集扩展配合 Windows 桌面端使用，让网页链接、文字摘录和单张图片进入同一个标签整理流程。打开扩展或使用网页右键菜单，核对内容、选择已有标签，再保存到本机拾签。

主要功能：
• 网页链接：保存来源地址、标题和可读取的正文摘录，便于后续回到来源。
• 文字摘录：保存主动选中的文字，并保留网页来源。
• 网页图片：保存主动选择的一张图片；下载受防盗链、登录或服务器规则限制时，显示具体原因。可按提示改用本地下载的图片。
• 保存前选标签：从桌面端已有普通标签中选择，减少保存后的整理工作。文件夹自动标签和未确认 AI 建议不进入选择列表。
• 离线队列：桌面端未连接时保留待处理内容，恢复连接后可重试；查看保存状态和失败诊断。
• 本地整理：资料进入桌面端后，可继续添加标签、搜索、预览和导出。

使用前请安装并运行兼容的拾签 Windows 桌面端，在“偏好设置 → 浏览器采集”中选择正在使用的浏览器并启用连接，然后打开扩展检查连接。本扩展不能脱离桌面端独立完成保存，不提供云端同步。扩展当前界面为简体中文；桌面端支持中英文。

采集由你的操作触发，不会持续监视浏览历史或自动保存所有网页。浏览器内部页面等受保护页面不支持采集，网站的图片访问限制仍可能影响保存。

AI 为桌面端可选功能。保存前已选择标签的采集内容不再自动请求 AI 标签；未选择标签且你在桌面端启用 AI 自动标注时，相关分析内容可能发送至你配置的模型服务，建议经确认后再成为正式标签。基础网页采集无需 AI 服务。

项目与使用帮助：https://github.com/SACO1F/Tagrove
问题反馈：https://github.com/SACO1F/Tagrove/issues

## English full description

Bring useful things you find on the web into your local library.

The Tagrove web capture extension works with the Windows desktop app to organize web links, selected text and individual images using tags. Open the extension or use a page context menu, review the content, select existing tags and save it to Tagrove on your computer.

Features:
• Web links: retain the source URL, title and a readable text excerpt when available.
• Text excerpts: save text you actively select together with its source page.
• Individual images: save the image you choose. If a server blocks access or requires authentication, inspect a detailed failure reason and use the local-image fallback where appropriate.
• Tags before saving: choose existing ordinary tags from the desktop library. Automatic folder tags and unconfirmed AI suggestions are excluded.
• Local capture queue: retain pending captures while disconnected, inspect their status and retry after reconnecting.
• Continue organizing in the desktop app with tags, search, previews and exports.

Install and run a compatible Windows desktop app first. In Preferences → Browser capture, select your browser and enable its connection, then check the connection in the extension. The extension requires the desktop app and does not provide cloud synchronization. The extension interface is currently Simplified Chinese; the desktop app supports Chinese and English.

Capture is initiated by your actions. The extension does not continuously monitor your browsing history or automatically capture every page. Protected browser pages are unsupported, and website access restrictions may prevent some images from being downloaded.

AI features are optional and configured in the desktop app. Captures saved with manually selected tags skip automatic AI tagging. If no tags are selected and desktop AI auto-tagging is enabled, analysis content may be sent to the model service you configure. Suggestions require confirmation. Basic capture works without AI services.

Project and help: https://github.com/SACO1F/Tagrove
Support: https://github.com/SACO1F/Tagrove/issues

## 建议分类与搜索词

分类：Productivity / 效率（按后台实际选项选择）。
中文搜索词：拾签、Tagrove、网页采集、文字摘录、本地标签。
English keywords: Tagrove, web clipper, local library, tags, excerpts.

发布前请在正文补上已验证匹配商店 ID 的桌面版本下载链接。不要将“支持全部网站”“独立运行”“无限抓取”等不真实承诺加入文案。
''')

write('02-权限与隐私填写.md', '''
# 权限与隐私填写建议

以下依据当前扩展和桌面采集链路编写。商店表单选项可能调整，须按最终选项与实际处理行为逐项确认。不要勾选“完全不处理用户数据”。

## 单一用途 / Single purpose

将用户主动选择的网页链接、文字和单张图片，连同选定标签，保存到本机 Tagrove 资料库。

Save user-initiated web links, selected text and individual images, together with chosen tags, to the local Tagrove desktop library.

## 权限说明（中英可粘贴）

| 权限 | 用途 |
| --- | --- |
| activeTab | 用户主动打开采集界面时读取当前页标题和地址；仅作用于当次授权的活动标签页。 Read the active page title and URL when the user initiates capture. |
| scripting | 在当次授权的网页中读取用户选中文字及可阅读的正文摘录，供用户核对后保存。 Read selected text and a readable page excerpt in the user-authorized active page. |
| storage | 在本机浏览器配置中保存待处理采集队列、草稿和重试状态。 Store the capture queue, drafts and retry state in the local browser profile. |
| contextMenus | 在页面、选中文字和图片的右键菜单中提供主动保存入口。 Add explicit capture actions to page, selection and image context menus. |
| nativeMessaging | 与用户安装的本机 Tagrove 原生主机通信，检查连接、读取普通标签和将采集内容保存到本地资料库。 Communicate with the installed Tagrove native host for connection status, ordinary tags and local saves. |
| alarms | 调度本地采集队列的重试检查，不用于跟踪浏览行为。 Schedule local queue retry checks, not browsing tracking. |

无主机全站授权；右键菜单的 documentUrlPatterns 是菜单显示条件，不是 host_permissions。没有远程执行的代码。远程网页是保存的内容，不作为扩展执行代码加载。原生桌面程序由用户另行安装，不嵌入扩展 ZIP。

## 数据使用声明

- Website content / 网站内容：**涉及**。标题、URL、选中文本、正文摘录、所选图片来源与标签关联由采集操作处理。
- Web history / 浏览记录：不读取 history API，不连续记录访问轨迹；但捕获记录包含用户主动保存的 URL、标题及操作时间。若表单定义将这种页面 URL 归为 Web history，亦应据实勾选并说明“仅主动采集页面”。不要只因未申请 history 权限就宣称不处理任何 URL。
- Personally identifiable information / 身份信息、健康、财务等：扩展不主动索取这些字段，但用户选择的正文或图片可能包含。不要承诺自动识别或删除敏感内容；按后台定义说明用户提供内容的潜在范围。
- Authentication information：扩展不请求密码、浏览器 Cookie 或登录令牌。不要将密钥放入商店表单、截图或诊断报告。
- 用户行为、位置：不提供广告跟踪、行为画像、位置跟踪或自动分析浏览活动。
- 浏览器至桌面：通过 Native Messaging 和受验证的本机连接传输，不发送给开发者运营的采集服务器。
- 图片下载：桌面端向图片源服务器请求所选图片；必要时使用来源站点 Referer，源服务器可看到网络请求。不会绕过访问控制或提供浏览器 Cookie。
- 可选 AI / 向量服务：用户在桌面端配置并启用后，相关分析或索引内容可能发送到该服务提供方。应在政策与文案中披露；基础采集无需这些服务。
- 不出售用户数据，不用于广告、信用评估或与单一采集用途无关的处理；不向开发者自动上传诊断。仅在你能遵守这些承诺时确认商店合规复选框。

## 远程代码答复

No. All executable extension code is included in the submitted package. The extension does not load remotely hosted JavaScript or evaluate downloaded code. Native messaging talks to a separately installed local desktop application. Captured website content is user data, not executable extension code.

## 待本人在后台填写

开发者身份、联系邮箱、已公开的隐私政策 URL、面向的地区、最终数据使用选项与认证。该资料不包含虚构账号、邮箱、商店 ID 或签名文件。
''')

policy = '''
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
'''
write('privacy-policy.md', policy)
paragraphs = []
for block in policy.strip().split('\n\n'):
    if block.startswith('# '):
        paragraphs.append('<h1>' + html.escape(block[2:]) + '</h1>')
    elif block.startswith('## '):
        paragraphs.append('<h2>' + html.escape(block[3:]) + '</h2>')
    else:
        paragraphs.append('<p>' + html.escape(block) + '</p>')
write('privacy-policy.html', '<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Tagrove Privacy Policy</title><style>body{font:16px/1.8 system-ui,sans-serif;max-width:900px;margin:48px auto;padding:0 24px;color:#252c37}h1{font-size:28px}h2{font-size:21px;margin-top:40px}p{overflow-wrap:anywhere}</style><main>' + '\n'.join(paragraphs) + '</main></html>')

write('03-审核测试说明-中英.md', f'''
# 审核测试说明 / Reviewer testing notes

**维护者填写前置条件：取得商店 ID 并完成桌面适配后，填写已公开的匹配 Windows 安装包下载链接、版本号和 SHA256，再将本文发给审核人员。当前不应以 v0.6.12 或未适配的 {VERSION} 作为商店安装验收版本。**

本扩展需要本机 Windows 桌面端和原生消息主机；不需要项目账号，不需要登录示例网站，不需要 AI 密钥。扩展界面当前为简体中文。不要提供账号密码。

The extension requires the compatible Windows desktop app and native messaging host. No Tagrove account, website login or AI key is needed. The extension UI is currently Simplified Chinese. The publisher must supply a public download URL for the desktop build verified with this store item's extension ID before submitting these notes.

1. 安装并运行匹配的 Tagrove Windows 桌面端。打开“偏好设置 → 浏览器采集”，选择当前浏览器，连接/注册本机主机。
   Install and run the compatible desktop build. Open Preferences → Browser capture, select Chrome or Edge and connect/register its local host.
2. 桌面端创建普通标签“资料”，在浏览器访问 https://example.com ，点击扩展图标，点击“检查连接”。应显示桌面连接状态。
   Create an ordinary tag in the desktop app. Visit https://example.com , open the extension and check the connection.
3. 在扩展中确认标题和 URL，选择“资料”标签并保存。桌面库应出现来源网页记录并带手选标签，采集不自动请求 AI。
   Review the title and URL, select the tag and save. The source record should appear in the desktop library with that tag and without automatic AI analysis.
4. 选择网页文字，右键保存到拾签，核对摘录后提交。支持浏览器自动打开弹出界面；如浏览器阻止，点击工具栏带徽标的扩展图标继续。
   Select page text and use the capture context menu. Review and save the excerpt. If automatic popup opening is blocked, click the extension toolbar icon with its badge to continue.
5. 对公开可访问的一张图片使用右键采集，核对来源及图片地址后保存；服务器限制应以失败原因显示，不应绕过登录。
   Capture one publicly accessible image through its context menu. Server restrictions should produce diagnostics rather than bypass authentication.
6. 退出桌面端后尝试保存，查看采集队列中的失败/待处理记录；重新运行桌面端后重试，应只入库一次。重启浏览器检查队列保留。
   Stop the desktop app, attempt a capture and inspect the queue. Restart the app and retry; the capture should be saved once. Restart the browser to check queue persistence.
7. Chrome 与 Edge 分别安装，确认两者独立连接；移除一个连接不应影响另一个。不要以开发版 ID 的加载测试代替商店 ID 验收。
   Verify both browsers independently. Removing one browser connection must not break the other. Development-ID sideload tests are not a substitute for verifying store IDs.

项目地址 / Project: https://github.com/SACO1F/Tagrove
支持 / Support: https://github.com/SACO1F/Tagrove/issues
''')

write('04-验证结果.md', f'''
# 本次交付检查

- 两个包均为 Manifest V3，内部版本 {VERSION}，根目录包含 manifest.json；ZIP 完整性检查通过。
- 12 个运行时条目：manifest、7 个脚本/页面/样式文件、4 个尺寸图标。无多余父文件夹、README、测试主机、桌面 EXE、用户资料或密钥。
- 移除仅用于开发固定 ID 的 manifest key，源开发版 manifest 和 extension-id.txt 均未更改。两个商店后台将建立各自身份，不能保证相同 ID。
- 权限维持 6 项，无新增全站 host_permissions、远程脚本或分析 SDK。
- 运行时文件逐字节复用当前 {VERSION} 源码，未改变采集逻辑。本次真实 Edge → 隔离 QA 原生主机 → 桌面验证了连接、链接采集、手选标签和队列展示，并生成新的截图；采集 worker 的队列、草稿、诊断、图片路由、重试等单元回归通过。
- 此前 {VERSION} 实机回归曾覆盖摘录、公开图片、手选标签跳过 AI、离线重试、幂等及独立浏览器注册。本次额外全链复跑中，公开图片源站下载超时；再次尝试时桌面设置分组等待稳定超时，因此本次不宣称完整端到端回归全部通过。随后使用独立截图流程验证了连接、保存和队列。图片源站可访问性仍受网络环境影响。
- 商店包的身份与现有开发版不同；尚未取得两个商店 ID，因此**未进行商店 ID 到桌面的端到端验证**。已成功的开发 ID 回归不能作为正式商店安装验收。
- assets/screenshots 下是实机扩展页面截图，尺寸 1280×800，使用合成测试资料；共享的采集与队列界面截图来自 Edge，两个浏览器使用相同界面源码。截图没有伪造连接状态或私人文件。包含完整页，不截去标题。截图内“验收”字样是测试资料名称。
- assets/promotional 下为以现有透明 LOGO 排版生成的商店图，无新增产品功能示意；中文扩展界面没有被宣称为已完成英文翻译。
- 隐私政策文件已准备，尚未上线；须先复核并托管，然后填真实公开 URL。公开支持入口使用项目 Issues，开发者邮箱由本人在后台填写。
- 本次未提交审核、未发布商店、未变更生产注册表、未修改桌面程序或旧版交付包。

未完成项：两个商店 ID、匹配桌面构建及公开下载链接、公开隐私政策 URL、开发者后台身份/邮箱填写、商店 ID 实际安装验收、商店审核。先创建草稿，再完成这些项目。
''')

identity = ROOT / 'releases/v0.6.13-edge-store.1/BUILD-IDENTITY.md'
if VERSION == '0.6.13' and identity.exists():
    write('05-Edge商店ID适配进度.md', identity.read_text(encoding='utf-8'))
    note = ('> 2026-10-19 后续进度：Edge 商店 CRX ID 已确认，桌面适配构建已完成本地验证。'
            '本资料初版中关于“Edge ID 尚未取得／尚未适配”的说明，以《05-Edge商店ID适配进度.md》为准。'
            'Chrome 商店 ID、匹配桌面的公开下载链接及商店签名安装验收仍未完成，当前继续保存草稿。\n\n')
    for name in ['00-先读我-上传顺序.md', '03-审核测试说明-中英.md', '04-验证结果.md']:
        target = OUT / name
        first, rest = target.read_text(encoding='utf-8').split('\n', 1)
        target.write_text(first + '\n\n' + note + rest.lstrip('\n'), encoding='utf-8')


def seal():
    files = sorted(p for p in OUT.rglob('*') if p.is_file() and p.name != 'SHA256SUMS.txt')
    write('SHA256SUMS.txt', '\n'.join(hashlib.sha256(p.read_bytes()).hexdigest() + '  ' + p.relative_to(OUT).as_posix() for p in files))
    bundle = OUT.parent / f'Tagrove-v{VERSION}-Edge-Chrome-Store-Submission-Kit.zip'
    with zipfile.ZipFile(bundle, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for p in sorted(OUT.rglob('*')):
            if p.is_file():
                archive.write(p, p.relative_to(OUT).as_posix())
    with zipfile.ZipFile(bundle) as archive:
        assert archive.testzip() is None
    print(f'Prepared {OUT}')
    print(f'Full kit (do NOT upload this outer ZIP): {bundle}')


if __name__ == '__main__':
    seal()
