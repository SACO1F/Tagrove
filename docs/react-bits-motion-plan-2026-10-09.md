# 拾签：React Bits 动效评估与实施建议

更新日期：2026-10-09。项目基线：桌面端 v0.3.0-alpha.16。

## 结论

适合拾签的方向是短距离过渡、清楚的状态反馈、轻微弹性和局部玻璃高光。每个效果需要对应具体操作：展开、选中、标注成功、正在识别或任务失败。下表是组件适配评估。五项核心微交互已按本项目 CSS 与 Web Animations 实施于 alpha.17，见 版本说明（历史资料，见 v0.6.12 源码快照）；没有引入 React Bits 源码或新增运行依赖。其余候选仍属建议。

查阅范围：用户提供的 [组件索引](https://reactbits.dev/get-started/index)、[官方组件目录源文件](https://github.com/DavidHDev/react-bits/blob/main/public/llms.txt) 及相关 TypeScript 实现。以下适配意见来自组件源码与本项目结构分析，不代表这些组件已经在拾签中完成 WebView2 性能验证。

## 候选组件

| 组件与示例                                                                                                                            | 拾签中的用途                                                 | 适配方式与优先级                                                                                                                                                                |
| ------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Animated Content](https://reactbits.dev/animations/animated-content) / [Fade Content](https://reactbits.dev/animations/fade-content) | 添加、编辑标签弹窗；标签编辑菜单；已有搜索、筛选和详情展开   | 高。参考淡入和短位移，统一已有过渡。建议 160～240ms、位移 6～12px。正文保持清晰，不使用模糊入场。官方实现依赖 GSAP 与 ScrollTrigger，简单弹窗可沿用 CSS/Web Animations。        |
| [Spring Check](https://reactbits.dev/micro/spring-check)                                                                              | 文件多选勾选框；AI 标签接受状态                              | 高。只保留轻微缩放、填色与勾线绘制；取消待办式划线和文字变暗。选中状态立即生效，动画不改变卡片或点击区域。官方实现依赖 Motion 和 Hugeicons，本项目已有 Lucide 图标可复用。      |
| [Glide Select](https://reactbits.dev/micro/glide-select)                                                                              | 排序菜单、标签九点编辑菜单；后续标签选择菜单                 | 高。借鉴菜单弹出与单块高亮连续滑动。保留键盘选择、Escape 关闭、边缘避让及焦点返回；悬停高亮和最终选择必须区分。                                                                 |
| [Animated List](https://reactbits.dev/components/animated-list)                                                                       | 浮窗最近三步记录；新建或接受标签后的局部出现；短 AI 建议列表 | 高。只对新增条目播放一次短入场，条目间最多错开 30～40ms。官方默认包含较大缩放和重复进入视口动画，需要减弱；不套在全部文件的虚拟列表上。                                         |
| [Lattice Loader](https://reactbits.dev/micro/lattice-loader)                                                                          | AI 识别状态；文件导入、标签导出状态                          | 高。用小型点阵提示处理中，完成转勾、失败转错误并提供重试。已有具体进度时显示真实数量；点阵不能冒充百分比。仅实际任务运行时循环。                                                |
| [Glass Surface](https://reactbits.dev/components/glass-surface) / [Specular Button](https://reactbits.dev/components/specular-button) | 工作台、浮窗的添加标签按钮及少量重要操作                     | 中。现有半透明玻璃已经实现，可补轻微悬停高光。Glass Surface 使用 SVG 滤镜并有支持检测；Specular Button 使用 OGL 着色器。先以 CSS 实现视觉方向，更强折射效果需要独立验证和降级。 |
| [Counter](https://reactbits.dev/components/counter)                                                                                   | 导入完成后标题旁文件数变化；导出结果数量                     | 中低。小幅数量变动播放 120～180ms 数字切换，大批导入合并更新，查询过程中避免反复滚动。屏幕阅读器应获得真实最终数值。官方实现依赖 Motion。                                       |
| [Masonry](https://reactbits.dev/components/masonry)                                                                                   | 列数变化、筛选或侧栏变化后的卡片重新排布                     | 中低。只借鉴位置过渡，不替换现有支持实测高度与虚拟滚动的布局。官方示例预加载传入图片并逐项动画，不能直接作为大资料库实现。先在可见卡片试验，拖标签时避免目标移动。              |
| [Spotlight Card](https://reactbits.dev/components/spotlight-card)                                                                     | 拖标签命中目标边缘；设置页小型功能卡片                       | 低、可选。仅保留微弱绿色边缘反馈，不用强聚光改变图片本身颜色或亮度。现有拖拽命中反馈可以作为基础。                                                                              |

## alpha.17 实施范围

1. 统一添加、编辑标签弹窗与九点菜单的过渡，保留立即可用的操作状态。
2. 文件多选增加轻微勾选反馈，兼容鼠标、Ctrl 多选与键盘。
3. 标签新建、接受 AI 标签后使用一次局部过渡，文件详情和浮窗同步逻辑保持现有语义。
4. 浮窗最近三步记录新增条目时轻微滑入，旧条目平滑离开；继续不增加撤销按钮。
5. AI 识别统一处理中、完成、失败视觉状态，并核对取消与重试。

后续再评估数字切换和可见卡片重排。现有搜索、筛选、侧栏、详情、布局按钮已经有过渡，应统一参数，避免重复套动画。

## 不建议放入日常工作台的效果

- Splash Cursor、Blob Cursor、Image Trail、Pixel Trail：持续跟随光标，容易遮挡文件并分散注意。
- Tilted Card、Circular Gallery、Dome Gallery：透视和旋转不利于快速浏览资料与准确拖拽。
- Pixel Transition、Glitch Text、Decrypted Text：遮盖或改变内容，识别文件名和原图时收益较低。
- Dock 的明显放大、Magnet 的按钮偏移：常用操作需要稳定位置；可在官网或演示页试用。
- 大面积动态背景、连续边框发光：主工作区域应保持安静。推广网页可以另行设计。

## 工程约束与验收

当前前端是 React + TypeScript + 普通 CSS，尚未安装 Motion、GSAP 或 OGL。若复制源码，优先选择 TS-CSS 版本，逐组件审查依赖、样式作用域、卸载清理和键盘行为；不为单一简单效果同时引入多个动画运行库。实际复制前应记录来源版本并核对仓库许可证。

- 交互立即响应；160～240ms 为建议主范围，面板展开沿用现有约 260～300ms 级别。
- 动画只作用于实际发生变化的局部，普通滚动不重复触发整页入场。
- 快速反向操作从当前位置过渡，取消时不留下旧动画、定时器或隐藏遮挡层。
- 标签拖拽期间点击区域与目标命中保持准确。
- 浅深主题、窄窗口、键盘操作、减少动态效果均需验收。
- 用 1,000／10,000 条合成记录验证滚动和内存，对比当前版本基线；性能阈值在取得基线后确定。
- 液态玻璃需在 WebView2 与目标显卡环境实测；失败时恢复现有 CSS 玻璃效果。

## 核对过的官方源码

- [Animated Content](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Animations/AnimatedContent/AnimatedContent.tsx)、[Fade Content](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Animations/FadeContent/FadeContent.tsx)
- [Spring Check](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Micro/SpringCheck/SpringCheck.tsx)、[Glide Select](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Micro/GlideSelect/GlideSelect.tsx)、[Lattice Loader](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Micro/LatticeLoader/LatticeLoader.tsx)
- [Animated List](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Components/AnimatedList/AnimatedList.tsx)、[Counter](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Components/Counter/Counter.tsx)
- [Glass Surface](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Components/GlassSurface/GlassSurface.tsx)、[Specular Button](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Components/SpecularButton/SpecularButton.tsx)
- [Masonry](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Components/Masonry/Masonry.tsx)、[Spotlight Card](https://github.com/DavidHDev/react-bits/blob/main/src/ts-default/Components/SpotlightCard/SpotlightCard.tsx)

完整开发路线见 [roadmap.md](roadmap.md)。
