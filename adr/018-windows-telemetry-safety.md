# 018 — Windows 遥测的 Safe-Rust 策略与诚实缺口清单

- 状态：已接受
- 相关：`crates/taskmanager-platform-windows/`、`adr/031-windows-native-safe-boundary.md`、`docs/CROSSPLATFORM_STRATEGY.md` §7.2

## 决策

Windows 适配器默认**只用发布在 crates.io 的 Safe 封装库**实现遥测功能；对主机刷新关键、
调用面极小且没有合适 Safe wrapper 的 Windows API，只允许经 ADR-031 的专用审计边界进入。
任何不满足"少量稳定调用 + typed safe seam + 反向防火墙"的 API **一律不调用**——对应
provider 注册 typed `Unsupported` 结果，缺口在本 ADR 的清单中如实记录，并注明候选 Safe
库与未来接入路径。完整能力集合以 platform contract/provider catalog 为准。

约束等级：

1. `taskmanager-platform-windows` 与业务层继续 `#![forbid(unsafe_code)]`；原生调用只能
   进入独立的 `taskmanager-windows-api` 边界，边界根使用 `#![deny(unsafe_op_in_unsafe_fn)]`。
2. 不在适配器内新增手写 FFI；ADR-031 边界只允许已登记的最小 API，不得演变成通用 Win32
   绑定层。
3. 标准产物 `hardware-all` 默认包含全部 Safe 可实现的 provider；NVML 等硬件后端运行时选择，不是发行 SKU。
4. 与 Linux 相同的 typed 失败模型：`MissingDependency` / `PermissionDenied` / `Unsupported`
   走 `ProviderFailure` + `SourceOutcome`，界面显示 "—" 或明确文案，绝不伪造零值。
5. Windows 生产、测试和开发辅助路径均**禁止 PowerShell 与其它命令解释器承担遥测**；
   系统脚本不得补齐可选字段。`smartctl`、`explorer` 和用户显式
   命令启动等非遥测兼容工具仍各自使用固定可执行文件、argv 和有界等待，不能借此重新
   引入脚本遥测。
6. ConfigStore 写入用 `atomicwrites` safe API（bounded sibling temp file +
   `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`），配 last-known-good `.bak` 恢复
   与保存代际；启动项控制只对已识别的 `StartupApproved\Run` 二进制状态做 partial
   update，不删 Run 值/文件，未知 blob 保持 typed Unsupported。
7. 本 ADR 记录的 Safe/边界接线均为**结构事实**；真机数字、DirectWrite 字体落盘与权限
   行为以 required Windows runner receipt 为准，headless/cross-target 证明不替代。

## Safe 库选型

| crate | 版本 | 职责 | 内部机制（Safe 化包装） |
|---|---|---|---|
| `sysinfo` | 0.39（与 platform-linux 同版本） | CPU 占用/每核、内存、进程清单/CPU/内存/磁盘 IO 总量/启动时间、网络计数、磁盘容量/只读、主机/内核信息 | `GetSystemTimes` / `SystemProcessInformation` / `GetIfTable2` / `GetDiskFreeSpaceExW` |
| `raw-cpuid` | 11.6 | x86/x86_64 advertised processor base/max frequency；缺失 CPUID leaf 时 typed unavailable | safe CPUID reader/intrinsics 内部封装 |
| `smbioslib` | 0.9.4 | DMTF SMBIOS 规范解析（Type 0 BIOS、Type 1 系统/主板） | safe 纯内存字节流结构解析 |
| `notify-rust` | 4.18 | Windows 桌面 Toast 告警通知（`alerts.notify`） | safe 包装，内部通过 WinRT Toast / Shell API 发送 |
| `nvml-wrapper` | 0.12（与 platform-linux 同版本） | NVIDIA GPU：利用率/温度/功耗/显存/频率/风扇/驱动版本；每进程显存 | 动态加载 `libnvidia-ml.dll` |
| `windows-registry` | 0.6（微软官方） | 启动项 Run 键、桌面深浅色、高对比标志 | `Reg*` 系列 |
| `starship-battery` | 0.11.1 | 电池容量%/充电状态/电压/功率/循环次数 | Windows safe API 封装 |
| `atomicwrites` | 0.4.4 | ConfigStore primary/backup 有界原子替换 | safe API；Windows 内部使用 `MoveFileExW` replace/write-through，应用层不接触 FFI |
| `open` | 5 | URL 打开 | 平台 shell |
| `std::process::Command`（兼容工具） | — | `smartctl`、`explorer /select` 与用户显式的 `cmd /C` 命令启动；**不得用于遥测** | 固定 executable + argv、3–5 s 有界等待；输出有 4 MiB 上限 |
| `taskmanager-windows-api`（ADR-031） | workspace local | host performance、locale、Known Folder、精确进程终止/优先级/亲和控制、提权判定、WTS 会话/注销/锁定、SCM start mode、processor topology/cache、NIC link metadata、IP Helper 连接表、ToolHelp32 线程、token/SID isolation、DXGI GPU 显存、D3DKMT PCI 地址、PDH（每核频率、GPU engine/adapter/process memory）、ACPI 热区（WMI/COM）、事件日志（EvtQuery/EvtSubscribe）、每线程挂起/恢复、power overlay、SMBIOS 固件表、句柄表 open-files（SystemHandleInformation + NtDuplicateObject + NtQueryObject 祭线程模式）、内存压缩（SystemProcessInformation 快照）、SetupAPI 计算加速器（NPU 库存）、EnumDisplayDevices + 注册表 EDID、job-object 限额（嵌套 job，会话级）、每进程 environment/cwd（PEB 读取，同位宽）、Task Scheduler 启动任务枚举（COM，只读）、时区规则（GetDynamicTimeZoneInformation/ForYear）；只出 typed 值/错误 | 最小 audited Win32 boundary；不暴露原生 handle/pointer |

