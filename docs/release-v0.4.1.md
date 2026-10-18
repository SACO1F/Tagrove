# 拾签 v0.4.1：侧栏布局、PDF 上限与 HTML 预览

2026-10-11。本轮本地交付，公开推荐下载仍为 RC.4。

- 修复侧栏展开/收起和拖动分隔条时重复启动卡片 FLIP 的抖动。侧栏连续变化期间实时布局，停止后列数加减仍有动效。
- PDF 放大阅读上限由 30 MiB 提高到 64 MiB。卡片缩略图仍限 30 MiB，避免浏览时同时读取多个大 PDF；超出缩略图限制的文件可通过详情放大阅读。单页画布约 8 MP 的预算保留。
- HTML/HTM 文件新增放大静态预览和源码切换。兼容现有“其他”类型记录，无需重新导入或改变数据库格式。文件上限 2 MiB，支持 UTF-8、UTF-16 及现有中文文本解码。

HTML 使用无脚本权限、无同源权限的 sandbox iframe，文档内 CSP 禁止网络、子框架、对象、表单提交和脚本；只允许内嵌样式及 data 图片/字体。删除源文档 meta/base 后再放入预览。源码显示使用 React 文本节点，不作为工作台 HTML 执行。依赖 JavaScript 或相邻 CSS/图片文件的网页不会完整呈现，可使用默认浏览器打开。

设计依据：[MDN srcdoc](https://developer.mozilla.org/en-US/docs/Web/API/HTMLIFrameElement/srcdoc)、[iframe sandbox](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe)。

读取 PDF 仍是有界整文件方式。64 MiB 为本轮限制，不表示任意大型文件都低内存或瞬间打开；进一步提高上限前应评估分块/按范围读取。

升级前导出标注备份，退出旧工作台和浮窗；安装时保留用户数据。交付目录 releases/v0.4.1/。未替换已发布或此前本地交付的安装包。

验证记录（历史资料，见 v0.6.12 源码快照） · 使用说明（历史资料，见 v0.6.12 源码快照） · 升级说明（历史资料，见 v0.6.12 源码快照） · 已知限制（历史资料，见 v0.6.12 源码快照）
