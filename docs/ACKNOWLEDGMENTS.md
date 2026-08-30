# 开源致谢与第三方清单

本文是 TaskForest 所用第三方项目的公开署名清单，也是第三方事实与许可边界的唯一权威。
许可证全文不在本文复制：代码依赖随源码/锁文件分发，字体条款随包分发，详见各节链接。

## 设计基线

- **Mission Center**（GPL-3.0-or-later，
  [gitlab.com/mission-center-devs/mission-center](https://gitlab.com/mission-center-devs/mission-center)）：
  TaskForest 的部分信息层级和用户可见行为受其启发。TaskForest 是独立实现，未复制、
  移植或链接其任何 GPL 代码，因此不构成代码层面的衍生关系。

## 界面工具包

| 项目 | 许可证 | 在本项目中的角色 |
|---|---|---|
| [GPUI](https://github.com/zed-industries/zed)（Zed Industries） | Apache-2.0 | GPU 加速桌面主前端，发行包唯一形态（本地补丁见下节） |
| [Iced](https://github.com/iced-rs/iced) | MIT | 响应式桌面前端，受支持但暂不进发行包 |
| [Ratatui](https://github.com/ratatui/ratatui) | MIT | 终端前端 |
| bevy / bevy_app / bevy_ecs | Apache-2.0 OR MIT | Bevy 门面与应用骨架/ECS：独立 Bevy 前端及平台运行时复用 |
| naga | MIT OR Apache-2.0 | 着色器翻译；iced 渲染链的 feature pin |
| fontdb | MIT | 系统字体枚举 |
| raw-window-handle | MIT OR Apache-2.0 OR Zlib | 窗口句柄抽象 |

## 本地补丁依赖

`[patch]` 覆盖的三个上游 crate 以补丁副本参与构建，源码保留各自原始许可证：

- **gpui**（Apache-2.0，`patches/gpui`）：上游无条件同时启用 xkbcommon 的 wayland+x11
  feature，导致 wayland-only 构建仍链接 X11；补丁将 x11 移入 dev-only feature，生产
  二进制零 X11 链接。
- **cryoglyph**（MIT OR Apache-2.0 OR Zlib，`patches/cryoglyph`）：上游 0.1.0 固定
  `lru` 0.16；补丁副本只把依赖下限提到 0.18.2，API 不变（ADR-045）。
- **proc-macro-error2**（Apache-2.0 OR MIT，`patches/proc-macro-error2`）：仓库内补丁副本。

## 平台采集与系统集成

| 依赖 | 许可证 | 用途 |
|---|---|---|
| sysinfo | MIT | 跨平台系统事实 |
| nvml-wrapper | Apache-2.0 OR MIT | NVIDIA NVML 封装 |
| starship-battery | ISC | 电池状态（archived `battery` 0.7 的维护续作） |
| smbios-lib | MIT | SMBIOS/DMI 解析 |
| raw-cpuid | MIT | CPU 拓扑与指令集 |
| nix | MIT | Unix API 安全封装 |
| libc | Apache-2.0 OR MIT | 平台 FFI 类型 |
| rustix | Apache-2.0 OR MIT | safe Unix 系统调用 |
| wayland-client / wayland-backend / wayland-scanner | MIT | Wayland 协议绑定 |
| zbus | MIT | D-Bus（systemd/会话/通知） |
| windows / windows-registry / windows-service | Apache-2.0 OR MIT | Windows API 绑定 |
| windows-capture | MIT | Windows.Graphics.Capture 视觉证据采集 |
| plist | MIT | macOS 属性表解析 |

## 桌面集成与无障碍

| 依赖 | 许可证 | 用途 |
|---|---|---|
| ksni | Unlicense | StatusNotifierItem 系统托盘 |
| tray-icon | Apache-2.0 OR MIT | 跨平台托盘图标 |
| notify-rust | Apache-2.0 OR MIT | 桌面通知 |
| open | MIT | 以系统默认程序打开文件/URL |
| accesskit / accesskit_consumer / accesskit_unix | MIT OR Apache-2.0 | 屏幕阅读器无障碍树 |

## 应用基础设施

| 依赖 | 许可证 | 用途 |
|---|---|---|
| serde / serde_json | Apache-2.0 OR MIT | 序列化 |
| chrono | MIT OR Apache-2.0 | 进程启动时间的本地时间呈现 |
| tracing / tracing-subscriber | MIT | 结构化日志 |
| crossbeam-channel | Apache-2.0 OR MIT | 采集线程与 UI 的消息通道 |
| bitflags | Apache-2.0 OR MIT | 位标志类型 |
| ringbuf | Apache-2.0 OR MIT | 遥测环形缓冲 |
| futures-timer | Apache-2.0 OR MIT | 异步计时 |
| sha2 | Apache-2.0 OR MIT | 证据链摘要 |
| atomicwrites | MIT | 原子落盘 |
| ctrlc | Apache-2.0 OR MIT | 终端信号处理 |

## 构建与测试（不进入发行二进制）

| 依赖 | 许可证 | 用途 |
|---|---|---|
| embed-resource | MIT | Windows 资源嵌入（build 依赖） |
| usvg | Apache-2.0 OR MIT | 图标资产测试中的 SVG 解析（dev 依赖，不进发行二进制） |
| proptest | Apache-2.0 OR MIT | 属性测试（dev 依赖） |

## 捆绑字体

字体随二进制分发以保证 CJK 与等宽字形一致，具体条款见
[资产字体许可](../crates/taskmanager-assets/assets/fonts/LICENSE.md)与
[资产许可](../crates/taskmanager-assets/ASSET-LICENSE.md)：

- **MiSans VF**（Xiaomi / Hanyi，SIL OFL 1.1）：汉字回退与可选正文字体，OFL 条款随包分发。
- **Roboto Mono VF**（Google，SIL OFL 1.1）：等宽/指标字体与最后回退，wght 100–700；
  许可证文本随包分发。

## 口径

- 本表列出全部**直接**依赖的名字与许可证；版本不在此复制，以仓库锁文件为唯一版本权威。
- 传递依赖闭包同样以锁文件为准；发布门禁中的依赖审计负责复核许可证集合。
- 二进制发行包（MSI、deb、rpm）随包携带打包期生成的
  `THIRD-PARTY-NOTICES.txt`（[scripts/gen_third_party_notices.py](../scripts/gen_third_party_notices.py)）：
  内含非开发依赖闭包中每个第三方 crate 的许可证全文与捆绑字体的 OFL 条款；本表不复制这些全文。
- 第三方名称与商标归各自所有者；本文署名不构成赞助或背书。
