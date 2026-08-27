# 跨平台信息采集与数据源全景清单 (Telemetry Manifest)

> **定位**：全仓唯一权威的跨平台底层数据源清单（Single Source of Truth）。
> **约束**：业务层保持 `#![forbid(unsafe_code)]`；原生 ABI 仅允许存在于受 ADR 独立审计的边界 crate 中；坚持 100% 诚实遥测，不支持即为 `Unsupported`，缺失即为 `Unavailable`，严禁伪造零值或推断虚假状态。

---

## 1. 核心设计与数据采集原则

1. **三平台对等契约 (Uniform Contract)**：所有平台适配器均实现统一的 `taskmanager-platform-contract` 与 `taskmanager-platform-provider` 特征集，产出 toolkit-neutral 的 Core DTO 模型。
2. **安全分级路径 (Safety Hierarchy)**：
   - 首选 **(A) 社区成熟且已审计的 Safe Rust 库**（如 `sysinfo`、`raw-cpuid`、`starship-battery`、`smbioslib`、`notify-rust`、`windows-registry`、`windows-service`）；
   - 必备极小原生 ABI 走 **(B) 独立最小审计边界 crate**（Linux 的 `perf-ioctl`、`afpacket`、`fd-bridge`；Windows 的 `taskmanager-windows-api`；严格做到 `#![deny(unsafe_op_in_unsafe_fn)]` 与 `// SAFETY:` 逐块注释，零 handle/pointer 穿透）；
   - 固定的辅助兼容工具走 **(C) 有界受限 Shell-out**（仅限 `smartctl`、`explorer /select` 等，固定参数、硬超时、stdout/stderr 单流与两流合计均为 4 MiB 上限并保证 kill/wait/reader 回收，**禁止作为遥测核心路径**，Windows 生产与测试严格禁用 PowerShell/CMD 解释器进行遥测）；
   - 无合格 Safe 来源或涉及不可控全局框架的指标，如实返回 **`Unsupported` / `Unavailable`**。
3. **零伪造与显式降级 (Zero Fabrication & Honest Fallback)**：
   - 硬件供应商扩展（如 NVIDIA NVML）采用动态加载；当 NVML 缺失时，Windows 自动降级为 DXGI 1.4 原生显存与共享内存采集，Linux 降级为 sysfs/drm；
   - 绝不使用"空字符串"、"0% 占用"或"0 MB 显存"掩盖未支持或未采集到的指标。

---

## 2. 跨平台信息采集全景对照表

### 2.1 主机、系统与硬件资产 (Host & Hardware Inventory)

Linux 软件包版本查询按发行版选择有界来源：pacman、dpkg、apk 使用本地元数据；RPM
系列系统使用固定参数、只读且有界的 rpm 查询。软件包数据库或工具缺失时，该字段保持
`None`，绝不猜测桌面版本。

