# Bevy UI (TaskForest-B) 前端架构与开发指南

> **Role**: Implementation guide — widget patterns, rendering, interaction. For architecture decisions see [BEVY_UI_FRONTEND.md](BEVY_UI_FRONTEND.md).

本文定义 `taskmanager-bevy-ui` 前端基于 Bevy 0.20 的数据驱动 ECS 架构范式与交互纪律。
跨端中立契约见 [UI_COMPONENT_ARCHITECTURE.md](../../../docs/UI_COMPONENT_ARCHITECTURE.md)。

保存视图由应用页原生按钮进入，typed 命令只声明预设资源、shell、列选择、配置与剪贴板。
共享 shell 模型负责预设身份及转移；配置发布成功后恢复已保存行，失败由状态反馈说明。
应用页表格从实测内容槽扣除边缘安全带，再按完整行高确定虚拟窗口；紧凑时次要标题与计数组整体退让。
进程树通过应用页原生按钮进入统一 modal：标题及展开/收起动作固定，完整树行由单一 bounded body 滚动。
事件中心从告警页原生入口进入，以 shell 事件历史为唯一事实源；筛选仅改变呈现，导出保留完整历史，清空走 shell 命令。
检查面复用单一 modal 预算：标题与完整动作固定，剩余空间只分配给滚动 body，边缘保留 inset。

## 1. 核心思维：纯数据驱动的 ECS 哲学

- **显式访问范围**：业务 observer/system 使用具体的资源、查询、事件、`Commands` 或
  `SystemParam`，禁止接收整个 `World`，禁止用 `DeferredWorld`、组件 hook、排队闭包或
  服务定位器绕过范围声明。详见 [BEVY_UI_FRONTEND.md](BEVY_UI_FRONTEND.md) 的边界铁律。
- **告别命令式回调思维**：Bevy UI 不是基于 DOM 或对象树的命令式界面，而是纯正的
  ECS（实体-组件-系统）图。界面中的每一个元素都是一个 `Entity`，其属性与行为
  完全由挂载的 `Component`（如 `Node`、`Text`、`Button`、`NavTarget`）决定。
- **状态机的正确用法**：
  - 路由与宏观页面状态由 `Route` / `RouteChanged` 与 Bevy 调度器协同驱动；
  - 设备选择与局部焦点由专有资源（如 `PerformanceDeviceFocus`）单源管理；
  - 状态迁移只由系统通过 `commands.trigger(...)` 发出，严禁在场景闭包中尝试
    伪造或篡改全局状态。
- **100% `bsn!` 声明式场景树**：所有界面结构必须使用 `bsn!` 场景宏声明，
  并通过 `spawn_scene` 挂载，严禁使用命令式 `with_children` 另起游离树。
  写法判据归 [BEVY_UI_FRONTEND.md](BEVY_UI_FRONTEND.md)：运行时值字段直写
  （如 `TextLayout { linebreak: LineBreak::NoWrap }`），动态列表
  `Children [ { scenes } ]`，EntityEvent 观察者 `on(...)`；`template_value`
  硬零，创建链 `.insert(` 与直接 `spawn` 由
  `scripts/quality/bevy_bsn_guard.py` 拒收（页面级全局事件观察者除外）。
  异步就绪的表现补全（图标位图）用 `apply_scene(bsn! { ... })` 声明。

- 导航方向由原生按钮触发 typed 事件。布局系统只声明方向资源、窗口和 chrome 标记查询；
  根 workspace 与页内容分配独立槽，竖向 rail 自有滚动，方向切换不改变路由或遥测。

## 2. 交互与拾取机制（核心避坑守则）

