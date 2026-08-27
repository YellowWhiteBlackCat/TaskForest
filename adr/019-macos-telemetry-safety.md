# 019 — macOS 遥测的 Safe-Rust 策略与诚实缺口清单

- 状态：已接受
- 相关：`crates/taskmanager-platform-macos/`、`adr/018-windows-telemetry-safety.md`（Windows 同策略）

macOS capability catalog 对没有合格 Safe 来源的 optional facet 保留注册描述符并提交
typed `Unsupported`；目录枚举完整不等于 on-box 能力声明。

## 决策

macOS 适配器**只用发布在 crates.io 的 Safe 封装库 + 有界 `std::process` shell-out**实现
遥测；没有 Safe 来源的 Windows/macOS API **一律不调用**——对应 provider 注册 typed
`Unsupported` 结果，缺口在本 ADR 如实记录。

约束等级：

1. `#![forbid(unsafe_code)]` 维持（与 Linux/macOS/Windows 适配器一致）。
2. 不新增手写 FFI 模块。
3. shell-out 是**有意的架构选择**：Linux 适配器对 systemctl/journalctl/smartctl 就是这么做的
   （`engine/bounded_command.rs` 2 秒硬超时 + 管道并发排空）；macOS 对其系统工具
   （launchctl / smartctl / defaults / system_profiler / who / log / renice / open -R /
   sysctl / ps）
   同构镜像，超时与错误分类一致。
4. 与 Linux 相同的 typed 失败模型：`MissingDependency` / `PermissionDenied` /
   `TemporarilyUnavailable` 走 `ProviderFailure` + `SourceOutcome`，绝不伪造零值。

## Safe 库与工具选型

| 来源 | 职责 | 说明 |
|---|---|---|
| `sysinfo` 0.39（同版本复用） | CPU 占用/每核/频率/brand、内存、进程清单/CPU/内存/磁盘 IO 总量/信号控制、网络计数/MAC、磁盘容量/只读、**温度（SMC 事件，Intel+ARM）**、users | 比 Windows 面更全：`kill_with(Signal)` 提供 POSIX 信号控制 |
| `battery` 0.7（同版本复用） | 电池容量%/状态/电压/功率 | core-foundation 后端 |
| `open` 5（同版本复用） | URL 打开 | 平台 shell |
| `plist` 1.10 | LaunchAgents/Daemons 解析与回写、diskutil 输出解析 | 纯 Rust |
| `serde_json` | system_profiler / smartctl JSON | 已在工作区 |
| `std::process` + `command.rs` | launchctl / smartctl / defaults / system_profiler / who / log / renice / open -R / sysctl / ps | 与 Linux bounded_command 同构（复制自 Linux 适配器） |

## 已实现域

| 域 | 内容 | 诚实边界 |
|---|---|---|
| host/cpu/memory | sysinfo（uptime、进程数、每核占用、brand、频率、内存+swap）+ 有界 `ps` 快照求和的主机线程数 | 线程数在 `ps` 快照缺失时 → typed Unavailable（不伪造 0） |
| **温度** | **sysinfo Components（SMC 事件服务，Intel+ARM）** | 风扇转速无 Safe 访问器 → 缺失 |
| storage | sysinfo 容量/只读 + **smartctl JSON SMART 属性**（diskutil 发现物理盘） | IOPS/吞吐无 Safe 计数源 → Unavailable；smartmontools 缺失 → MissingTool |
| SMART 自检 | smartctl -t 启动 + smartctl --json 轮询（简化单次镜像 Linux 策略机） | — |
| **目录占用扫描** | 共享 `DirectoryUsageScanner`（pure safe-`std::fs`、有界可取消分块；APFS firmlink/克隆/符号链接树按构造不跟随） | 实机 APFS 大目录 receipt 未取 |
| network | sysinfo 收发字节/速率/MAC + **`networksetup`/`airport`/`ifconfig` 解析**（WiFi SSID/链路速率） | 解析失败 → None（不猜测） |
| process list | sysinfo 全字段 + **fd 计数（sysinfo `open_files`）+ 有界 `ps -Ao pid,nice,thcount` 快照（线程数/nice，~5s 缓存）** | 快照 miss/列为空 → 该标量 typed Unsupported（绝不伪造 0） |
| **process control** | **sysinfo kill_with（Term/Kill/Stop/Continue/Hangup/Interrupt/User1/User2）** + renice 优先级 | — |
| process insights | 资源（内存用量）、线程清单（pending） | 逐进程网络/GPU/隔离/open-files 明细 → Unsupported |
| services | **launchctl list 清单 + kickstart/kill/enable/disable 控制 + log show 日志快照** | 系统域守护进程需特权 → 诚实省略（用户域清单）；依赖图/流式日志 → Unsupported |
| startup | **plist 解析 LaunchAgents/Daemons + Disabled 键回写控制** | /System/... 只读目录 → control_policy Unsupported |
| **启动证据** | **`sysctl -n kern.boottime`（启动证据 + 开机墙钟时间戳）** | 分阶段启动耗时无等价物 → 不提供（不伪造 0ms） |
| sessions | **who 会话清单 + id -u** | 会话控制（锁屏/断开）→ Unsupported |
| power | battery crate | 循环次数 crate 不给 → Unavailable |
| filesystem health | sysinfo 只读标志 | 错误计数/完整性 → 缺失 |
| appearance | **defaults read AppleInterfaceStyle / AppleIncreaseContrast** | — |
| hardware | sysinfo + **system_profiler SPHardwareDataType -json**（型号/芯片） | 固件版本无 Safe 访问器 → typed Unavailable 片段 |
| integration | open crate URL + **open -R 资源揭示** + sh -c 命令启动 | 桌面通知/首启 setup → 注册-pending（见下表） |