| 遥测能力 / 指标项 | Linux 采集源与机制 | Windows 采集源与机制 | macOS 采集源与机制 | 权限与安全边界 |
|---|---|---|---|---|
| **主机身份 (Host Identity)**<br>• 主机名、系统版本、内核版本、开机运行时间 (Uptime)、活动桌面名/版本、Shell/终端/locale、init、包管理器与已安装包数、窗口管理器/版本、合成器后端 | • `/proc/sys/kernel/hostname`<br>• `/etc/os-release`<br>• `/proc/version`<br>• `sysinfo::System`<br>• `XDG_CURRENT_DESKTOP` / `XDG_SESSION_DESKTOP`、`SHELL`、`TERM*`、`LC_ALL`/`LANG`（仅读取会话环境，不启动 shell）<br>• PID 1 `/proc/1/comm`<br>• pacman/dpkg/apk 本地数据库（有界只读计数与 Plasma/KWin 版本）<br>• `sysinfo` 进程表 + Wayland KDE 输出 registry（KWin/Wayland） | • `sysinfo::System::long_os_version()`<br>• `sysinfo::System::kernel_version()`<br>• `sysinfo::System::uptime()` | • `sysinfo::System::long_os_version()`<br>• `sysinfo::System::kernel_version()`<br>• `sysinfo::System::uptime()` | **User** (Safe Rust；包数据库/会话协议不可读时保持字段 `None`) |
| **系统固件与主板 (Firmware / Motherboard)**<br>• BIOS 厂商、BIOS 版本、主板/产品型号、版本 | • `/sys/class/dmi/id/bios_vendor`<br>• `/sys/class/dmi/id/bios_version`<br>• `/sys/class/dmi/id/product_name` | • `taskmanager-windows-api::raw_smbios_table()` (`GetSystemFirmwareTable(RSMB)`)<br>• `smbioslib` (Type 0 BIOS & Type 1 System 解析) | • `system_profiler SPHardwareDataType`<br>• 缺失时返回 `None` | **User** (Windows 走 ADR-031 边界双探测有界缓冲，4 MiB 保护) |
| **主机全局运行线程总数** | • `/proc/loadavg` (第 4 字段线程总数) | • `taskmanager-windows-api` (`K32GetPerformanceInfo` -> `SystemPerformance.thread_count`) | • `sysinfo` / `host_info(HOST_VM_INFO)` | **User** (Windows 走 ADR-031 最小性能查询) |
| **显示器静态硬件详情**<br>• 连接器、厂商、型号、序列号、物理尺寸、首选分辨率/刷新率、HDR 能力 | • `/sys/class/drm/card*-*/status` + `edid`（身份、首选 timing、CTA HDR Static Metadata；只读）<br>• 不读取 compositor 当前状态进入硬件详情 | • 原生静态显示 provider 待接入，缺失保持 `Unsupported` | • 原生静态显示 provider 待接入，缺失保持 `Unsupported` | **User** (静态 inventory；缺失字段保持 `None`) |

当前 mode、实时刷新率、HDR 开关、VRR 策略和 compositor 能力属于独立的动态显示遥测；在性能投影或专用 display-runtime capability 接入前，不进入 `HardwareInfo`。

---

### 2.2 CPU 与计算拓扑 (CPU & Compute Topology)

| 遥测能力 / 指标项 | Linux 采集源与机制 | Windows 采集源与机制 | macOS 采集源与机制 | 权限与安全边界 |
|---|---|---|---|---|
| **CPU 全局与每核占用率** | • `/proc/stat` (CPU 时间片差值计算) | • `sysinfo::System::cpus()` (`GetSystemTimes` 差值) | • `sysinfo::System::cpus()` (`host_cpu_load_info`) | **User** (Safe Rust；共享 core gate 拒绝非有限、负数与 `>=100.5%` 的幻觉值，`100..100.5` 仅饱和到 `100%`) |
| **实时频率 (Live Frequency)** | • `/sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq` | • PDH `% Processor Performance` × 每核基频（Task Manager 算法，基频来自 `PROCESSOR_POWER_INFORMATION.MaxMhz`）<br>• 降级：`Processor Frequency` 计数器 / `CallNtPowerInformation.CurrentMhz` (sysinfo) | • `sysinfo::Cpu::frequency()` (或标量 `Unavailable`) | **User** (Safe Rust) |
| **标称基频与睿频上限 (Base / Max Freq)** | • cpufreq policy `base_frequency`（取可见核心类型的最高静态基频）；缺失时才回退 CPUID 0x16 | • 基频：`PROCESSOR_POWER_INFORMATION.MaxMhz`（按核心类型取最高）；缺失时回退 CPUID 0x16<br>• 睿频上限：CPUID 0x16 → SMBIOS Type 4 `Max Speed`（CPUID 0x16 为 0 的混合 CPU 必需） | • `sysctl hw.cpufrequency` | **User** (Safe Rust) |
| **CPU 架构、型号与物理/逻辑核数** | • `/proc/cpuinfo`<br>• `/sys/devices/system/cpu/topology/*` | • `sysinfo::Cpu::brand()`<br>• 逻辑核/物理核由 `sysinfo` 识别 | • `sysinfo::Cpu::brand()`<br>• `sysctl hw.physicalcpu/logicalcpu` | **User** (Safe Rust) |
| **CPU 插槽数 (Sockets) 与 L1/L2/L3 缓存** | • `/sys/devices/system/cpu/cpu*/cache/index*/*` | • `taskmanager-windows-api::processor_topology()` (`GetLogicalProcessorInformationEx(RelationProcessorPackage/RelationCache)`) | • `sysctl hw.l1icachesize / hw.l2cachesize / hw.l3cachesize` | **User** (Windows 走 ADR-031 原生拓扑解析) |
| **CPU 温度与封装功耗 (RAPL)** | • `/sys/class/hwmon/hwmon*` 温度按 `coretemp`/`k10temp`/`zenpower` 精确芯片优先；其后只接受带 `Tctl`/`Tdie`/`Package`/`APU`/`CPU` 语义且排除 `edge`/`junction`/`mem`/`vrm` 的 labeled hwmon；最后才用 ACPI thermal zone，并以 `CpuTemperatureSource` 保留来源<br>• `perf_event_open` (RAPL energy-pkg，经 `taskmanager-perf-ioctl` ADR-022) | • 温度/功耗暂无无侵入 Safe API，保持 typed `Unsupported`（绝不拉起管理员 WMI/OHM 后台服务） | • `powermetrics` (经提权 helper) 或标量 `Unavailable` | **Linux**: 温度来源按 tier 诚实降级；RAPL 需 CAP_PERFMON 或特权 Helper；**Windows/macOS**: 诚实缺口 |

