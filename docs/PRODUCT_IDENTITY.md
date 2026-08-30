# TaskForest 产品身份

## 用户可见名称

| 表面 | 名称 | 标识 |
|---|---|---|
| 共享品牌 | TaskForest / 任务森林 | Eye-friendly native system monitor |
| GPUI | TaskForestG | `io.github.YellowWhiteBlackCat.TaskForestG` |
| Iced | TaskForestI | `io.github.YellowWhiteBlackCat.TaskForestI` |
| Bevy | TaskForestB | `io.github.YellowWhiteBlackCat.TaskForestB` |
| TUI | TaskForest | 终端进程，无桌面 app id |

GPUI 是当前发行包形态。Iced、TUI 与 Bevy 支持源码构建；Bevy 走独立二进制
`taskforest-b`（由 `scripts/build-frontend-binaries.sh` 构建），成熟度口径见
[BEVY_UI_FRONTEND.md](BEVY_UI_FRONTEND.md)，不进入发行包矩阵。

## 程序与兼容名称

仓库和 crate 仍使用 `taskmanager` / `taskmanager-*` 内部名称，以保持配置、包和升级兼容。
Linux 发行包安装的主桌面可执行文件是 `taskforest-g`；兼容 CLI 名称为 `taskmanager`。

公开安装与产品说明只使用上表中的 TaskForest 标识；兼容名称只保留在必要的内部实现边界。

## 发布产物命名

所有 GitHub Release 资产遵循唯一约定：

```
TaskForest-<UI>-<版本>-<平台>.<格式>
```

- `<UI>`：发行包前端的单字母后缀（GPUI 为 `G`；Iced 若进入发行包为 `I`）；
- `<版本>`：完整 Cargo 版本；预发布后缀统一连写为 `rcN`（如 `0.1.0-rc5`，不带点），
  与 git tag（`v0.1.0-rc5`）逐字一致；
- `<平台>`：统一为 `x64` / `arm64`，与 DEB `Architecture`（`amd64`/`arm64`）和
  RPM arch（`x86_64`/`aarch64`）一一对应，包内元数据不改；
- `<格式>`：`deb` / `rpm` / `msi`（AppImage 若发布为 `AppImage`）。

示例：`TaskForest-G-0.1.0-rc5-x64.deb`。产物矩阵的权威表在
[RELEASE.md](RELEASE.md)。

## 特权 helper 与 polkit 命名空间

Linux 安装树中的特权面统一使用产品前缀，全部落在 `/usr/libexec/taskforest-*`：
`taskforest-setup-helper`、`taskforest-privilege-helper`、`taskforest-net-launcher`、
`taskforest-process-control-helper`（udev 规则资产为
`/usr/share/taskforest/setup/99-taskforest.rules`）。cargo 构建产物名仍是内部的
`taskmanager-*`，安装时映射为发行名。

polkit action id 与桌面 app id 共用同一 reverse-DNS 命名空间：
`io.github.YellowWhiteBlackCat.TaskForest.<feature>`。目前声明的四个 action：
`perf-helper`、`net-launcher`、`process-control`、`first-run-setup`；前三个的
`.policy` 安装文件名与 action id 同名（模板在 `polkit/`），`first-run-setup`
的安装文件是 `packaging/linux/io.github.YellowWhiteBlackCat.TaskForest.setup.policy`。

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
