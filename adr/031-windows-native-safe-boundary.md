# ADR-031: Minimal Windows native safe boundary

- 状态：已接受
- 相关：`adr/018-windows-telemetry-safety.md`、`docs/PERMISSION_MODEL.md` Boundary 1、
  `tests/logic/workspace_architecture_test/dependency_firewall/frontend_safety.rs`

## 背景

Windows 的 host 刷新曾把主机线程总数放在外部脚本快照里；当脚本不存在、启动慢或输出
解析失败时，可选字段会拖累完整系统快照。Windows 的用户语言也不能只依赖 POSIX 环境
变量。另一方面，直接把 Win32/COM/PDH/WMI 全家桶接入适配器会把原生 ABI 信任根扩成不
可审计的框架层。进程控制还必须防止 PID 复用误杀；会话和启动目录也不能继续依赖文本
命令或工作目录推断。CPU cache/socket 和 NIC link facts 需要少量原生查询，但不值得把
通用 Win32 网络或硬件框架带入产品。

## 决策

新增 `taskmanager-windows-api`，作为第四个、独立且最小的 audited boundary crate。当前
允许八个彼此独立、可逐块审计的调用组：

- `K32GetPerformanceInfo`：主机进程/线程总数；
- `GetUserDefaultLocaleName`：系统用户 locale，供 application/app-host 选择 `en`/`zh`；
- `OpenSCManagerW` / `OpenServiceW` / `ChangeServiceConfigW`：仅修改既有服务的 start
  mode（Automatic/Disabled）。SCM 配置的 binary path、启动参数、依赖、账户和 display
  name 全部通过 `SERVICE_NO_CHANGE` 或空参数保留，不能由适配器重建。
- `SHGetKnownFolderPath`：解析 Roaming AppData、Local AppData 和 Startup；返回值只在边界
  内释放，公开面只出绝对 `PathBuf`。
- `OpenProcess` / `GetProcessTimes` / `TerminateProcess`：读取高精度创建时间，并且只在
  创建时间与冻结身份相等时终止同一拥有的进程句柄；PID 本身永远不是授权凭据。
- `WTSEnumerateSessionsW` / `WTSQuerySessionInformationW` / `WTSLogoffSession`：有界读取
  WTS 会话、用户名和站名，并执行本地会话注销；WTS 分配由 RAII 私有释放。
- `GetLogicalProcessorInformationEx(RelationProcessorPackage/RelationCache)`：有界读取
  socket 数与 L1/L2/L3 cache relationship；只输出聚合 KiB，不输出 group mask 或原始
  记录指针。
- `GetIfTable2` / `FreeMibTable`：有界读取接口 alias、收发链路 bps 和 oper status；表
  指针由 RAII 在边界内释放，接口索引/GUID 不作为公开稳定身份。
- `GetSystemFirmwareTable(RSMB)`：有界读取原始 SMBIOS 表字节流（上限 4 MiB），由 `smbioslib` 安全解析；
- `CreateDXGIFactory1` / `EnumAdapters1` / `IDXGIAdapter3::QueryVideoMemoryInfo`：有界枚举显卡设备与专有/共享显存用量；
- `OpenProcessToken` / `GetTokenInformation(TokenElevation)`：读取进程提权事实；
- `GetPriorityClass` / `SetPriorityClass`：读取并在核验创建时间后设置进程优先级；
- `PeekMessageW` / `TranslateMessage` / `DispatchMessageW`：**系统托盘宿主线程的有界消息
  泵**（ADR-032）。`tray-icon` 的隐藏窗口只有被泵取 `WM_USER_TRAYICON` 回调才能收到
  `Shell_NotifyIcon` 事件；本边界以 `PM_REMOVE` 非阻塞排空、单次上限
  `MAX_PUMPED_MESSAGES_PER_CALL`（64），`WM_QUIT` 只移除不派发，无句柄/指针跨公共 API。
- `CreateMutexW` / `CreateEventW` / `OpenEventW` / `SetEvent` / `WaitForSingleObject` /
  `GetLastError` / `CloseHandle`：**单例互斥量与激活事件**（ADR-032）。命名互斥量
  原子独占（二次 `CreateMutexW` 报 `ERROR_ALREADY_EXISTS`），命名自动复位事件承载
  "激活已有实例"握手；句柄全部由 RAII guard 内部持有，`unsafe impl Send + Sync` 附
  SAFETY 说明（内核对象句柄可跨线程使用、由 guard 保证恰好一次关闭），无句柄/指针跨
  公共 API。

边界根使用 `#![deny(unsafe_op_in_unsafe_fn)]`，每个 `unsafe` 块带 `// SAFETY:` 说明，
公开 API 只返回 typed 值（`SystemPerformance`、`ServiceStartMode`、`KnownFolder`、
`WindowsSession`、`WindowsProcessorTopology`、`WindowsNetworkAdapter`、`WindowsGpuAdapter`、
`ProcessPriorityClass`、路径、创建时间、SMBIOS 原始字节向量）
和 `WindowsApiError`。HANDLE、指针、UTF-16 缓冲以及 Windows 生成绑定类型都不得跨 crate
边界。唯一生产消费者是
`taskmanager-platform-windows`；不得让 GPUI、application 或其它平台直接依赖它。