---

### 2.3 内存与交换分区 (Memory & Swap)

| 遥测能力 / 指标项 | Linux 采集源与机制 | Windows 采集源与机制 | macOS 采集源与机制 | 权限与安全边界 |
|---|---|---|---|---|
| **物理内存总量、已用、可用 (Total / Used / Available)** | • `/proc/meminfo` (`MemTotal`, `MemFree`, `MemAvailable`, `Buffers`, `Cached`, `SReclaimable`, `Zfs`) | • `sysinfo::System::total_memory()`<br>• `sysinfo::System::used_memory()`<br>• `sysinfo::System::available_memory()` (`GlobalMemoryStatusEx`) | • `sysinfo::System`<br>• `vm_stat` (wire/active/inactive/free/compressed) | **User** (Safe Rust；ZFS ARC 缺失时保持 typed absence，不改写为零) |
| **交换分区 (Swap / Pagefile)** | • `/proc/meminfo` (`SwapTotal`, `SwapFree`) | • `sysinfo::System::total_swap()`<br>• `sysinfo::System::used_swap()` | • `sysctl vm.swapusage` | **User** (Safe Rust) |
| **ZFS ARC 与压缩交换细节** | • `/proc/meminfo` `Zfs` 作为可回收 ARC 层<br>• `/sys/block/zram*/mm_stat` 的 `orig_data_size`、`compr_data_size`、`mem_used_total`（逐 zram 汇总）<br>• `/proc/swaps` zram used 与 `/sys/module/zswap/parameters/enabled` | • 无 Linux ZFS/zram 对等来源；保持对应 typed facts 缺省 | • 无 Linux ZFS/zram 对等来源；保持对应 typed facts 缺省 | **User** (无 ZFS/zram 为 Empty/Unavailable；缺少旧内核 `mm_stat` 不伪造零；压缩比仅在两端事实同时 current 时推导) |

zram 深度事实的三条出口使用同一 typed 真值（`optional_observations.compression`）：
swap 读出（`/proc/swaps` used 口径）、压缩深度（orig→compr + 守卫压缩比）、以及 store 实占
内存（`mem_used_total`，含元数据）。`mem_used_total` 在导出 JSON 中固定为
`optional_observations.compression.compressed_swap_memory_used_bytes`，并由 core 导出行为测试
钉名（不可用时绝不伪报 present 零值）；GPUI/Iced/TUI 的内存读出与 swap 标签均以
`mem.zram_ram_used` 标签独立展示该事实，`/proc/swaps` 口径与 `mm_stat` 口径不得互相冒充。
| **内存增长/释放压力速率 (Pressure Rate)** | • `taskmanager-platform-*` 内置时序环样本差值计算 (MiB/s) | • `taskmanager-platform-windows` 内置时序环样本差值计算 (MiB/s) | • 时序环样本差值计算 | **User** (纯算法推导，时钟倒流时标记 `IdentityChange`) |

---

### 2.4 磁盘、文件系统与目录用量 (Storage & Filesystem)

