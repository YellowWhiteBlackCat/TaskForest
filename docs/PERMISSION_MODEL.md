# 权限与信任边界总纲

每项能力必须同时回答三个问题：代码是否 safe、默认是否无特权、拒绝后用户看见什么。
本文件只保留当前规则；实现细节在对应 crate README。历史审计与主机回执属于私有发布材料。

## Boundary 1 — Safe Rust

业务与产品 crate 使用 `#![forbid(unsafe_code)]`。`unsafe` 仅允许存在于四个最小审计边界：

- `taskmanager-perf-ioctl`：`perf_event_open`；
- `taskmanager-afpacket`：AF_PACKET；
- `taskmanager-fd-bridge`：SCM_RIGHTS；
- `taskmanager-windows-api`：最小 Windows performance/locale/Known Folder/exact-process/WTS/SCM、
  topology/cache/NIC metadata API。

边界内部拥有句柄、指针、缓冲和长度校验；公共 API 只返回 typed 值、owned fd 或 typed
error，逐个 `unsafe` 块必须有 `SAFETY` 证明和 `unsafe_op_in_unsafe_fn` 约束。eBPF 不属于
当前生产信任根。

**遍历守卫与双执行面纪律**：libc/OS 辅助遍历宏按其**最弱保证**审计——`CMSG_NXTHDR`/`__cmsg_nxthdr` 只
保证下一个 cmsg 头的 16 字节落在报告窗口内，不保证 cmsg 声明的载荷适配窗口。遍历循环里
每项长度守卫必须**相对当前元素自身偏移**（`声明长度 ≤ 窗口长度 − 当前偏移`，偏移用
`byte_offset_from` 计算、不解引用），禁止对整个窗口绝对比较——"头在窗口内"与"载荷适配
剩余窗口"是两个独立不变量，必须分别断言。每个边界 crate 的 unsafe 纯逻辑面必须有**双
执行面**：Miri 默认门（`miri-boundaries.sh`，纯逻辑 filter 完整执行）+ 解析/遍历面另加
fuzz target（契约：任意字节序列不 panic、不越界、不伪造成功）；纯常量 ABI 面（如
perf-ioctl 的 `#[repr(C)]` 布局）无攻击者可控输入，豁免 fuzz、只入 Miri。审计是准入，
执行面矩阵随 QUALITY_GATES §5 与 Miri 脚本登记；执行面才是持续防线。

## Boundary 2 — Default unprivileged

主程序永远以普通用户启动，不使用主二进制 blanket `setcap`/`setuid`。需要权限的能力只在
用户主动使用时，按 feature 经 OS 原生授权进入：Linux polkit/pkexec 已接线；Windows UAC
`runas` 传输已在代码层接线（ADR-035 stage 2：`taskmanager-windows-api` 的
`ShellExecuteExW("runas")` + 有界等待 call group，`taskmanager-platform-windows` 的
driver 加一次性随机命名回传文件），并已通过 `x86_64-pc-windows-msvc` 交叉编译验证，
固定 helper 已纳入 Windows MSI 并安装在 GPUI 同目录，但尚无目标机成功/拒绝 receipt——
开发或损坏安装仍以 typed `HelperUnavailable` 诚实失败，不伪造提权。macOS native authorization 的 typed 词汇与纯映射已落地
（`taskmanager-escalation::authorization`），Security-framework 跨界保持未接线
（typed `Unsupported`），等待 signed privileged-helper 的 ADR。不得把普通同 token
子进程宣称为提权。用户拒绝、授权
未完成、helper 缺失、helper 协议违约和不支持必须是互斥的 typed outcome；Linux 仅把
`pkexec` 126 解释为明确拒绝。目标机实测取消也可能返回 127，因此 127 使用中性的
`AuthorizationUnavailable`/“授权未完成”，不得臆测是用户拒绝或授权服务故障。

## Boundary 3 — Capability classification