## 已实现域（Safe 库 + ADR-031）

| 域 | 内容 | 诚实边界（同表内注明） |
|---|---|---|
| host | uptime、进程数（sysinfo）+ 主机线程总数（ADR-031 `K32GetPerformanceInfo`） | native API 不可用 → 线程标量 typed Unsupported；不阻塞其它 host facts |
| cpu | 全局/每核占用、brand、物理/逻辑核、live per-core frequency（PDH Task-Manager 同源算法）+ advertised base/max（raw-cpuid）+ socket/L1/L2/L3（bounded `GetLogicalProcessorInformationEx`）+ 聚合温度（ACPI 热区 WMI/COM，含 LHM/OHM 命名空间，sysinfo Components 兜底）+ energy_preference（effective power overlay） | 每核温度无用户态来源 → 聚合温度 only；功耗/能耗 → typed Unsupported；CPUID leaf 或 topology query 缺失时对应字段 unavailable |
| memory | total/used/available/swap（sysinfo）+ 压力速率（sysinfo delta，MiB/s）+ commit/cache/pool（`K32GetPerformanceInfo`）+ **内存压缩大小（`SystemProcessInformation` 快照里 "Memory Compression" 伪进程的 WorkingSet，免提权免句柄；无该进程 = typed 缺席，绝不报零）** | 压缩交换（zram 对应物）无 Windows 等价 → unavailable |
| storage | 容量/可用/fs/挂载点/可移除/只读 + read/write throughput delta（长生命周期 `sysinfo::Disks::usage()`） | IOPS/活跃时间/响应时间 → typed Unsupported；底层累计计数未确认前不投影 idle zero |
| filesystem directory usage | 挂载点/fs 类型/只读标志（sysinfo）+ **有界目录扫描（共享纯 Safe `std::fs` `DirectoryUsageScanner` 委托）** | 深度/条目/报告有界；symlink 只计数不跟随；不可读目录 typed PermissionDenied；根不可读 → typed 终态失败 |
| network | 每接口收发字节/速率/总量/MAC/IP/operational state（sysinfo）+ negotiated link speed（bounded `GetIfTable2`） | SSID/driver/adapter classification → typed Unsupported；接口 query 失败不从吞吐量推断速率 |
| gpu | NVIDIA（NVML）：利用率/温度/功耗/显存/频率/风扇/驱动；**AMD/Intel/通用 GPU（DXGI 降级，ADR-031）：专有显存总量/用量、共享内存总量/用量（共享实时用量经 PDH `\GPU Adapter Memory(*)\Shared Usage` 按 LUID 匹配合入，与任务管理器同源）、设备名、每引擎利用率（PDH `\GPU Engine(*)`）** | 无 GPU、WDDM 不支持显存查询或 PDH 无对应实例时对应标量 unavailable，不伪造 0 |
| telemetry.gpu.engines | 每引擎利用率行（PDH `\GPU Engine(*)` per-adapter/per-engtype 聚合，免提权；Linux 侧对应 lane 走特权 Intel PMU helper，Windows 无需提权） | PDH 不可用 → typed 失败快照；未知 device_id → typed 失败 |
| process list | PID/父PID/名/命令行/exe/CPU%/内存/磁盘速率（sysinfo disk_usage delta）/启动时间/结束控制；handle 数（sysinfo `open_files`）；优先级/nice、所有者（token/SID→account）、线程数（ADR-031）；60 s 历史环（cpu/mem/disk-read/disk-write） | 无 Safe accessor 的字段缺失时对应列 unavailable |
| process insights | 资源（内存用量）、GPU（每进程利用率经 PDH `\GPU Engine(pid_*)` 聚合 + 每进程显存经 PDH `\GPU Process Memory(*)`，NVML 仅作 NVIDIA 补充）、网络连接（IP Helper 连接表）、隔离（TokenElevation/AppContainer/IntegrityLevel）、threads（ToolHelp32 + GetThreadDescription/GetThreadTimes）、**environment/cwd（PEB 读取：`NtQueryInformationProcess(ProcessBasicInformation)` + 宽度推导偏移 + 有界 `ReadProcessMemory`；跨位宽（WOW64）诚实拒绝；预算与 core `MAX_ENVIRONMENT_BYTES/ENTRIES` 对齐）**、**open-files（`SystemHandleInformation` 全表 → `NtDuplicateObject` → `NtQueryObject` 祭线程模式：命名管道句柄的名称查询会永久阻塞，唯一缓解 = 专用线程 + 超时 TerminateThread，Process Hacker 同法；仅 File 类对象入选，非文件内核对象绝不冒充 fd；1024 条/进程上限）** | open-files 限同用户进程（他用户 owner 打不开 → typed `PermissionDenied`，与 Process Explorer 行为一致）；每进程网络**字节速率** → 永久 Unsupported（见缺口清单） |
| process control | End/Kill、SetPriority、SetAffinity 经创建时间二次核验的 native exact-handle boundary；Suspend/Resume 经文档化逐线程路径（ToolHelp32 快照 + `OpenThread(THREAD_SUSPEND_RESUME)` + `SuspendThread`/`ResumeThread`；不用未文档化的 `NtSuspendProcess`，且两种机制不可混用）；**资源限额（`process.resource.control`）：边界自持嵌套 job（Win8+）施加内存/进程数/CPU 百分比限额，CPU 配额只在 quota/period 为整数百分比时接受（2.5 核配额诚实拒绝，绝不四舍五入）** | 旧 PID/身份失败保持 `IdentityChanged`；job 限额**只收紧、会话级**（应用退出即失效，非持久 cgroup 写入——UI 必须如实标注）；他人匿名 job 不可编辑；越权 → PermissionDenied（escalation 词表无 job 项） |
| services | `windows-service` 原生 SCM 清单/状态/依赖/启停/重启；`taskmanager-windows-api` 只对启用/禁用做 start mode partial update，保留既有 binary path 与启动参数；缓存 ~5 s；**事件日志快照/增量流（winevt `EvtQuery`/`EvtSubscribe`，System 频道标准用户可读）** | Security 频道 → PermissionDenied；EvtQuery 失败 → typed ProviderFailure；依赖仅投影 `requires`；服务无事件 → 诚实 Empty |
| sessions | WTS 原生清单（会话名/用户/状态）与 WTS `LogoffSession`；**Lock = `LockWorkStation()`（调用会话，免提权）** | WTS 不提供 Unix UID，uid=0；非 Windows → MissingDependency |
| startup | 用户/系统 Run/RunOnce 键 + Known Folder Startup 文件夹 + **Task Scheduler 登录/开机触发任务（COM `ITaskService` 只读枚举，`StartupSource::ScheduledTask`）**；Run 与 StartupFolder 的 `StartupApproved` 12-byte 状态经 bounded parser；**StartupFolder 控制已实现**（同格式 blob，键 = 文件名，只翻转状态字节/按文档布局创建，绝不删文件）；启动耗时证据 = Diagnostics-Performance/Operational 事件 100（winevt） | unknown approval blob 不进入 enabled 清单；任务的 enable/disable 保持 Unsupported（改动任务库需独立控制 seam，charter 未立）；任务 scope 未定（需 definition principal）→ `StartupScope::Unknown` 诚实 |
| startup control | 只修改已存在、已识别的 `StartupApproved\Run` status byte；不删 Run 值/文件 | 缺 approval blob、RunOnce、StartupFolder → Unsupported；权限/身份失败 typed |
| config/path | ConfigStore bounded atomic replace + backup recovery + generation ordering；Windows Known Folder native path | primary/backup 都损坏才回到 default；Known Folder 不可用时使用绝对 temp fallback，不使用 CWD 相对路径 |
| local time | **时区规则（`GetDynamicTimeZoneInformation` + `GetTimeZoneInformationForYear` 有界年窗 → 纯函数合成 TZif v2 → 复用 core `LocalTimeRules::from_tzif` 解析器，零 core 改动；经 platform-native cfg 分支接入 app-host）** | 合成不可表示（无报告瞬时的固定偏移变更、瞬时冲突）或解析器拒绝 → typed `ProviderFault`，**绝不回退 UTC 假装本地时区** |
| power | 电池容量%/状态/电压/功率（starship-battery）+ 压力速率派生 | 循环次数等字段缺失时 Unavailable |
| filesystem health | 挂载点/fs 类型/只读标志（sysinfo） | 错误计数/完整性状态 → Unsupported |
| SMART/自检 | 观测 + 控制均走有界 `smartctl` shell-out（`-a` 解析自检日志段 / `-t <kind>`），ATA/NVMe 同路径 | smartmontools 缺席 → MissingDependency；实盘读数 on-box-unverified |
| sensors | ACPI 热区（WMI/COM `MSAcpi_ThermalZoneTemperature` 等，含 LHM/OHM 命名空间）+ sysinfo Components 兜底；validated/去重/排序 | OEM firmware 无热区或兜底来源身份不明 → discovery typed `Partial(Unsupported)`，不伪造为空设备 |
| appearance | 深浅色 + 高对比（windows-registry `AppsUseLightTheme` / `HighContrastOn` DWORD） | 键/值缺失 → 对应字段 None，不出错 |
| integration | URL 打开（open）、命令启动（cmd /C）、资源揭示（`explorer /select,<path>`）、**桌面通知（`notify-rust` WinRT Toast）** | 揭示无缓存 exe 路径 → TemporarilyUnavailable；explorer 缺席（Linux CI）→ MissingDependency |
| hardware inventory | 主机/内核/拓扑（sysinfo）+ base frequency（raw-cpuid）+ socket/cache topology（ADR-031）+ SMBIOS 固件信息（ADR-031）+ **显示器清单（`EnumDisplayDevicesW` + 注册表缓存 EDID + portable 纯解析器，Linux/Windows 同源）**+ **已装应用计数（ARP `Uninstall` 注册表三棵树、仅计非空 DisplayName，`package_count`；`package_manager` 诚实留空——ARP 不是包数据库）** | 不回退 WMI/脚本；无 SMBIOS 时字段返回 None；EDID 缺席 → 仅身份行（typed Partial）；hive 不可读 → typed 降级 |
| accelerator.npu | **SetupAPI 计算加速器设备类（`COMPUTE_ACCELERATOR_CLASS_GUID`）枚举：身份/品牌/驱动描述，免提权；空清单 = 诚实"无 NPU"成功** | 利用率/每引擎/显存 → typed unavailable（任务管理器背后的计数器集合未公开，见缺口清单）；与 Linux `/sys/class/accel` 的 inventory-first 策略对称 |