| 遥测能力 / 指标项 | Linux 采集源与机制 | Windows 采集源与机制 | macOS 采集源与机制 | 权限与安全边界 |
|---|---|---|---|---|
| **挂载点、卷容量、文件系统类型、只读属性** | • `/proc/mounts`<br>• `statvfs`<br>• `/sys/block/*` | • `sysinfo::Disks::new_with_refreshed_list()` (`GetDiskFreeSpaceExW`, `GetVolumeInformationW`) | • `sysinfo::Disks` (`statfs`) | **User** (Safe Rust) |
| **磁盘实时读写速率 (Throughput Read / Write)** | • `/proc/diskstats` 扇区读取量差值 / 采样周期 | • 长生命周期 `sysinfo::Disks::usage()` 动态 delta 计算 | • `iostat -d -c 2` 差值采样 | **User** (Safe Rust，未获取到有效基线前不伪造 0 速率) |
| **磁盘身份细节 (Serial / Revision)** | • `/sys/block/<dev>/device/serial`<br>• `/sys/block/<dev>/device/firmware_rev`、`rev` 或 `revision` | • 由原生磁盘属性 provider 提供；缺失保持 `None` | • 由原生磁盘属性 provider 提供；缺失保持 `None` | **User** (Safe Rust；缺失不变成空字符串) |
| **有界目录用量扫描 (`filesystem.directory.usage`)** | • 共享纯 Safe `std::fs` `DirectoryUsageScanner` (递归深度/文件数/耗时多重硬上限保护，符号链接只计数不穿透) | • 共享纯 Safe `std::fs` `DirectoryUsageScanner` (同左) | • 共享纯 Safe `std::fs` `DirectoryUsageScanner` (同左) | **User** (Safe Rust，Chunk 粒度响应取消) |
| **磁盘健康与 SMART 自检** | • `smartctl -a -j` (NVMe / ATA 结构化 JSON 解析)<br>• `smartctl -t <kind>` | • NVMe Health Log Page 经 `taskmanager-windows-api` IOCTL (`IOCTL_STORAGE_QUERY_PROPERTY` / `IOCTL_STORAGE_PREDICT_FAILURE`，ADR-031)<br>• 自检经受限有界 `smartctl`（超时 3~5s，输出上限 4 MiB）<br>• 查询失败/无管理员时 availability 非 `Available`，UI 整块隐藏 | • `smartctl` (经受限 Command 调用)<br>• 缺失时返回 `MissingDependency` | **User / Admin** (IOCTL 用户态；`smartctl` 为外部固定工具，无脚本，零注入风险) |

---

### 2.5 网络接口与流量 (Network Interfaces)

| 遥测能力 / 指标项 | Linux 采集源与机制 | Windows 采集源与机制 | macOS 采集源与机制 | 权限与安全边界 |
|---|---|---|---|---|
| **接口清单、MAC 地址、IPv4/IPv6、运行状态** | • `/proc/net/dev`<br>• `/sys/class/net/*`<br>• `getifaddrs` / `sysinfo::Networks` | • `sysinfo::Networks` (`GetAdaptersAddresses`) | • `sysinfo::Networks` (`getifaddrs`) | **User** (Safe Rust) |
| **实时网络上下行速率与吞吐量** | • `/proc/net/dev` 字节计数时序差值 | • `sysinfo::NetworkData` 接收/发送字节时序差值 | • `sysinfo::NetworkData` 接收/发送字节差值 | **User** (Safe Rust) |
| **物理链路协商速度 (Link Speed)** | • `/sys/class/net/<iface>/speed` (Mbps) | • `taskmanager-windows-api::enumerate_network_adapters()` (`GetIfTable2` -> `TransmitLinkSpeed` / `ReceiveLinkSpeed`) | • `ifconfig <iface>` (media 行解析) 或返回 `None` | **User** (Windows 走 ADR-031 边界 RAII 释放 MIB 表) |
| **Wi‑Fi 关联细节**<br>• BSSID、频率/信道、RX/TX 协商速率、802.11 模式 | • 有界无 shell 的 `iw dev <iface> link/info`；只读并解析确认的 `Connected to`、`freq/channel`、`rx/tx bitrate` 和 EHT/HE/VHT/HT 模式；字段独立 typed availability | • WLAN 原生 adapter API（未接入前保持 `Unsupported`） | • CoreWLAN 原生 API（未接入前保持 `Unsupported`） | **User** (Safe Rust；命令固定参数、有界超时，不启动解释器) |