| 类别 | 典型能力 | 拒绝后的产品语义 |
|---|---|---|
| Direct | 进程/CPU/内存/磁盘/网络聚合、sysfs、cgroup-v2 | 字段级 unavailable/partial |
| Requires escalation | Intel PMU、AF_PACKET、foreign-uid control、受保护 SMART/服务控制、SMBIOS 明细（type-17 内存 + type 0/1/2 身份表，entries 与 id 序列号/UUID 节点 root-only）、RAPL 包功耗（energy_uj 0400）、MSR 读数（/dev/cpu/*/msr 0600） | `RequiresEscalation`、`PermissionDenied`、`AuthorizationUnavailable`、`HelperUnavailable` 或 `HelperProtocolViolation` |
| Unsupported | 没有稳定 safe/native seam 的 provider | `Unsupported`，不伪造数字 |

每个请求必须携带 capability、稳定 identity/generation、边界和取消语义；控制前后复核
身份，不能只凭 PID、路径或显示名称授权。一个能力的失败不能让其他设备或其他字段消失。

## 当前 helper 规则

- `taskmanager-privilege-helper`、`taskmanager-net-launcher`、`taskmanager-process-control-helper`、
  `taskmanager-smbios-helper`、`taskmanager-rapl-helper`、`taskmanager-msr-helper`
  和 `taskmanager-setup-helper` 都是固定参数、单次、有界 helper；不是 shell、daemon 或通用 root API。
  `taskmanager-process-control-helper` 额外接受可选的第四个参数——UAC 传输的一次性
  回传文件（必须已由应用侧独占创建，helper 只截断重写固定 JSON envelope，绝不新建路径）。
- `PolkitGate` 对每项已接线能力精确验证 `pkexec + policy + executable helper`；provider 注册
  和实际请求复用这一权威，缺安装时不得发起无效授权。
- helper 的安装、polkit policy、授权拒绝、成功、恢复和卸载分别验收；代码链闭合不等于
  live on-box receipt 闭合。
- Windows 遥测、测试和开发辅助路径禁止 PowerShell/CMD；缺少合格 API 时保持 typed fallback。

## 磁盘弹出（MC !493）授权走查

弹出是控制（写）能力，不是遥测：removable 介质与 hotplug 已是 typed 事实
（`media_removable`/`hotplug_capable`/`device_generation`），弹出入口只出现在可弹出设备上。
候选授权路径裁决如下：

| 路径 | 信任根 | 授权 | 裁决 |
|---|---|---|---|
| udisks2 over D-Bus（system bus，zbus blocking-api） | ioctl 与设备策略由 OS 系统服务承担；TaskForest 侧纯 safe Rust，零新增 `unsafe` | udisks 自带 polkit action（如 `org.freedesktop.udisks2.eject-media`），提示由系统 polkit agent 完成，站点策略可覆盖 | 采用 |
| 自有边界 crate 直接 ioctl（SG_IO/CDROM eject） | 打开设备节点加 ioctl 即第五个 audited unsafe 边界；设备节点通常仅 root 可打开，还需第五个 pkexec helper 与自有 policy | 自有 polkit policy | 否决 |
| 调用 `udisksctl`/`umount` 等命令行 | 命令解释器路径 | — | 否决（provider/frontends 不执行 shell） |

裁决依据：Boundary 1 的四个 audited boundary 是治理不变量，不是容量配额；为 OS 已原生
提供的服务扩大 `unsafe` 信任根违反"少做但诚实"。host 无 udisks2 或无 system bus（如最小
OpenRC）时的结论是 typed `Unsupported`，不是自建第五根。zbus 5.19（blocking-api）已是
`taskmanager-platform-linux` 依赖，无新依赖家族、无第二执行器。本能力不新增
`EscalationFeature` 变体、helper 或自有 `.policy`；授权语义与桌面文件管理器走同一条
udisks action。拒绝与未完成必须互斥：`Ejected`、`Denied`（用户拒绝）、
`AuthorizationUnavailable`（授权未完成/无 agent）、`Busy`（携带挂载分区列表，不自动
卸载）、`TargetUnavailable`（设备消失或 generation 变化）、`ServiceUnavailable`（无
udisks2/system bus）、`Unsupported`（非 removable）。

身份纪律与进程控制同级：请求携带稳定 device identity 与 generation；provider 调用前用
同一 identity 重解析 udisks object path，禁止缓存跨 generation 的路径，调用后复核；
热插拔竞态只能产生 `TargetUnavailable`，绝不弹出另一台设备。

失败/恢复 receipt（各一份 on-box）：真实 removable 介质成功弹出、polkit 拒绝、Busy
（挂载文件系统）、服务缺失、点击后拔出的竞态，以及弹出后设备以 generation 变化从遥测
投影消失（不留 stale 行、不冒充成功）。

串行审查检查点（eject port 实现的固定顺序，前一关未过不得开写下一关）：

1. typed contract：core/application 的请求/响应词汇、identity 字段与互斥 outcome 先行；
2. provider：D-Bus 错误名到 typed outcome 的穷举映射、bounded 超时、取消、不在 UI lane
   阻塞；
3. 权限面：无新 helper/policy；udisks 错误不得折叠成 `PermissionDenied`；
4. UI：四前端同一投影——仅 removable 可见、Busy 展示挂载分区、无阻断确认也必须有 typed
   反馈与恢复路径；
5. 行为测试与七层证据齐备后才允许接通 control 通道；产品账本只随 receipt 变化。

Windows/macOS 弹出不在本裁决的采用范围：Windows 若实现走 `taskmanager-windows-api`
既有边界并遵守无 PowerShell 规则；macOS 走 DiskArbitration native seam；两者实现前各自
按"新能力退出门"补 ADR。

## 新能力退出门

新增特权能力必须先有 ADR、最小 boundary、typed contract、失败/拒绝/恢复测试、打包策略和
目标机 receipt；新实现上线时同步下线或迁移旧实现，不保留语义分叉等待后来者处理。默认策略
永远是“少做但诚实”，不是“先升权再填数字”。
