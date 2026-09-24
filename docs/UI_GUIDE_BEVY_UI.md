# Bevy UI (TaskForest-B) 前端架构与开发指南

> **Role**: Implementation guide — widget patterns, rendering, interaction. For architecture decisions see [BEVY_UI_FRONTEND.md](BEVY_UI_FRONTEND.md).

本文定义 `taskmanager-bevy-ui` 前端基于 Bevy 0.19 的数据驱动 ECS 架构范式与交互纪律。
跨端中立契约见 [UI_COMPONENT_ARCHITECTURE.md](UI_COMPONENT_ARCHITECTURE.md)。

## 1. 核心思维：纯数据驱动的 ECS 哲学

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

## 2. 交互与拾取机制（核心避坑守则）

- **拾取穿透铁律（`Pickable::IGNORE`）**：
  - 在 Bevy 0.19 中，`bevy_picking` 默认对所有 UI 节点生效；
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
  - 各设备分区（`Cpu`, `Memory`, `Disk`, `Network`, `Gpu`, `Battery`）挂载
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

## 5. 锚定证据边界

Bevy 可在 headless `App` 中 `spawn_scene` 后查询 `Text` 组件。Swap 单元格锚点因此
在投影断言之外，把每个投影行的生产 `row_scene` 挂载并回读 `Text`，证明单元格文本
进入绘制场景图；像素等价仍由 Wayland capture 矩阵承担。
