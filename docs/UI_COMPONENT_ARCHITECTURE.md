# 自有 UI 组件层总纲

TaskForest 自己拥有 theme、icons 和各 toolkit 的 renderer-local UI primitives。
`taskmanager-ui` 是 GPUI 的自有组件层；Iced、TUI 和 Bevy 各自维护符合自身生命
周期的组件层，四者只复用语义、资产和 token，不共享会泄漏 toolkit 状态的 widget。

## 边界

| 模块 | 职责 |
|---|---|
| `taskmanager-theme` | palette、mode、skin、font、UI size、density、motion、semantic colors |
| `taskmanager-icons` | `IconId` 到资产的稳定语义映射与可选 renderer adapter |
| `taskmanager-assets` | 嵌入 SVG、字体、产品资源和 provenance |
| `taskmanager-ui` | GPUI primitives、inputs、overlays 和样式组合 |
| `taskmanager-ui-contract` | 页面、命令、焦点、语义快照和 icon identity 合同 |
| `taskmanager-gpui` | RootView、页面投影、窗口状态和 GPUI 绘制 |
| `taskmanager-iced` | Iced 页面组件、焦点/激活壳、Canvas、虚拟表格和响应式布局 |
| `taskmanager-bevy-ui` | `bsn!` 场景组合的页面、表格/图表/控件与确认面（`src/widgets.rs`） |

组件层不拥有 provider、业务 reducer、窗口组合或系统路径。慢 I/O、历史查询和权限请求
必须在更下层完成，组件只呈现 typed projection。

## 组件族

- Primitives：label、selectable text、button、icon button、badge、pill、divider、spinner、progress、tooltip、scrollbar、toolbar、state panel、card surface；
- Inputs：switch、slider、checkbox、text input、search input、select；
- Overlays：layer stack、dialog、popup、context menu、dropdown、toast；
- Layout：page viewport/frame/scaffold、list-page scaffold、scroll region；
- Data：table、row selection、data row、key/value row、status cell、highlighter、empty/loading/error/recovery state。

每个组件必须有键盘路径、焦点可见态、语义名称/角色/状态、窄窗口行为和失败/禁用表达。
危险操作只提交 typed intent；确认框关闭、取消和 Escape 不产生副作用。
只读详情值通过 `SelectableText` 提供鼠标拖选、Ctrl/Cmd+A、Ctrl/Cmd+C，并在 Linux 鼠标
释放时同步主选择区；同一窗口只有一个活动文本选择，新选择必须清除旧高亮。密集表格必须
先解决与行选择、双击和列拖动的仲裁，不能直接套用。

## 尺寸与密度合同

- 桌面界面尺寸只有 `UiSize::{Small, Standard, Large}` 一条产品轴；Standard 是新安装和旧配置
  缺省值，正文基准为 16px，Small 以原 14px 几何兼容高密度需求，Large 正文为 18px。
- UI size 控制字体、图标、常规控件与 toolkit 产品缩放；系统 DPI/Wayland 分数缩放由 compositor
  独立提供并与之相乘。禁止把用户 UI size 写成 DPI，或覆盖系统 scale factor。
- `RowDensity::{Comfortable, Compact}` 只控制数据行的垂直 padding 与 leading；Compact 不得缩小
  字体。页面字号、密度和窗口断点是三个独立事实，配置字段也必须独立持久化。
- GPUI 的 `FONT_*` 是根字号相对令牌，窗口按 Small/Standard/Large 设置 14/16/18px rem；旧页面
  的显式令牌因此统一响应。新增文字必须使用语义或 `FONT_*` 令牌，禁止新增裸 10–13px 正文。
- Iced 通过 application `scale_factor` 施加产品缩放，仍保留 winit/compositor DPI；Small 是旧
  1.0 基线，Standard/Large 按 16/14、18/14 放大整个控件树。TUI 不控制终端模拟器字体，但读写
  配置时必须保留桌面 `ui_size` 字段。
- Standard 下中文说明文字不得低于 12px；常规正文、表格值和可点击标签以 16px 为基准。字号
  切换必须即时重排，不能只缩放 glyph 而保留会裁字的固定行高。

## GPUI 弹性布局合同

