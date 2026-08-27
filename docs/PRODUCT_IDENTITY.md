# TaskForest 产品身份

## 用户可见名称

| 表面 | 名称 | 标识 |
|---|---|---|
| 共享品牌 | TaskForest / 任务森林 | Eye-friendly native system monitor |
| GPUI | TaskForestG | `io.github.YellowWhiteBlackCat.TaskForestG` |
| Iced | TaskForestI | `io.github.YellowWhiteBlackCat.TaskForestI` |
| Bevy | TaskForestB | `io.github.YellowWhiteBlackCat.TaskForestB` |
| TUI | TaskForest | 终端进程，无桌面 app id |

GPUI 是当前发行包形态。Iced 与 TUI 支持源码构建；Bevy 是实验性前端，不进入发行包。

## 程序与兼容名称

仓库和 crate 仍使用 `taskmanager` / `taskmanager-*` 内部名称，以保持配置、包和升级兼容。
Linux 发行包安装的主桌面可执行文件是 `taskforest-g`；兼容 CLI 名称为 `taskmanager`。

公开安装与产品说明只使用上表中的 TaskForest 标识；兼容名称只保留在必要的内部实现边界。

## 图标权威

- 应用 SVG：`packaging/linux/io.github.YellowWhiteBlackCat.TaskForest.svg`
- 托盘 SVG：`packaging/tray/taskforest-tray.svg`
- freedesktop 图标 token：`taskforest-taskboard`
- 平台派生资产由 `bash packaging/regenerate-icons.sh` 统一生成。

PNG、ICNS、ICO 和内嵌 RGBA 都是派生产物，不允许手工维护平台分叉。Windows EXE、MSI
快捷方式和卸载列表必须使用同一 ICO；macOS bundle 必须包含对应 ICNS。

## 平台消费契约

- Wayland `app_id` 与 `.desktop` 文件名一致；
- 桌面包同时安装可执行文件、desktop entry、AppStream metadata 和图标；
- macOS 只把完整 `.app` bundle 视为安装形态；
- Windows MSI 使用固定 UpgradeCode，并为 GPUI 可执行文件建立开始菜单入口；
- 用户级开发安装与系统包由不同所有权回执管理，不能互相删除文件。

产品 URL 统一为 `https://github.com/YellowWhiteBlackCat/TaskForest`。
