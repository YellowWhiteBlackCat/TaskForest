# 跨前端 UI 场景平权审计总纲

前端平权的唯一判定标准是能力与场景的 1:1 全射。GPUI 作为基准发布表面，其拥有的全部 54 个专用交互场景均属于产品必须作答的业务契约。任何一端缺失对应入口或像素实证，均属于未平权的缺陷状态。

## 场景全景审计与当前状态

状态标记：`Aligned`（已建立对应场景与截屏验证）、`Pending`（处于待对齐推进状态）。

| 分组 | GPUI 场景 Token | 场景定义 | Iced | Bevy | TUI |
|---|---|---|:---:|:---:|:---:|
| 基础与系统 | `about` | 关于系统与版本模态框 | Aligned | Pending | Aligned |
| | `system-about` | 系统关于面板展开 | Aligned | Pending | Aligned |
| | `first-run` | 首次运行系统引导配置 | Aligned | Pending | Pending |
| | `system-dashboard` | 多硬件总览仪表盘 | Aligned | Aligned | Aligned |
| | `system-hardware` | SMBIOS 硬件拓扑明细 | Aligned | Aligned | Aligned |
| | `system-npu` | NPU 神经网络遥测明细 | Aligned | Aligned | Aligned |
| 进程诊断切片 | `process-selection` | 选中单行进程基准态 | Aligned | Aligned | Aligned |
| | `process-properties-performance` | 进程性能指标明细 Tab | Aligned | Aligned | Aligned |
| | `process-memory-pss-swap` | 进程 PSS/USS/Swap 内存展开 | Aligned | Aligned | Aligned |
| | `process-network-details` | 进程网络 Socket 与 RTT 明细 | Aligned | Aligned | Aligned |
| | `process-gpu-details` | 进程 GPU 显存与引擎明细 | Aligned | Aligned | Aligned |
| | `process-resource-limits` | cgroups/prlimit 资源配额明细 | Aligned | Aligned | Aligned |
| | `process-isolation` | 命名空间沙箱与 Seccomp 审计 | Aligned | Aligned | Aligned |
| 危险操作确认 | `process-force-kill` | 强制终止（SIGKILL）警示弹窗 | Aligned | Aligned | Aligned |
| | `process-tree-confirm` | 结束完整进程树连带确认框 | Aligned | Aligned | Aligned |
| | `process-batch-confirm` | 批量终止多个选中进程确认框 | Aligned | Aligned | Aligned |
| | `smart-self-test-confirm` | 磁盘 SMART 自检二次确认框 | Aligned | Aligned | Aligned |
| 存储与硬件诊断 | `storage-health` | 磁盘 SMART 属性与健康详情 | Aligned | Aligned | Aligned |
| | `smart-missing-tool` | 缺失 smartctl 时的引导提示 | Pending | Pending | Pending |
| | `smart-permission` | SMART 特权提权失败/引导态 | Pending | Pending | Pending |
| | `partition-disk-usage` | 分区挂载点与磁盘空间图 | Pending | Pending | Pending |
| | `partition-live-usage` | 磁盘动态吞吐与 IOPS 视图 | Pending | Pending | Pending |
| | `device-hotplug` | 设备热插拔事件触发与重排 | Pending | Pending | Pending |
| | `sensor-center` | 主板温控传感器与各 Thermal Zone | Aligned | Aligned | Aligned |
| | `battery-fan-performance` | 电池状态与风扇转速联动卡 | Pending | Pending | Pending |
| | `battery-live-performance` | 电池实时放电速率与电压曲线 | Pending | Pending | Pending |
| | `gpu-engine-inventory` | 多 GPU 引擎枚举与负载图表 | Pending | Pending | Pending |
| | `intel-gpu-telemetry` | Intel 专属硬件遥测扩展指标 | Pending | Pending | Pending |
| 服务与系统诊断 | `service-details-logs` | 服务实时日志流输出面板 | Aligned | Aligned | Aligned |
| | `services-search-highlight` | 服务名关键词搜索过滤态 | Aligned | Aligned | Aligned |
| | `diagnostic-preview` | 系统一键诊断报告生成后预览 | Aligned | Aligned | Aligned |
| | `diagnostic-failure` | 诊断生成失败或超时错误模态 | Aligned | Aligned | Aligned |
| 开机启动项分析 | `startup-impact` | 启动项引导耗时影响等级分析 | Aligned | Aligned | Aligned |
| | `startup-failure-evidence` | 启动服务崩溃/异常退出取证 | Aligned | Aligned | Aligned |
| | `startup-boot-markers` | systemd-analyze 开机关键链时间轴 | Aligned | Aligned | Aligned |
| 监控、告警与事件 | `active-alert` | 顶部活动告警横幅展开态 | Aligned | Aligned | Aligned |
| | `alert-rules-manager` | 自定义告警规则配置中心 | Aligned | Aligned | Aligned |
| | `event-center` | 系统安全与审计事件追踪中心 | Pending | Pending | Pending |
| | `saved-view-presets` | 自定义列宽与视图预设切换 | Pending | Pending | Pending |
| | `telemetry-paused` | 遥测全局暂停冻结状态水印 | Aligned | Aligned | Aligned |
| 历史数据回放 | `history-replay` | 性能指标时间轴历史回放控制条 | Aligned | Aligned | Aligned |
| | `history-60m` | 60 分钟跨度全局历史趋势图 | Aligned | Aligned | Aligned |
| | `application-history-replay` | 进程级 CPU/内存历史回溯展开 | Aligned | Aligned | Aligned |
| 交互与显示形态 | `apps-search-highlight` | 进程搜索关键字高亮与命中过滤 | Aligned | Aligned | Aligned |
| | `apps-group-expanded` | 进程分组/进程树全部节点展开 | Aligned | Aligned | Aligned |
| | `apps-identity-matrix` | 进程身份校验与伪装报警状态 | Pending | Pending | Pending |
| | `apps-zero-gray` | 进程零值置灰与活动值对比渲染 | Pending | Pending | Pending |
| | `settings-switch-focus` | 设置页焦点切换与高亮状态 | Pending | Pending | Pending |
| | `settings-zero-gray` | 设置项置灰与可用性状态对比 | Pending | Pending | Pending |
| | `settings-permission-center` | 特权 Helper 授权管理中心视图 | Pending | Pending | Pending |
| | `keyboard-focus` | 全键盘无障碍导航焦点环形态 | Pending | Pending | Pending |
| | `vertical-nav` | 垂直导航栏折叠与图标态 | Pending | Pending | Pending |
| | `sidebar-hidden` | 性能页左侧设备导航栏收起态 | Aligned | Aligned | Aligned |
| | `sidebar-edit` | 性能页设备显示/隐藏列编辑态 | Pending | Pending | Pending |

## 平权推进与收敛机制

1. **绝对基准**：本表 54 个场景 Token 属于硬性全集，任何针对 GPUI 场景的扩充必须同步在此登记。
2. **状态推进**：每一端新增场景时，必须同时满足代码状态机接线、单测断言、场景矩阵收录、真实 Wayland 截图生成四项要求，才允许将状态标记从 `Pending` 升级为 `Aligned`。
3. **闭环守护**：质量门禁通过静态脚本机械校验本表完整性，严防场景描述遗漏或拼写漂移。
