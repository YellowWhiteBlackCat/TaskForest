# Bevy UI 第四前端章程

`taskmanager-bevy-ui` 是第四个前端：以 Bevy 0.19 的官方两件套 `bevy_ui` +
`bevy_ui_widgets` 渲染同一份中立 shell 投影。本文是它当前的公开事实权威；
跨前端组件契约归 [UI_COMPONENT_ARCHITECTURE.md](UI_COMPONENT_ARCHITECTURE.md)，
行为与像素门禁归 [QUALITY_GATES.md](QUALITY_GATES.md)。

## 定位

- 与 GPUI/Iced/TUI 同级的产品表面：只消费中立层投影，不读 OS 数据源，不拥有
  独立业务事实。成熟度低于 GPUI；页面功能覆盖、性能页深度、系统页和托盘仍
  是公开的已知边界，不在文档中虚构。
- 以 `TaskForestB` 登记于 [PRODUCT_IDENTITY.md](PRODUCT_IDENTITY.md)，是受支持的
  源码构建形态（独立二进制 `taskforest-b`），不进入发行包矩阵。
- 产品组件（进程表、图表、确认面）由自有 theme tokens + `ui-contract` 定义
  语义；Bevy 官方 Feathers 皮肤体系不采用——theme tokens 是唯一皮肤权威。

## 基座与边界铁律

- Bevy 锁定 `=0.19.1`，与 `taskmanager-platform-runtime` 的 `bevy_app`/`bevy_ecs`
  保持单一 workspace 解析；升级需架构与发布评审。
- Feature 闭包显式声明：`bevy_ui`、`bevy_ui_widgets`、`bevy_scene`（`bsn!` 宏）、
  `bevy_ui_render`、`bevy_core_pipeline`、`bevy_render`、`bevy_asset`、`bevy_winit`、
  `bevy_text`、`ui_picking`；Linux 追加 `wayland` 与 `accesskit_unix`，`x11` 永不
  开启。default features 关闭，`multi_threaded` 关闭以保持 drain 可观察。
- 依赖白名单：application、app-host、core、platform-contract、shell、theme、
  ui-contract、assets、icons（neutral 半，gpui feature 关）、`accesskit`（与
  bevy 栈同版本）—— never platform-runtime、platform crates。Bevy 类型不跨
  本 crate 公开 API。
- **100% `bsn!` 场景法**：生产 UI 树（页面、行/单元格、控件、覆盖层、
  loading/empty/error 变体）全部由 `bsn!` 场景组合并经 `spawn_scene` 挂载；
  禁止命令式 `Node`/`Children`/`with_children` 另起 UI 树。ECS 系统只更新
  场景实体的 typed 组件、接线事件/焦点，或以新场景替换有界子树。
  `scripts/quality/bevy_bsn_guard.py` 机械强制。
- 两个 World 永不合并：平台 client 经 app-host `OnceLock` 缓存每进程一次；
  窗口重建复用句柄，绝不重开 runtime。
- Linux 窗口仅 Wayland；X11 会话由现有三个前端承载。

## 数据接缝

`PreUpdate` drain 每帧以有界批量（`EVENT_DRAIN_BATCH`）非阻塞排水平台事件端口，
折叠进共享 `ShellApp`，并触发 `ShellProjectionFolded`——页面唯一的数据刷新
事件，永不轮询。刷新合并与暂停语义复用 shell 的 `TelemetryRefreshPolicy`；
效果提交只走共享 `queue_effect`。

## 输入接缝

`src/input.rs` 是原生输入进 shell 的唯一入口，自身不定义任何交互语义：

- 键盘按下归一为 `ShellKeyEvent` 后只转发 shell 的 `handle_local_char` /
  `handle_local_key`——与 TUI 相同的两个入口，模态优先级（确认门 > 帮助 >
  建议 > 搜索 > 自由）由 shell 单源裁定，前端本地和弦无法窃取已被门或编辑器
  拥有的按键。
- 前端自有路由权威（`Page`，九页）：Alt+1..8 与裸 `P` 切路由，同一动作同步
  应用到 shell 页面，使 `CommandScope` 派生跟随可见页面；Settings 无共享和弦。