---

### 2.6 GPU 与图形显存 (GPU & Video Memory)

| 遥测能力 / 指标项 | Linux 采集源与机制 | Windows 采集源与机制 | macOS 采集源与机制 | 权限与安全边界 |
|---|---|---|---|---|
| **NVIDIA 独显完整遥测**<br>• 利用率、核心频率、显存总量/用量、温度、风扇、功耗、驱动版本 | • `nvml-wrapper` (动态加载 `libnvidia-ml.so`) | • `nvml-wrapper` (动态加载 `nvml.dll`) | • macOS 现代架构不支持 NVIDIA 显卡 (typed `Unsupported`) | **User** (动态加载，未安装驱动时平滑降级) |
| **PCI 图形设备原始身份与 marketing SKU** | • `/sys/class/drm/card*/device/{vendor,device,subsystem_*,modalias}`、PCI slot、driver；有界只读 `pci.ids` 将 vendor/device 映射为 marketing name（例如 Arc B390），未命中不猜 | • DXGI adapter LUID/描述由 Windows 原生 provider 提供 | • IOKit/系统显示 provider 待接入 | **User** (Safe Rust；原始 ID 与 marketing name 分字段，数据库缺失保持 `None`) |
| **运行时图形 API 版本**<br>• OpenGL、Vulkan physical-device API version | • 可选固定 argv 的 `glxinfo -B` / `vulkaninfo --summary`，每个进程仅有界探测一次；只有唯一可见 DRM GPU 时才绑定到该 GPU，输出只保留版本 token | • 当前保持 typed 缺省；DXGI 不等价于 OpenGL/Vulkan capability，待原生 loader/query seam | • 当前保持 typed 缺省；Metal 不等价于 OpenGL/Vulkan capability | **User** (工具缺失、无显示上下文、超时、解析失败或多 GPU 无法精确归属均省略；不从 driver 名推断) |
| **AMD / Intel / 集成显卡通用显存与设备识别** | • `/sys/class/drm/card*` (`device/vendor`, `device/device`, `mem_info_vram_total/used`, `mem_info_gtt_total/used`) | • **DXGI 1.4 原生降级**：`taskmanager-windows-api::enumerate_gpu_adapters()` (`CreateDXGIFactory1` + `IDXGIAdapter3::QueryVideoMemoryInfo`)<br>• **共享显存实时用量走 PDH** `\GPU Adapter Memory(*)\Shared Usage`（WDDM 2.0+，与任务管理器同源；DXGI NON_LOCAL 在 Intel/AMD 驱动上不可靠时不再伪造 0，未观测保持缺省）<br>• 获取专用显存/共享显存总容量与实时用量 | • `system_profiler SPDisplaysDataType` (获取显卡名称与 VRAM 容量；命令/JSON 失败为 typed unavailable，只有成功返回空数组才是 `Empty`) | **User** (Windows 走 ADR-031 极简 COM 接口封装，纯 Safe 结构体输出) |
| **GPU 引擎细分利用率 (3D / Copy / Video)** | • `/sys/class/drm/card*/engine/*` (Intel/AMD PMU) | • `taskmanager-windows-api::query_gpu_engine_utilization()`（PDH `GPU Engine`，只按 DXGI adapter LUID 精确归属；未匹配保持 unavailable，不复制 sibling、不伪造 0） | • typed `Unsupported` | **Linux/Windows**: 部分支持；**Mac**: 诚实缺口 |
| **NPU/AI 加速器设备发现与利用率**<br>(capability `accelerator.npu`) | • `/sys/class/accel/*` (Linux 6.3+ DRM Accel：设备 id、绑定驱动；利用率/内存 typed `Unsupported` 直至内核接口稳定) | • registered-pending `windows.accelerator.npu`：typed `Unsupported`（MCDM 原生接口尚无足够小的可审计 safe 边界） | • registered-pending `macos.accelerator.npu`：typed `Unsupported`（ANE 需 powermetrics/IORegistry 缝，未接入） | **User**（sysfs 只读；空设备列表=诚实无 NPU，非失败） |
| **CPU 指令集特性向量**<br>(`CpuInstructionFeature`，canonical ALL 枚举) | • `/proc/cpuinfo` `flags:`/`Features:` 行（单映射表，含 avx_vnni/amx_*/sve） | • `raw-cpuid` safe 特性叶子（SSE4.x/AVX2/AVX-512F/VNNI/AMX/SHA 等，canonical 序输出） | • 有界 `sysctl -n hw.optional.*` 子进程（仅 4 键映射，无键=不猜） | **User**（无特权读；未报告的特性不出现，绝不猜测） |