- **拾取穿透铁律（`Pickable::IGNORE`）**：
  - 在 Bevy 0.20 中，`bevy_picking` 默认对所有 UI 节点生效；
  - 当一个实体携带 `Button` 组件且其内部拥有子节点（如 `Text` 标签或 `ImageNode`
    图标）时，指针点击会默认命中子实体。由于子实体没有 `Button` 组件，
    `button_on_pointer_click` 会忽略该点击，导致**“点击按钮毫无反应”**；
  - **规则**：所有挂载在 `Button` 内部的子节点（文本、图标容器）必须显式标记
    `Pickable::IGNORE`，确保点击事件穿透并正确定位到携带 `Button` 的父实体！
- **双重实体解析**：在 `On<Activate>` 观察者系统中，必须使用
  `targets.get(activate.entity).or_else(|_| targets.get(activate.event().entity))`
  进行双重检索，杜绝因事件携带字段差异而漏掉目标。

## 3. 页面路由与性能页设备切换

- **顶栏纯图标自适应（< 800px）**：在 `app.rs` 中为导航文本外层标记 `NavTabLabelNode`，
  由 `sync_nav_strip_layout` 监听窗口宽度。宽度 < 800px 时自动置为 `Display::None`，
  平滑切换为纯图标导航，彻底消灭文字裁切乱码。
- **性能页设备切片响应**：
  - 各设备分区（`Cpu`, `Memory`, `Disk`, `Network`, `Gpu`, `Battery`, `Fan`）挂载
    `DeviceViewCategory` 标记组件；
  - 侧栏设备按钮触发 `PerformanceDeviceFocusChanged` 后，系统遍历容器将匹配项置为
    `Display::Flex`，其余置为 `Display::None`，实现真正可切换的硬件看板。
- **详情面板抽屉自适应（< 960px）**：在紧凑视口下自动隐藏常驻进程详情面板，
  使表格区域弹性占满 100% 满宽，避免挤压表格列。

## 4. 文本对齐与防溢出纪律

- **消除左侧截断乱码**：在具有 `justify_content: FlexEnd` 的右对齐数值容器中，
  严禁施加过度狭窄的容器限制；移除左侧标签的绝对最小宽度（如 100px 硬编码），
  设置 `flex_shrink: 0.0`，为右侧数值预留充足弹性空间，杜绝数字左侧被裁切。
- **服务生命周期控制**：顶栏工具条完整挂载启动（`Start`）、停止（`Stop`）、
  重启（`Restart`）按钮，并由观察者直接投递到 shell 门控，摆脱纯只读状态。

## 5. 证据分层与测试避坑守则（警醒后人）

- **测试三层体系**：
  1. **纯无渲染 ECS 契约（`MinimalPlugins`）**：验证投影折叠、路由模型、键盘状态机、
     确认门控与无障碍树（AccessKit），不碰真实窗口或 GPU；
  2. **无头排版与几何约束**：验证弹性断点、视口裁切与滚动导轨几何（如 `scroll_and_motion_parity.rs`）。
     通过 `UiPlugin` + `TextPlugin` + 无头 Camera/Window 实体驱动，回读 `ComputedNode` 尺寸与
     `UiGlobalTransform`，并显式注入 `TimeUpdateStrategy::ManualDuration(16ms)` 消除墙钟竞态；
  3. **真实合成器像素捕获**：嵌套私有 Wayland（KWin + Niri）中运行真实 `taskforest-b` 二进制，
     验证真实 DPI、字体光栅化与窗口呈现（`capture-bevy.sh`）。
- **生命周期与调度提交边界**：
  - `commands().trigger(...)` 与 `Commands::spawn` 随命令在 ApplyDeferred 边界生效。断言前
    必须显式推进 `app.update()`（级联反应需推进两次），严禁在同一帧直接 query 新状态；
  - 严禁为了“求稳”滥用 `world.flush()` 打破正式调度；
  - `run_system_once` 仅限用于单次窄输入（如向 `MessageWriter<KeyboardInput>` 写入测试按键），
    严禁连续调用其测试带 `Local`、变化检测（Change Detection）或计时器的系统。