- Dialog-scope Enter：共享命令表把 Enter 绑定为 `CommandScope::Dialog` 下的
  确认命令，shell 的 `dispatch_key` 派生不出该 scope，由本接缝补充路由。
- 效果桥：shell 状态迁移返回的 `PlatformEffect` 进入 `PendingEffects`，由
  drain（唯一持 client 锁处）提交；输入线程永不直接触碰平台端口。
- 重渲染信号：任何 shell 变更触发 `ShellInteractionApplied`，挂载页据此从
  投影重建——指针选行与滚轮（`pages/processes/input.rs`）也折算成同一组
  typed EntityEvent seam 后走相同的 shell reducer。
- shell 的退出决定每帧检查一次，仅转发一次 `AppExit`。

## 确认面

`src/confirmation.rs` 渲染 shell 的已武装门并只做两件事：显示冻结目标的
回声（`confirm.*` 共享文案），把两个选择路由回 shell 的 typed confirm/dismiss
路径。冻结目标集的稳定 key 是身份而非序列化细节——武装顺序不影响它，不同
目标集必得不同 key。键盘 y/n/Enter/Escape 与确认/取消按钮汇合同一 typed
路径；本前端当前无法武装的门类（服务/启动/会话控制等只读清单页）fail-closed
不渲染对话框，键盘 y/n 仍由 shell 门词汇权威处理。

## 无障碍接缝

`src/semantic.rs` 维护两份同源产物：ui-contract 的 `SemanticSnapshot`
（有界行、状态播报、已武装确认 modal，修订号键控增量重建）与场景行上的
`bevy_a11y::AccessibilityNode`（行身份标签）。Linux 的 `accesskit_unix`
feature 让 winit 的 AccessKit 桥把组件树发布到 AT-SPI；无窗口的 headless
组合中节点是惰性组件，不伪造平台回执。

## 视觉对等协议（2026-08-29 起强制）

捕获评审暴露的根因不是单点 bug，而是证据链只验证"存在与来源"，从不判定
"质量与对等"。以下四条从机制上封死这一盲区：

- **tofu 法（禁装饰字形）**：图标永远走语义注册表（`IconId` → `icons/*.svg`
  共享 SVG → `packaging/regenerate-ui-icons.sh` 派生的白底 RGBA 位图 →
  `ImageNode` 着色），字体字形只承载文本。源码里出现在代码位置的装饰区段
  码点（箭头/几何/杂项符号/PUA/emoji）是门禁失败——`visual_parity_tests`
  机械扫描，历史 tofu（`◔▦◈⚙`）即由此而来。
- **有界行纪律**：一切有界行（事实栏、表头、设备行、页签标签）必须
  `LineBreak::NoWrap` + 祖先 `Overflow::clip_x`——长值裁切，绝不换行成叠栈；
  排序方向是图标板不是拼接字符，列标签保持纯词。headless 契约测试钉死。
- **单一权威**：导航 strip 的页签集 = 共享 `AppPage::ALL`（同序），尾钮承载
  Alerts/Settings；图标身份由 `taskmanager-icons::path` 单源解析，SVG（GPUI）
  与位图（bevy）是同一 key 的两种材质化。卡片标题用 caption 级（页面只允许
  一个 Heading）。
- **逐张目检闭环**：每轮收尾必须把本次 bevy 捕获与 GPUI 参考捕获逐页并排
  目检，差距逐条记账——修掉的进提交，暂缓的进 roadmap"存异"清单。绿色门禁
  只证明行为与来源，不证明观感；观感由本条的人类（或代理）评审环节负责。

## 门禁

- `cargo check --locked -p taskmanager-bevy-ui --tests`；
  `cargo nextest run --locked -p taskmanager-bevy-ui --all-targets -j 4`；
  `cargo clippy ... -- -D warnings`。
- `python3 scripts/quality/bevy_bsn_guard.py --mode enforce`（quick gate）。
- `bash scripts/accept-bevy-interactions.sh`：交互矩阵先 discovery 后全量，
  见 [QUALITY_GATES.md](QUALITY_GATES.md) 前端证据表。
- `bash scripts/capture-bevy.sh`：真实像素，fail-closed 验证器。
- `cargo tree -p taskmanager-bevy-ui -d`：bevy 栈单一 0.19 解析。