## Registered-pending optional facets

描述符已注册（目录枚举诚实），提交完成 typed `Unsupported`。当前余一项
（`filesystem.directory.usage`、`alerts.notify`、`process.insights.open_files`
均为真实能力，见已实现域表）：

| 能力 | 描述符身份 | 原因 | 未来 Safe 路径 |
|---|---|---|---|
| `first-run.setup` | `windows.first-run.setup-script` | 未打包 setup 资产/提权 runner（Linux 是 setup.sh + pkexec 对） | 打包决策 + elevated runner |

## 诚实缺口清单（只记录，不实现）

> 当前仍开放的缺口。**永久 Unsupported**（无 Windows 等价物，或唯一机制需管理员且无
> safe 封装）标注"永久"；其余为有明确路径的候选。隔离/亲和/线程/
> 挂起恢复/服务日志/启动证据/energy preference/open-files/job 限额/内存压缩/
> 显示器清单/NPU 库存已实现并移入已实现域表。

| 能力 | 需要的 Windows API | 现状 | 未来 Safe 路径 |
|---|---|---|---|
| 每进程网络字节流量 | ETW `Microsoft-Windows-Kernel-Network` | **永久** Unsupported（kernel session 需管理员 + 无 Safe ETW 封装；IP Helper 连接表只给连接不给字节；与 Linux eBPF 移除对称） | 无 |
| 每进程提权换 fd 链 | —（AF_PACKET/SCM_RIGHTS 是 Linux 机制） | **永久** Unsupported（`network.escalation` lane 诚实拒绝） | 无 |
| 能耗/RAPL 等价物 | —（E3 能耗数据仅经 ETW/SRUM，两者都需管理员；无 WMI/perf 类暴露） | **永久** Unsupported（电池放电功率除外，见 power 域） | 无 |
| 每核温度、风扇转速 | 无内核驱动的用户态 MSR/EC 访问 | Unsupported（仅 ACPI 热区聚合温度） | 无（vendor 驱动自带工具） |
| Wi-Fi SSID/信号 | Wlan API（Win11 位置同意门控） | Unsupported（产品决策：不做位置授权换取 SSID） | 无 |
| WSL 每 distro cpu/mem | Windows 侧无公开事实源 | `TemporarilyUnavailable`（distro 清单经 Lxss 注册表真实枚举） | 无 |
| NPU 利用率/每引擎/显存 | 任务管理器背后的 perf counter 集合**未公开**（Microsoft 支持确认名称非 `GPU Engine` 且保密；LibreHardwareMonitor 至今未实现） | typed unavailable（库存已真实；利用率不硬编码任何名字） | 实机运行时发现：`PdhExpandCounterPath` 枚举计数器实例 + SetupAPI `DEVPKEY_Gpu_PhyId`/LUID 匹配归属，命中即缓存；需 native receipt |
| UAC 提权 transport | `ShellExecuteEx("runas")` + `SEE_MASK_NOCLOSEPROCESS`；结果回传 = 临时文件（argv 传路径）或命名管道（跨提权边界） | **不做，等待决策**：机制已定，但未签名 helper 的 UAC 同意框显示"未知发布者"——helper 签名与打包是前置产品决策；`process-control-helper` 的 Windows 代码路径已备好 | 签名/打包决策 + 一个 ADR；不做 Win11 `sudo` 依赖（需系统设置开启且仅 Win11） |
| 修改他人已有 job | 匿名 job 无句柄可得 | 只施加"自有嵌套 job"限额（已实现）；他人 job 不可编辑 | 无 |
| 边界已备、事实无家 | `process_memory_counters`（缺页/工作集/private bytes）、`process_gui_resources`（GDI/USER）、`process_modules`、原生 handle 计数、`query_system_power_status`（AC/节电） | 边界函数已审计存在，core/前端尚无对应事实槽位——有产品需求时接线，不预先造无消费者的事实 | core fact + UI 槽位 |
| first-run setup | — | registered-pending（见上表） | 打包决策 + elevated runner |
| 显示器运行时状态（当前模式/HDR/VRR） | `QueryDisplayConfig` 等（用户态可用） | **主动不做**：core `DisplayRuntimeInfo` 在任何平台都还没有消费 lane（Linux 的 wayland merge 也是 no-op 占位）——按"不预造无消费者的事实"纪律，不造休眠生产者；等消费 lane 立起来后 Windows 与 Linux 同步接 | 先立 runtime display lane（跨平台决策） |