- **严禁预写终态（Anti-False-Pass）**：
  - 严禁直接向 World 写入“已授权/已确认/已销毁”伪造成功状态；
  - 破坏性操作必须从 `KeyboardInput` / `PointerPress` 进入，经共享 Gate 挂起与确认闭环；
  - 浮层外部点击（Outside Click）必须向背景节点分发真实 `PointerPress`，证明点击未被遮罩截胡。
- **场景挂载以 `Ready` 事件为准**：
  - 场景声明期实体尚不存在，必须观察 0.20 的自底向上 `Ready` 事件作为装配就绪点，禁止按组件 `Add` 逐个探测。
- **时间推进策略**：
  - 涉及动画、过渡曲线（Easing）、进度推进或告警超时的测试，必须使用 `TimeUpdateStrategy::ManualDuration`
    固定步进，严禁依赖系统墙钟 `std::time::Instant`。

## 6. Bevy 0.20 规范

- **rem 尺寸轴**：类型阶梯长度（控件高度、图标、字号、行高）一律
  `rem(...)`/`FontSize::Rem`，禁止为它们新增 `px(...)` 字面量或硬编码比例；
  px 只属于真实几何合同（边框、半径、固定窗口几何）。`RemSize` 由 palette 的
  UI-size 档位驱动，文本无关节点的 `EmSize` 由 `sync_node_em_size` 兜底。
  **机械强制**：`scripts/quality/bevy_rem_guard.py`（BEVY-REM-001
  字号零 `FontSize::Px`、BEVY-REM-002 px 派生值不得作布局长度、BEVY-REM-003 图标
  适配器不得收裸数字尺寸）与 `scripts/quality/bevy_scene_gate.py`（共享包
  `zhuge-ui-kit` 随 crate 发布的唯一执行器，UIK-REM-001 禁硬编码非零 px 字面量、
  UIK-REM-002 字号必须 rem）都已注册进 `scripts/quality/local-gates.sh` 与 CI；
  `scripts/quality/bevy_scene_ratchet.json` 棘轮台账维持零容忍**空账**，
  台账外新增非零 px 字面量立即失败。
- **rem 轴写法（新增控件照此写）**：
  - 控件/行高写 `rem(palette.control_height_rem)`（需要比例时
    `rem(palette.control_height_rem * 2.8)`，比例必须挂在 palette 档位上）；
  - 需要把固定间距并入 rem 长度时用 `palette.rem_ratio(space_8())` 把 px 间距
    折成 rem 比例（`Val` 没有加法，`rem(a) + px(b)` 不存在）；
  - 图标调用 `crate::icons::icon_scene(id, palette, ink)` /
    `inline_icon_scene(id, palette, ink)`，尺寸由适配器按 palette 档位持有，
    调用点不传尺寸；
  - `palette.control_height_px()` / `icon_size_px()` 只出现在纯算术里
    （行计数、虚拟窗口、折线段几何），绝不作为场景长度来源。
- **headless `TabList`**：页签面（导航 strip、弹窗页签）用 `TabList`+`Tab`，
  点选经 typed `ValueChange<Option<Entity>>`；选择权威（`Route`/呈现 state）
  永远只有一个，条带经 `SelectedTab` 单向镜像，镜像挂在 `Ready` 上（场景声明期
  实体不存在），禁止双写（不挂 `tablist_self_update`）。
- **`FixedNode` 浮层**：模态 scrim、tooltip、菜单浮窗根节点声明 `FixedNode`
  （视口锚定，不继承布局/裁剪/变换），不再写 `PositionType::Absolute` +
  `percent(100)` 惯用法。
- **`InlineIcon` 行内图文**：与文字同线的语义图标（表头排序板）用
  `crate::icons::inline_icon_scene`——`InlineBox`（几何权威）+ 标记组件在场景
  声明，位图句柄由观察者 typed 补全；图标随 ui_size 阶梯缩放
  （`palette.icon_size_px()`，流框是唯一的引擎 px 缝）。