---

### 2.7 进程管理与深度洞察 (Process & Process Insights)

| 遥测能力 / 指标项 | Linux 采集源与机制 | Windows 采集源与机制 | macOS 采集源与机制 | 权限与安全边界 |
|---|---|---|---|---|
| **进程清单与基础指标**<br>• PID、PPID、进程名、命令行、状态、CPU 占用、内存用量、启动时间 | • `/proc/[pid]/stat`<br>• `/proc/[pid]/cmdline`<br>• `/proc/[pid]/statm` | • `sysinfo::System::processes()` (`SystemProcessInformation` / `NtQuerySystemInformation`) | • `sysinfo::System::processes()` (`proc_pidinfo` / `sysctl`) | **User** (Safe Rust) |
| **进程高精度创建时间与防 PID 复用令牌** | • `/proc/[pid]/stat` `starttime` (jiffies) | • `taskmanager-windows-api::process_creation_time_100ns(pid)` (`GetProcessTimes` 100ns 粒度内核时间戳) | • 当前 safe adapter 仅有 `sysinfo` 秒级清单时间，不具备精确授权 token；target read/control/reveal 因此 typed `Unsupported`，直至最小 `proc_pidinfo` 边界落地 | **User** (任何 target read/write 均先后复核 provider-issued exact token；拿不到即 fail closed) |
| **进程磁盘 I/O 速率** | • `/proc/[pid]/io` (`read_bytes`, `write_bytes` 差值) | • `sysinfo::Process::disk_usage()` delta 差值计算 | • `sysinfo::Process::disk_usage()` 累计计数差值；首样本/gap/回滚保持 typed unavailable | **User** (稳定身份绑定 baseline，首样本不伪造 0) |
| **进程优先级 / nice 值** | • `/proc/[pid]/stat` (nice 字段) | • `taskmanager-windows-api::process_priority(pid)` (`GetPriorityClass` 映射为标准 nice 区间) | • `getpriority(PRIO_PROCESS, pid)` | **User** (Windows 走 ADR-031 映射) |
| **进程管理员提权状态 (Is Elevated)** | • `/proc/[pid]/status` (`Uid` / `CapEff`) | • `taskmanager-windows-api::process_is_elevated(pid)` (`OpenProcessToken` + `GetTokenInformation(TokenElevation)`) | • `proc_pidinfo` (`pbi_uid == 0`) | **User** (Windows 走 ADR-031 Token 查询) |
| **进程级显存用量 (Process VRAM)** | • `nvml-wrapper` (NVIDIA 进程级显存表) | • `nvml-wrapper` (NVIDIA 进程级显存表，WDDM 模式下驱动限制时返回 `NOT_AVAILABLE`) | • typed `Unsupported` | **User** (Safe Rust) |
| **进程打开句柄数 (Handle / FD Count)** | • `/proc/[pid]/fd` 目录条目计数 | • `sysinfo::Process::open_files()` (返回 Windows Open Handles 计数) | • `proc_pidinfo` (`PROC_PIDLISTFDS`) | **User** (按低频 cadence 刷新以避免 CPU 尖峰) |
| **进程线程清单 (`threads`)** | • `/proc/[pid]/task/*` | • `taskmanager-windows-api::process_threads(pid)` (`CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD)` 遍历并按进程过滤) | • `proc_pidinfo` (`PROC_PIDLISTTHREADS`) | **User** (Windows 走 ADR-031 RAII 句柄快照，设置 4096 条目上限防内存膨胀) |
| **进程 CPU 亲和性掩码 (`process.affinity`)** | • `sched_getaffinity` | • `taskmanager-windows-api::process_affinity(pid)` (`GetProcessAffinityMask` 映射为核心列表) | • typed `Unsupported` (macOS 内核无用户级硬亲和性绑定) | **User** (Windows 走 ADR-031) |
| **进程打开文件详情清单 (`open_files`)** | • `/proc/[pid]/fd/*` readlink | • 注册为 Pending `Unsupported` (sysinfo 无全量映射，`NtQuerySystemInformation` 框架过大) | • 注册为 Pending `Unsupported` | **Linux**: 读 `/proc`；**Win/Mac**: 待后续轻量边界评估 |
| **进程网络实时连接与流量** | • `taskmanager-afpacket` (AF_PACKET 嗅探，经 ADR-024 边界) + `taskmanager-fd-bridge` (SCM_RIGHTS ADR-025) | • IP Helper tables 提供按 PID 的连接清单，并以 exact creation token 前后复核；每进程 rx/tx 流量仍 typed `Unsupported`（不用 ETW 猜值） | • typed `Unsupported` (需系统扩展与 NetworkExtension 签名) | **Linux**: 特权 byte accounting；**Windows**: direct connections / unsupported traffic；**Mac**: 诚实排除 |

