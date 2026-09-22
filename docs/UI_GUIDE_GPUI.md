# GPUI (TaskForest-G) 前端架构与开发指南

本文定义 `taskmanager-gpui` 前端的专属工程范式、核心思维与渲染纪律。
跨端中立契约见 [UI_COMPONENT_ARCHITECTURE.md](UI_COMPONENT_ARCHITECTURE.md)。

## 1. 核心思维与架构定位

- **GPU 加速的即时+保留混合范式**：GPUI 采用类似现代游戏引擎与专业工具（如 Zed）
  的亚像素级 GPU 渲染模型。每一帧通过 `Render` trait 声明性构建元素树，但在内部
  维持 `Entity<RootView>` 的保留状态句柄，兼具即时模式的编写简洁与保留模式的高性能。
- **状态权威与单向流向**：`RootView` 是窗口界面的唯一权威。数据从平台事件端口或
  `ShellApp` 进入，通过 `cx.notify()` 驱动微调度局部重绘。GPUI 禁止建立脱离
  `RootView` 生命周期的私有状态岛。
- **直绘 Canvas 与亚像素抗锯齿**：CPU 火花线、主性能历史曲线通过 GPUI `canvas`
  结合硬件顶点直接栅格化，平滑支持 60 秒刻度抗锯齿网格与渐变填充。

## 2. 布局与响应式规约

- **Win11 Task Manager 像素级复刻**：GPUI 作为全功能标杆实现，严格保持左侧导航栏、
  顶部快捷操作条、中间多列虚拟表格以及右侧/下部动态详情的经典布局。
- **响应式断点机制**：
  - `LayoutProfile::Wide`（宽屏）：双列展开，常驻侧栏与详情面板。
  - `LayoutProfile::Standard`（标准桌面 1180×780）：弹性自适应列宽与单行工具条。
  - `LayoutProfile::Compact`（紧凑 720×480）：顶栏文本自动隐藏并退化为纯图标模式，
    操作栏折叠为 Essential 核心命令，表格启用横向弹性滚动。
- **列宽契约**：严格绑定 `taskmanager_ui_contract::PROCESS_COLUMNS` 顺序，
  列宽调整通过 `mount_resize_handle` 实现鼠标拖拽与持久化记忆。

## 3. 页面模块与设备路由

- **页面划分**：覆盖 7 大核心页面（Processes, Performance, Services, System,
  Startup, Users, AppHistory）及模态浮层（Alerts, Settings）。
- **性能页设备切片**：根据侧栏点击的 `PerfDevice` 目标，精准路由到专有的硬件看板：
  - `Cpu`：核心利用率、60秒历史走势、基准频率、拓扑架构与核心网格。
  - `Memory`：使用量、已提交、分页池/非分页池、硬件预留与多色块内存构成条。
  - `Disk`：活动时间、读写吞吐速率、响应延迟与物理分区明细。
  - `Network`：实时收发吞吐、接口类型、IP/MAC 物理地址。
  - `Gpu`：3D / Compute / Copy / Video 独立引擎利用率与专有/共享显存。
  - `Battery`：充放电实时瓦数、健康度百分比与循环计数。

## 4. 交互、浮层与快捷键

- **右键上下文菜单**：行右键触发 `PopupMenuState`，包含 11 项标准进程操作
  （结束任务、结束树、挂起、恢复、强杀、优先级调整、能效模式、文件位置、在线搜索、属性等）。
- **键盘导航系统**：严格遵守 `Alt+1..7` 切页、`Ctrl+F` 聚焦搜索、`Delete`
  结束任务等全局和弦，单按键事件直接派发至焦点实体。
- **无头 Wayland 验收**：全量测试必须通过嵌套 Niri/KWin 虚拟 Framebuffer
  的像素级自动化验收套件（`accept-gpui-interactions.sh` 与 `accept-gpui-demo.sh`）。
