# Edge 扩展安装与连接引导

本地交付：v0.6.13-edge-store.2；桌面和扩展内部版本仍为 0.6.13。

## 用户流程

- 首次使用说明中提供可跳过的 Edge 扩展入口；本地整理不依赖安装扩展。
- 偏好设置 → 浏览器采集 → Edge 显示安装与连接指南，保留安装、检查和修复入口。
- 只有用户点击“安装 Edge 扩展”才会打开官方商品页，不会启动时自动打开浏览器，不会自动安装扩展。
- 安装后在桌面连接或修复，再在浏览器工具栏的扩展内点击“检查连接”。桌面的“检查连接”只检查本机配置并读取通信记录，不能直接枚举浏览器已安装扩展。
- “稍后”收起本次引导；“不再提醒”将偏好保存在当前机器的当前资料库对应界面状态中。手动打开指南仍可安装。
- 已实际完成扩展通信后，自动收起引导并保存完成状态。以后临时断连可以检查和修复，不会因没有收到实时消息就声称“插件未安装”。
- Chrome 商店上传暂停，界面不提供虚构的 Chrome 商店链接；原开发版连接继续可用。

## 身份与连接状态

Edge 官方商品入口使用已确认 CRX ID：

https://microsoftedge.microsoft.com/addons/detail/oeeppgdhbgppkjgmjdaebffdhcfgeidi

该网页需在商店审核通过并公开后才能安装。页面暂未开放时可稍后再试；本机没有尝试自动提交或更改商店审核状态。

注册成功只代表本机主机配置已就绪，不代表浏览器已经安装扩展。成功通信的记录保存在安装本机的主机配置中，不跟随资料库备份到其他电脑。修复同一资料库连接会保留通信记录。未知扩展来源不会产生有效通信记录。

原开发版在 Chrome、Edge 使用同一 ID，无法仅凭该 ID 确定浏览器，因此仅记录通用开发版通信，不把它当作 Chrome 商店安装证明。Edge 商店 ID 的记录不标记 Chrome 为商店版已验证。

## 交付与测试

新增引导不改变扩展权限、不更改已提交的商店 ZIP、不增加浏览器渲染引擎或新的网络服务。中英文文案随应用语言切换，样式沿用主题颜色和字号。

QA 使用隔离资料库和本机连接。在明确设置测试环境变量时，商店打开动作仅记录固定官方 URL，防止自动化测试打开正常浏览器。正式运行时该按钮使用系统 URL 打开器。

本轮实际测试结果和所用 EXE SHA256 见本地交付目录的 BUILD-IDENTITY.md。真实商店签名包的安装与更新验收仍与同公钥开发者加载测试分别记录。

## English overview

The optional first-run hint and Browser capture preferences guide users through installing the Edge extension, enabling its local connection and checking communication from the extension toolbar. The browser opens only after an explicit install action. Later dismisses the current guide; Do not remind me again stores an installation-local preference, while manual access remains available.

Local registration is not installation detection. Only successful authenticated extension communication creates a receipt. Receipts stay with the local host configuration and survive repair for the same library. Store-identity and development-identity communication remain distinct; Edge store receipts do not verify a Chrome store installation. The Chrome store rollout is paused.

No extension permission change or new rendering engine is required. The official Edge listing becomes installable after store approval and publication. Store-signed installation acceptance remains separate.