---

### 2.8 进程控制与信号操作 (Process Control & Batch Execution)

| 控制能力 / 动作 | Linux 实施机制 | Windows 实施机制 | macOS 实施机制 | 安全与防误杀机制 |
|---|---|---|---|---|
| **结束进程 (End Task / Kill)** | • `kill(pid, SIGTERM / SIGKILL)` (操作前比对 `/proc/[pid]` 启动时间) | • `taskmanager-windows-api::terminate_process_exact(pid, expected_start_time)` (`OpenProcess` + 核验 `GetProcessTimes` + `TerminateProcess`) | • typed `Unsupported`，直至有 safe precise-token + same-handle boundary | **PID-Reuse Guard**：绝不凭裸 PID 授权破坏性操作，必须在同一个已打开句柄上核验创建时间一致性 |
| **调整进程优先级 (Set Priority / Renice)** | • `setpriority(PRIO_PROCESS, pid, nice)` (必要时走 pkexec helper) | • `taskmanager-windows-api::set_process_priority_exact(pid, expected_start_time, priority)` (`SetPriorityClass`) | • typed `Unsupported`，不以秒级时间授权 `renice` | **精确句柄核验** + 合理映射 (Idle, BelowNormal, Normal, AboveNormal, High, Realtime) |
| **设置 CPU 亲和性 (Set Affinity)** | • `sched_setaffinity` | • `taskmanager-windows-api::set_process_affinity_exact(pid, expected_start_time, cpus)` (`SetProcessAffinityMask`) | • typed `Unsupported` | **PID-Reuse Guard** + 核心位图转换与校验 (不允许空掩码) |
| **挂起 / 恢复进程 (Suspend / Resume)** | • `kill(pid, SIGSTOP / SIGCONT)` | • 保持 typed `Unsupported` (`NtSuspendProcess` 为未公开 NTAPI，暂无独立最小审计) | • typed `Unsupported`（同一 exact-token 缺口） | 平台一致性保护 |

---

### 2.9 系统服务与启动项 (Services & Startup)

| 遥测能力 / 管理项 | Linux 采集源与机制 | Windows 采集源与机制 | macOS 采集源与机制 | 权限与安全边界 |
|---|---|---|---|---|
| **系统服务清单与实时状态** | • `systemd` D-Bus 接口 / OpenRC `rc-status` (按系统运行时自动选择) | • `windows-service` crate (原生连接 SCM `OpenSCManagerW` + `EnumServicesStatusExW`) | • `launchctl list` (受限有界 Command 调用) | **User** (Safe Rust，缓存 5s 防止高频刷 SCM) |
| **服务启动类型修改 (Start Mode)** | • `systemctl enable / disable` | • `taskmanager-windows-api` (`OpenServiceW` + `ChangeServiceConfigW` 仅修改 StartType，保留其余原有配置字段) | • `launchctl load / unload -w` | **Admin / SCM** (Windows 走 ADR-031 partial update 保护) |
| **服务日志快照与实时流** | • `journalctl` 有界读取 / 流式读取 | • 快照保持 typed `Unsupported` / 事件日志接入；实时流保持 `Unsupported` | • `log show --predicate` 有界读取 | **User** (Safe / Bounded) |
| **自启动项清单 (Startup Inventory)** | • XDG Autostart 目录 (`~/.config/autostart`, `/etc/xdg/autostart`) | • `windows-registry` 扫描 `HKCU/HKLM` 的 `...\\Run`、`...\\RunOnce` 与 Known Folder Startup 目录 | • `LaunchAgents` 目录 (`~/Library/LaunchAgents`, `/Library/LaunchAgents`) | **User** (Safe Rust) |
| **自启动项启用/禁用控制** | • 写入 `.desktop` 文件的 `Hidden=true` 或 `X-GNOME-Autostart-enabled` | • `windows-registry` 精确更新 `StartupApproved\\Run` 二进制标志字节（不删注册表项与原文件） | • 修改 plist 文件中的 `Disabled` 键 | **User** (Windows 状态字节防破坏性修改) |