实现顺序、预算公式、整组准入和收口清单统一见 [ELASTIC_LAYOUT_PLAYBOOK.md](ELASTIC_LAYOUT_PLAYBOOK.md)；
本节定义组件边界，playbook 定义动手流程，二者不能被页面特化覆盖。

- root 先消费窗口、装饰、告警和导航方向，生成一次 `FrameBudget`；它扣除实际 shell chrome
  后生成 `ContentBudget`，页面只消费这个内容槽位，不在每个页面重复判断窗口宽高。横向
  `LayoutProfile` 固定区分 ultra-compact、compact、standard 和 wide，垂直空间是独立 typed axis，
  因此超宽矮窗仍可合并横向 chrome，同时折叠吃高度的次级内容。
- `PageLayoutBudget` 是从 `ContentBudget` 导出的兼容页面投影；Performance、Apps、Startup、System
  各自在页面边界恰好一次映射为 exhaustive page presentation，primitive 不再接受
  `compact`/`fill` 等响应式 bool，也不能重新读取 viewport 像素。
- Performance 的设备导航、主视图和统计栏由同一份页面槽位预算分配：不足以容纳三列时，设备栏
  自动变为横向 strip；统计栏下沉为 stacked 或在极限空间隐藏，主视图始终保留可读最小宽度。
  持久化侧栏宽度只是偏好，当前帧会在槽位上限内临时收缩，不会污染独立 App 的窗口状态。
- Performance 的滚动边界是硬合同：只有左侧设备选择栏（sidebar/strip）允许滚动；主视图和右侧
  统计栏必须使用固定视口与弹性分配，不能挂载 `scroll_region`、`uniform_list` 或隐式滚动句柄。
  下方内容必须在进入视口前整组适配、摘要或省略，最后一行不得靠裁切或滚动来“解决”。
- 所有右侧 detail/stat rail 共享 label/value 槽位：复合事实必须拆成独立行，超长值只能在自己的
  bounded value 槽内截断或换行，不能把标签挤出行首、把值推过边框，或用一条超长字符串填满窄栏。
- 自适应网格必须以内容最小可读宽度为约束，允许换行但禁止压缩文字、图表和操作控件到不可见；`flex_1` 的兄弟必须显式
  `min_w(0)` / `min_h(0)`，滚动内容必须保留真实 intrinsic extent。
- 页面自身拥有 pinned trailing rail 时，`PageFrame` 的外层 trailing inset 必须归零；可点击的
  rail 宽度由 tracked viewport 内部预留，不能在 rail 外再留一条无归属空带；主内容列仍必须在
  divider 前拥有 budget 分配的内部 trailing inset，不能让图表或文字贴住 rail。
- 稀疏页面优先让主图或主详情填充剩余空间；密集表格优先保持行高和关键列可读，次要列进入隐藏、横向滚动或详情面板。
- 每个新布局至少覆盖窄窗口、标准窗口、宽窗口、垂直导航和长文本；不能用单张宽屏截图证明弹性成立。

## GPUI 动效与帧预算合同

- 动效只能表达状态、层级和因果：页面/面板出现、导航选择、恢复提示、图表数据变化和焦点反馈；禁止给虚拟化列表逐行
  添加持续动画。
- 统一使用 theme motion policy 和稳定 `ElementId`；默认过渡保持在 80/120/180ms 等级，必须有 reduced/no-motion 路径。
- 高频 telemetry 只更新动态图层；网格、渐变、静态文字和曲线几何应按数据 revision、尺寸、主题和选项缓存。hover 只刷新
 交互层，不得触发整页数据 fold 或 O(N) 重建。
- 阴影、模糊和重复 canvas pass 只用于有限层级；密集表格、侧栏和可滚动行使用边框/色阶表达层次。
- 动效验收除最终帧外，还要验证 0%、中间帧和稳定帧的布局边界；性能验收必须证明 UI-only frame 不复制历史数据。

## 页面架构与特化边界

- GPUI 顶层页面统一经过 `page_viewport` / `PageScaffold` / `PageFrame`；长内容使用
  `scroll_region`、`auto_scroll_region_fill`、`bounded_scroll_region` 或带 rail 的同族 helper，
  页面不得重新拼 flex/min-size 约束。稀疏详情页使用 fill 变体：内容不足时主图占满剩余
  视口，内容超出时保留真实滚动范围。
