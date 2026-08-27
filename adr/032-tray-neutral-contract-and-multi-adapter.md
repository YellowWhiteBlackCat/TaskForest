# ADR-032: 三平台系统托盘中立契约与多适配器

- 状态：已接受
- 相关：`adr/017-own-ui-component-layer.md`、`adr/029-one-binary-three-ui-shapes.md`、
  `adr/031-windows-native-safe-boundary.md`、`docs/PERMISSION_MODEL.md`

## 背景

系统托盘（tray）是三个 UI 形状（GPUI/Iced/TUI）共有的桌面集成能力；三个平台的
原生托盘差异巨大：

- **Linux**：协议是 freedesktop StatusNotifierItem（D-Bus）。传统落地路线
  （`tray-icon`/`muda` 的 GTK + libappindicator/libayatana-appindicator）正在衰败：
  gtk-rs 有 RUSTSEC-2024 系列无人维护通告、libayatana-appindicator 已弃用，且在 KDE
  Plasma 6 / Wayland 上有已复现的"不注册 StatusNotifierWatcher"问题。本仓库零 GTK、
  Wayland 优先，这条路线不可接受。
- **Windows**：`Shell_NotifyIcon` + Win32 菜单，需要 win32 消息循环泵取隐藏窗口回调。
- **macOS**：`NSStatusItem` + `NSMenu`，只允许主线程创建和使用。

单一"一个 crate 通吃"（`tray-icon`）在 Linux 强依赖 GTK，与本仓库"业务 crate
`#![forbid(unsafe_code)]`、unsafe 只限四个 audited 边界 crate、优先成熟 safe crate"
的铁律冲突。

## 决策

采用**中立契约 + 多适配器**：托盘词汇在 `taskmanager-core` 定义一次，三个平台适配器各自
实现，跨平台共用逻辑收进一个共享 crate。

1. **中立契约 `taskmanager-core::tray`**：`TraySpec` / `TrayMenuSpec` / `TrayMenuItem`
   （Action/Checkmark/Radio/Submenu/Separator）/ `TrayIconData`（RGBA）/ `TrayEvent`
   （MenuActivated/IconActivated/IconDoubleClicked）。构造期校验：RGBA 长度 = 宽×高×4、
   尺寸上限 256、菜单节点 ≤64、嵌套深度 ≤3、标签/工具提示/标题长度上限、radio 组最多
   一个选中。零 unsafe、零 OS/线程/队列类型。
2. **契约 seam**：`taskmanager-platform-contract::TrayController`（`Send + Sync`）与
   `TrayFailure`（`Unsupported` / `MissingDependency` / `TemporarilyUnavailable` /
   `WrongThread` / `Rejected`）；`taskmanager-platform-native::tray::spawn_tray` 按
   `cfg(target_os)` 分发到各 OS crate；`taskmanager-app-host` 与
   `taskmanager-application` 再导出，使被依赖防火墙挡在 core 之外的 TUI 也能构建托盘
   spec。
3. **多适配器**：
   - Linux `taskmanager-platform-linux::tray`：`ksni` 0.3（`blocking` + `async-io`，
     **不启用 tokio**，避免 ksni README 记录的 zbus#526 混合执行器 panic；与仓库既有
     zbus 5.18 的 async-io+blocking-api 默认一致）。无 GTK、无额外线程；无 session bus
     或无 StatusNotifierWatcher 时 spawn 以 typed `MissingDependency` 失败，产品优雅
     无托盘。
   - Windows `taskmanager-platform-windows::tray`：`tray-icon` + `muda` 在专用宿主线程
     上创建，配自有的有界 `PeekMessageW` 消息泵。消息泵是唯一新增 raw Win32 面，收进
     audited `taskmanager-windows-api`（`pump_pending_messages`，`// SAFETY:` 注释、无
     句柄/指针/缓冲区跨公共 API）；控制器全方法经通道转发到宿主线程，故 `Send + Sync`。
   - macOS `taskmanager-platform-macos::tray`：`tray-icon` + `muda` 在应用主线程创建，
     原生对象存主线程 `thread_local` 槽；跨线程变更以 typed `WrongThread` 拒绝；在非
     创建线程 Drop 时清理延迟到进程退出（文档化、一次性、无害）。事件由后台转发线程
     泵 `tray-icon` 全局通道，再入前端自有通道。
   - **共享 `taskmanager-tray-muda`**：spec→muda 菜单的唯一映射（菜单 id 编码
     `taskmanager:<id>`、CheckMenuItem 建树、radio 组互斥 `RadioState::set_checked`），
     Windows 与 macOS 适配器共用——遵守"禁止跨 crate 复制私有助手"规则。
4. **平台能力不对称诚实化**：`set_title` 在 Windows → `Unsupported`；`set_visible` 在
   Linux(SNI 无隐藏/显示) → `Unsupported`；`set_tooltip` 三平台可用。
5. **前端宿主**：GPUI/Iced/TUI 各自在其事件循环线程调用 `spawn_tray`；TUI 在 macOS
   记录为不支持（`NSStatusItem` 要求主线程 + 无 Cocoa 运行循环）。宿主接线（图标资产、
   i18n 文案、退出语义、"显示窗口"映射）由各 frontend README 与当前 receipt 管理，本 ADR 只交付接缝与适配器。

## 不做什么

- 不引入 GTK / libappindicator / xdotool。
- 不把 audited 边界扩成通用 Win32 框架：只加一个有界消息泵。
- 不给托盘造 request port / capability：托盘是进程生命周期对象，不是 worker-lane
  capability，事件走前端自有通道，不经 platform-runtime 车道。
- 当前不实现三个前端的完整宿主接线；该边界需要各前端分别满足自己的线程模型与证据门。

## 验收约束

- `taskmanager-core` / `-contract` / `-native` / `-linux` / `-windows` / `-macos` /
  `-tray-muda` 均保持 `#![forbid(unsafe_code)]`；`taskmanager-windows-api` 消息泵是唯一
  新增 unsafe 面且逐块 `// SAFETY:`。
- 纯校验、映射和状态转换必须有单元测试；测试数量不在 ADR 中维护。
- 平台适配器在各自目标交叉编译通过；Linux 协议级测试在 `dbus-run-session` 可用时跑
  （ksni 自身测试同法）。
- quick 门禁通过。

## Current integration status

- GPUI 已有托盘宿主接线；其窗口、i18n 和退出语义仍按 GPUI crate README 与当前视觉/交互 receipt 验收。
- GPUI 托盘生命周期已闭环：主窗口关闭只最小化并保留 root/ECS/单例守卫，托盘 Quit 才退出进程；二次启动通过单例事件恢复并激活原窗口。托盘不可用时保留无托盘退出回退，Linux D-Bus 激活通知有界且不会让二次进程悬挂。
- 单例 seam 已由 platform contract/native 与 Windows boundary 承载；跨平台 release claim 仍要求对应 native receipt。
- Iced 宿主接线和品牌 RGBA 图标仍是开放项；TUI 桌面托盘明确不适用。

历史逐项跟踪属于私有发布准备材料，不进入当前公开 ADR 路由。