---

### 2.10 系统会话、电源与桌面集成 (Sessions, Power & Desktop Integration)

| 遥测能力 / 集成项 | Linux 采集源与机制 | Windows 采集源与机制 | macOS 采集源与机制 | 权限与安全边界 |
|---|---|---|---|---|
| **登录用户与会话清单** | • `systemd-logind` D-Bus / `utmpx` | • `taskmanager-windows-api::enumerate_sessions()` (`WTSEnumerateSessionsW` + `WTSQuerySessionInformationW`) | • `utmpx` (`getutxent`) / `who` | **User** (Windows 走 ADR-031 RAII 内存释放) |
| **注销会话 (Logoff Session)** | • `logind` `TerminateSession` | • `taskmanager-windows-api::logoff_session(session_id)` (`WTSLogoffSession`) | • `pkill -u <user>` | **User / Admin** (ADR-031 边界保护) |
| **电池与电源供应状态、健康与续航估计** | • `/sys/class/power_supply/*`：`energy_full`/`energy_full_design`（无 energy 节点时以 charge × voltage 明确降级）、`time_to_empty_now`/`time_to_full_now`，status gate 只在 Discharging/Charging 时接受对应估计<br>• `starship-battery` crate 作为 portable 路径 | • `starship-battery` crate (`GetSystemPowerStatus`) 的 energy-full/design 与 native time estimates；未报告的值为 typed `Unsupported` | • `starship-battery` crate (`IOPMPowerSource`) 的 energy-full/design 与 native time estimates；未报告的值为 typed `Unsupported` | **User** (Safe Rust；health = full/design 的纯比例，估计缺失/状态不适用绝不显示零或错误的另一侧) |
| **桌面深浅色与高对比度外观** | • XDG Desktop Portal `org.freedesktop.portal.Settings` (D-Bus) | • `windows-registry` 查询 `Personalize\\AppsUseLightTheme` 与 `Accessibility\\HighContrastOn` | • `defaults read -g AppleInterfaceStyle` | **User** (Safe Rust) |
| **桌面 Toast 告警通知 (`alerts.notify`)** | • `notify-rust` (通过 freedesktop D-Bus 接口通知系统通知中心) | • `notify-rust` (调用 Windows 原生 WinRT Toast 通知) | • `notify-rust` (`NSUserNotification` / `osascript`) | **User** (Safe Rust) |
| **资源管理器文件定位 (`shell.resource.reveal`)** | • D-Bus `org.freedesktop.FileManager1.ShowItems`<br>• 降级为 `xdg-open` | • exact creation token 校验后 `explorer.exe /select,<path>`（路径只是 payload，不是身份） | • typed `Unsupported`，直至可先验证 exact process identity | **User** (target identity + 参数校验；拒绝必须先于桌面副作用) |

---

## 3. 能力状态

本清单只定义数据源、权限和安全边界，不保存易漂移的覆盖比例、测试数量或现场回执。
能力是否已注册、可用、部分可用或 `Unsupported`，以 platform contract/provider catalog、
对应 crate README 和实现验证共同判定；本清单不承载现场数据、评分或待办事项。

---

## 4. 维护与更新规范

1. **新增数据源**：若未来引入新的数据源或提升某一平台能力的实现度，必须同步在本清单中登记底层机制、安全等级与 ADR 关联。
2. **拒绝退化**：任何平台适配器不得以牺牲安全边界（如在非审计 crate 引入 `unsafe`，或拉起临时 PowerShell/Shell 解释器）为代价虚增采集项。