- Iced 的根入口是 `ui.rs::view`，统一经过 `components::page_scaffold`；Performance 的页面壳、
  设备选择、详情映射和有界 rail 位于 `ui/performance.rs`，页面模块只保留自身的数据投影和交互编排。
- Services、Startup、Users 共用 Iced 的表格/状态组件；GPUI 的 Process Insights、Dashboard、
  Process Details 与 Iced 的对应页面保持 toolkit-local，不共享 widget 状态。
- 页面特化组件可以拥有自己的字段、图表或危险操作，但必须组合已有 surface、state、row、
  key/value、toolbar 和 scroll primitives；重复第二次的几何/状态语义必须下沉到对应
  toolkit 的组件层：GPUI 进入 `taskmanager-ui`，Iced 进入
  `taskmanager-iced/src/ui/components.rs`，Bevy 进入 `taskmanager-bevy-ui/src/widgets.rs`
  （100% `bsn!` 场景适配器，见 [BEVY_UI_FRONTEND.md](BEVY_UI_FRONTEND.md)）。
- 特化不是豁免：新增 wrapper 先说明数据或交互边界，并补窄窗口、焦点和失败态回归；不得用
  import alias、改名 re-export 或跨 crate forwarding facade 绕过组件边界；跨层类型必须从
  其 owner crate 的显式模块路径导入。

Iced 的 renderer-local 组件入口是 `taskmanager-iced/src/ui/components.rs`；`focus.rs` 负责
可访问激活壳，`theme.rs` 负责 token 到 Iced style 的映射，`virtual_list.rs` 负责有界表格窗口。
它们与 `taskmanager-ui` 平行而非上下游关系。

## 视觉不变量

- 颜色只能来自 `taskmanager_theme` token；禁止在组件中新增产品色 literal。
- 间距、字体、行高、圆角和动画使用 token；`px(...)` 只用于明确的几何合同。
- 可选择的应用汇总行使用 `ProcessRowId::Application(root_identity)`；root identity 仅是实时树查找键，
  不是代表 PID。分类标题是结构行，真实进程行才使用 `ProcessRowId::Process(identity)`。
- 渲染入口先完成数据 fold；同一 projection 同时服务鼠标、键盘和辅助技术。
- 组件不复制 core 文本匹配、时间、格式化或可用性规则；共享规则只有一个实现。
- 组件层保持零第三方 GPUI 组件依赖；上游代码只作研究材料，不复制 GPL UI 源码。

## 交互状态机合同

跨 crate 的 owner 台账、确认/request/ECS/quit/feedback 迁移与批处理顺序只见
[`STATE_OWNERSHIP.md`](STATE_OWNERSHIP.md)，本文件不复制状态矩阵。

组件层只补充 renderer 约束：每个前端至多一个阻断式 primary surface；输入先解析唯一
`InputScope`，键盘、指针、焦点、辅助语义和 dismiss 消费同一 projection。GPUI
`GpuiSurfaceKind::{Shared, Local}`、Iced/TUI 对应 scope 只组合 shared application surface 与真正的
renderer-local payload；Cancel/Escape/Close/Scrim/PageChanged/TargetUnavailable/Completed 不产生
平台 effect。Render 只消费 immutable frame，mount/paint/focus 协调可以留在 toolkit，session、
surface 迁移、后台 completion 和 effect 必须在 event/update system 完成。

GPUI 与 Iced 的持久显示偏好均由一次 config snapshot 原子投影；长寿命设置表单使用
Clean/Dirty/Conflict。projection cache 只返回 owned snapshot，不向 view 暴露 `RefCell` guard。

## 验收

组件改动需要行为回归、焦点/语义回归和适用的当前构建视觉证据；边界门禁必须继续证明
没有回流到外部组件层。质量入口见 [`QUALITY_GATES.md`](QUALITY_GATES.md)，视觉入口见
[`screenshots/README.md`](screenshots/README.md)，实现细节见 `crates/taskmanager-ui/README.md`
与 `crates/taskmanager-theme/README.md`。