Windows host lane 只用 `sysinfo` + 该 native query。CPU static facts、NIC metadata 和
其他 telemetry lane 也只能消费上述 typed seam。Windows 生产、测试和开发辅助路径
禁止 PowerShell 或其它命令解释器承担遥测；已有 `sysinfo`、`nvml-wrapper`、
`windows-registry`、`windows-service` 等成熟 safe crate 必须优先复用。没有成熟封装且
没有独立最小 ABI 审计的字段只返回自身的 typed `Unsupported`，不能为了“补齐”而回退到
脚本或伪造零值。

## Current adapter consumption rules

本 ADR 的边界不变，但适配器按以下规则收紧了消费方式：

- Host 进程数直接消费 SystemPerformance.process_count，不再为计数额外全量刷新 sysinfo
  进程表；详细进程表仍由 process-list lane 独立负责。
- End/Kill 不再在精确 native 句柄校验前预扫全量进程表；资源详情改为 PID 定向刷新，随后
  在 Windows 再次核对同一个冻结创建时间 token。任何 PID 缺失或 token 不一致都返回
  IdentityChanged。
- 进程句柄计数按低频 cadence 采样，并且只在创建时间 token 相同的情况下保留上一测量；
  首次/失败测量不投影为真实零值。服务依赖详情使用 windows-service query_config 定向
  查询，不重复枚举 SCM。
- SCM inventory 对单项查询失败和 4096 项上限发布 Partial，不把“全部不可读”或“被截断”
  伪装成 Empty/Available；Startup registry/folder 同样有条目、文本和总量上限。
- Known Folder、APPDATA/LOCALAPPDATA 都不可用时，持久配置/历史先尝试绝对的
  USERPROFILE AppData Roaming/Local，最后才允许显式 ephemeral temp fallback。
- CPU provider 复用 `sysinfo` 的 live frequency，并使用 `raw-cpuid` 的 safe API 读取 base/max；
  `GetLogicalProcessorInformationEx` 只在 provider 构造/硬件刷新边界产生 socket/cache
  facts。网络 provider 复用 `sysinfo` 的地址/operational state，并用 `GetIfTable2` 补 link
  speed；任何查询失败都保留 typed failure。

这些改动不扩大 unsafe 根；它们只减少重复扫描、保留真实失败语义，并把安全 wrapper 的
输出继续限制在 typed 值和 bounded collection 内。

所有未来原生 wrapper 还必须满足：公开 API 只出 typed 值/错误；句柄、指针、编码缓冲只
在边界内部拥有；任何 OS 返回长度先做符号、上界和转换检查再切片；分配、枚举、等待和
输出均有上限；每个 `unsafe` 块有逐块 `SAFETY:` 证明；生产路径无 panic。不能证明这些
不变量的候选 API 留在 typed `Unsupported`。

## 不做什么

本 ADR 不批准通用 Win32 FFI、COM、PDH、WMI、ETW、进程亲和/挂起/资源控制或 UAC helper。
本次新增的 topology/adapter-information 调用组是固定的 bounded exceptions，不构成通用
counter、WMI 或 adapter framework 的授权。
已登记的精确 End/Kill 只允许使用创建时间二次核验；不能借此扩展为任意进程控制。SCM
start mode 仍只能做上述 partial update；其它服务配置字段找不到成熟 safe crate 时保持
typed `Unsupported`。只有另一个可独立审计的最小 ABI 才能提交新的 ADR、反向防火墙和
对应回归门禁。

### 候选 API：MCDM NPU 枚举

`accelerator.npu` Windows 侧评估结论：任务管理器级 NPU 视图依赖
MCDM（Microsoft Compute Driver Model）适配器枚举 + D3D12/DirectML 引擎统计，现无成熟
safe crate；原生路径需 COM（`DXGIFactory` 扩展或 WDDM scheduler 接口），落入本 ADR
"不批准通用 COM"的禁区。处置：capability 以 `windows.accelerator.npu` 注册 pending
provider（typed `Unsupported`，目录描述符诚实存在），MCDM 保留为本清单候选——仅当
未来出现可审计的最小 COM 子面（镜像 adapter-information 先例）时，按新 ADR 准入流程
立项；在此之前不伪造任何利用率或设备行。

## 验收约束

1. `taskmanager-windows-api` 零 workspace 依赖；Windows adapter 是唯一生产消费者。
2. 架构门禁将其加入 unsafe allowlist、panic scan、safe-seam scan 和 reverse firewall。
3. Linux 目标返回 typed `Unsupported`，Windows 目标通过 `cargo check`；真实 Windows
   数字（含 CPU topology/cache、link speed）、配置恢复、Known Folder、WTS、进程权限和启动项权限必须由 required Windows
   native-safety receipt 执行，未取得 runner 回执前仍保持 on-box-unverified。
4. `tests/logic/workspace_architecture_test/dependency_firewall/frontend_safety.rs` 扫描
   Windows adapter 全部 Rust 源码，禁止重新出现命令解释器遥测 token；这条规则覆盖测试
   和开发辅助代码，不只是生产模块。
