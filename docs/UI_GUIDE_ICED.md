# Iced (TaskForest-I) 前端架构与开发指南

本文定义 `taskmanager-iced` 前端的纯函数式 Elm 架构（TEA）范式、状态流转与渲染纪律。
跨端中立契约见 [UI_COMPONENT_ARCHITECTURE.md](UI_COMPONENT_ARCHITECTURE.md)。

## 1. 核心思维：严格践行 The Elm Architecture (TEA)

- **单向数据流铁律**：`IcedApp` 是唯一顶层 Model；所有状态变更必须归约为强类型的
  `enum Message`；所有界面呈现必须是无副作用的纯函数 `view(&self) -> Element<Message>`；
  一切异步 I/O、剪贴板与平台交互必须通过 `iced::Task<Message>` 异步隔离。
- **存异思维：避免生搬硬套命令式回调**：Iced 没有可变组件树，禁止在 view 构建中
  尝试保存局部可变状态或命令式驱动重绘。状态一旦改变，通过返回新消息驱动 `update`
  完成模型迁移，由 Iced 运行时执行差异 Diff 与重绘。
- **性能与虚拟化**：大表渲染必须使用 `VirtualWindow` 与 `lazy` 缓存纪律，
  仅对可视区域行进行实际组件材料化，杜绝全表全量重绘造成的掉帧。

## 2. 屏幕空间与弹性布局纪律

- **紧凑视口防崩溃原则**：在 720×480 等小视口下，严禁简单粗暴地将顶栏导航、工具按钮、
  状态过滤器、视图预设和表格逐行垂直堆叠。
- **紧凑控件收敛规范**：
  - 工具栏在紧凑模式下强制约束为单行横向滚动或纯图标模式，严禁折为多行；
  - 搜索框与树视图展开/折叠控件在紧凑模式下合并至同一水平条；
  - 视图预设条（Presets Ribbon）从小视口多行折叠重构为单行（32px）横向滑轨；
  - 归还纵向垂直高度，确保紧凑模式下至少呈现 7–9 行有效进程数据。
- **绝对列序对齐**：进程表格表头与行单元格必须严格对齐 `PROCESS_COLUMNS`
  中立契约：`Name → User → PID → Threads → StartTime → Status → CPU (+ Trend) → Memory → Swap → MemoryPss → DiskRead → DiskWrite → Network → CPUTime → FDs → Nice`。

## 3. 性能页与设备切片呈现

- **设备详情路由**：根据 `app.performance.selected_device`，在主工作区精准呈现：
  - `CpuOrMemory`：CPU/内存历史走势与详细标量列表；
  - `Disk`：磁盘利用率、读写字节速率与分区统计；
  - `Network`：网卡发送/接收吞吐与接口地址；
  - `Gpu`：多引擎利用率与显存开销；
  - `Battery`：电池功率、健康度与状态。
- **交互手柄**：在侧栏与图表区间挂载带有 `Interaction::ResizingColumn`（↔）
  光标的鼠标拖拽手柄，满足视觉分割与动态宽度约束。

## 4. 浮层、右键菜单与多语言

- **浮层与上下文菜单**：行右键通过 `Popover` 浮层挂载，脱离表格视口裁切限制，
  包含 `EndTask`、`Kill`、`Priority`（高/正常/低）、`EfficiencyMode` 等完整动作；
- **纯净多语言**：严禁中英文混排（如 `Delete 确认` 统一为 `按 Delete 键确认`），
  浮层与提示文本统一通过 `taskmanager_application::i18n::t` 单源解析。

## 5. 锚定证据边界

iced 的 `view(&app)` 返回不透明 `Element` 树，headless 测试没有文本或像素回读
（仓库也未引入 `iced_test` 依赖）。因此 Swap 单元格锚点断言
`build_row_cells_with_rules` 产出的单元格文本（渲染输入）；要证明绘制帧文本需要
iced 侧新增 headless 文本读取能力，当前不存在。
