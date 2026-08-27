# 跨平台策略总纲

三平台是同等产品面，但不是同一事实源。共同能力进入 core/application/platform contract；
OS 特性留在 adapter；没有合格来源的能力保持 typed `Unsupported`，不以临时脚本或假数字
扩大支持面。

## 安全选择顺序

1. 复用成熟 safe crate 或标准 API，并把结果转换为 typed domain facts。
2. 若只剩小型、稳定、必要的 native ABI，建立独立 audited boundary crate，并为句柄、长度、
   分配、等待、错误和身份复核建立上限与测试。
3. 如果边界不值得审计，保留 capability descriptor，返回 `Unsupported` 或
   `MissingDependency`，记录候选 API 到 ADR，不在 adapter 内偷偷扩大权限。

业务 crate 不承载 `unsafe`；权限只通过 `taskmanager-escalation` 和 OS 原生授权路径进入。
Windows 生产、测试和开发辅助代码均禁止 PowerShell、CMD 或其他命令解释器承担遥测。

## 平台矩阵

| 平台 | 事实源与组合 | 当前原则 |
|---|---|---|
| Linux | `/proc`、`/sys`、systemd/OpenRC、SMART、hwmon、DRM、NVML、审计 boundary | 参考实机面；权限和硬件差异必须 typed、可恢复 |
| macOS | `sysinfo`、`battery`、plist、安全系统命令和原生 composition | 能编译不等于实机完成；缺 receipt 保持 contract/Unsupported |
| Windows | `sysinfo`、`raw-cpuid`、battery、安全 registry/service crate、ADR-031 boundary | safe/native-first；缺合格 seam 不实现 PDH/WMI/命令解释器旁路 |

## 统一能力规则

- 平台 adapter 只发布 capability registration、facts、failure 和 provenance，不改变 shared
  command、history 或 UI 语义。
- provider unavailable、权限失败、驱动缺失和 confirmed absent 必须分开；一个字段失败不
  能让同一设备或其他平台条目消失。
- 稳定 identity、generation、last-success 和 recovery 由 runtime/application 管理；热插拔、
  重排、counter rollback 和 PID reuse 都必须断开旧 baseline。
- 标准二进制在运行时发现 Intel、AMD、NVIDIA、NVMe、ATA、Wi-Fi 等能力；硬件 vendor 不是
  Cargo feature、包名或发行 SKU。
- 目标机 receipt 只证明对应平台/硬件/权限场景，不得外推到其他平台。

## 提权规则

默认启动不需要 root/setcap/setuid。单项能力在用户明确请求后经：

- Linux polkit/pkexec helper；
- Windows UAC/native helper（尚未接线；当前 foreign-process control 为 typed `Unsupported`）；
- macOS 原生授权 seam。

授权、拒绝、超时、helper 缺失和恢复必须转为 UI 可见的 typed outcome；主程序不能因为一项
能力需要权限而整体失败。

## 实现边界

平台细节进入以下 crate，而不进入 `taskmanager-core` 或 renderer：

- `taskmanager-platform-linux`：Linux provider、`/proc`/`/sys`、init、SMART、GPU 和控制；
- `taskmanager-platform-macos`：macOS provider SPI、safe command 和 fallback；
- `taskmanager-platform-windows`：Windows provider SPI、safe/native seam 和 fallback；
- `taskmanager-platform-native`：可执行组合边界；
- `taskmanager-platform-provider` / `runtime`：跨平台 provider contract、catalog、lane 和 delivery。

每个 crate 的入口、允许依赖、失败模型和验证命令见对应 `README.md`；信任决策见 `adr/`。