> 注意：NVML 每进程显存在 WDDM 下**设计上**返回 NOT_AVAILABLE（Windows KMD 管理显存），
> 不是封装缺失——Windows 上每进程 GPU 显存/利用率以 PDH `\GPU Process Memory(*)` /
> `\GPU Engine(pid_*)` 为正源（已入 ADR-031 边界），NVML 仅作 NVIDIA 补充。
> 每进程网络字节（上表第一行）与每进程 GPU 显存是两回事，不要合并口径。

## 验收标准

- `cargo check/clippy --target x86_64-pc-windows-msvc` 零警告（`-D warnings`）。
- Windows required native-safety job（`.github/workflows/portability.yml`）在 `windows-latest`
  执行 boundary/adapter 全 targets、workspace lib tests，并上传 `windows-native-receipt`；
  `spawn_composes_the_complete_runtime` 与 pending provider 的 typed Unsupported 断言仍保留。
- Linux 工作区不受影响：适配器在非 Windows 目标同样编译（Windows-only Safe 封装为
  `[target.'cfg(windows)'.dependencies]` + 诚实降级），契约测试 `tests/contract.rs`
  （STANDARD_SURFACE + PENDING_CAPABILITIES 的当前集合以该文件为准）在 Linux CI 每次运行。
- 行为断言不做源码文本断言：headless 断言直接调用 provider/executor。