## 诚实缺口清单（只记录，不实现）

永久 Unsupported（无 Safe 路径或天然缺失）：

| 能力 | 需要的 macOS API | 现状 | 未来 Safe 路径 |
|---|---|---|---|
| GPU 整域 | Metal/IOKit（unsafe） | **Unsupported**（2019 后无 NVIDIA 驱动，无 NVML） | 无 |
| 每进程 GPU/网络 | Metal HUD / nettop（需特权） | Unsupported | 无 |
| 风扇转速 | SMC（IOHID 无 Safe 封装） | 缺失 | sysinfo 未来版本若提供 |
| 磁盘 IOPS/吞吐 | 无 Safe 计数器源 | Unavailable 标量 | 无 |
| 进程隔离/沙箱（entitlement 域） | Sandbox/entitlements（unsafe） | Unsupported | 无 |
| CPU 亲和 | macOS 无此 API | Unsupported（天然） | — |
| 服务依赖图/流式日志 | launchd 无依赖图；log stream 无界 | Unsupported | — |
| 会话控制 | 无 Safe API | Unsupported | — |
| 电池循环次数 | SMC（无 Safe 封装） | Unavailable 标量 | — |
| 固件版本/BIOS | 无 Safe SMBIOS 等价物 | typed Unavailable 片段 | system_profiler 部分覆盖 |
| 逐进程网络提权链 | AF_PACKET/SCM_RIGHTS 是 Linux 面 | Unsupported（off-Linux 无等价链） | — |

注册-pending（描述符在目录中，提交完成于 typed `Unsupported`）：

| 能力 | 缺 Safe 路径的原因 | provider 身份 | 未来 Safe 路径 |
|---|---|---|---|
| open-files 明细（`process.insights.open_files`） | sysinfo 只给 fd **计数**；fd→target 列表（`proc_pidinfo(PROC_PIDLISTFDS)`）无 Safe 封装 | `macos.process.insights.open_files` | 有界 `lsof` shell-out 可作候选 |
| 桌面通知（`alerts.notify`） | Linux 的 freedesktop DBus 路径不存在于 macOS；NSUserNotificationCenter 无已发布 Safe 封装 | `macos.alerts.desktop-notification` | 有界 `osascript display notification` shell-out |
| 首启 setup（`first-run.setup`） | 尚未为 macOS 打包固定 setup 资产与提权 helper（Linux 打包 setup.sh + pkexec helper） | `macos.first-run.setup-script` | 打包资产 + osascript 授权 |

> 注：契约测试**不提交真实 URL 打开**（会触发平台浏览器，污染无头/CI 环境）；
> `shell.url.open` 的接受性由组合测试 + 实机验证覆盖。

## 验收标准

- `cargo check/clippy --workspace --all-targets -D warnings` 零警告（macOS crate 在
  Linux 上同样编译，sysinfo/battery 可跑真实数据；launchctl/smartctl shell-out 在
  Linux 上以 MissingDependency 诚实降级）。
- macOS crate 测试覆盖 pending provider typed Unsupported、
  plist 解析/回写、日志行解析、smartctl JSON 映射、目录扫描、契约四件套
  （组合面 43 描述符/观察面 5 条 pending 失败/控制面 4 条 pending 失败/目录健康）。
- 无浏览器/外部副作用测试。
