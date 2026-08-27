# 标量可用性总纲

所有可失败的数值都必须携带值、可用性和最近成功时间。`0` 只表示真实测得的零，不能
代替未知、不支持、权限不足、暂时失败、过期或设备消失。契约归 `taskmanager-core`，
provider 与前端只实现或消费它，不定义第二套语义。

## 共享契约

`ScalarObservation<T>` 区分 `Unknown`、`Available`、`Partial`、`Stale` 和 `Unavailable`；
`FailureKind` 至少区分 `Unsupported`、`PermissionDenied`、`MissingDependency`、`Timeout`、
`IdentityChanged`、`TemporarilyUnavailable`、`Rejection` 与 `ProviderFault`。可选事实使用
`OptionalObservation<T>` 的 `Unknown`、`Present`、`Absent`、`NotApplicable` 轴，禁止用
`ScalarObservation<Option<T>>` 混淆语义。

保留旧值时必须标为 `Stale`，current accessor 不得返回它。设备 discovery authority 负责
存在性，generation 负责重接，失败不等于 confirmed absent；恢复的第一样本通常只建立
baseline，不得伪造速率。No hardware-vendor feature is part of this contract。

## 迁移矩阵

| Domain | Compatibility only | Typed vertical | Remaining |
|---|---|---|---|
| CPU | 旧标量只在 typed truth 为 `Unknown` 时读取 | 利用率、频率、温度、功耗与逐核 group | macOS/Windows 与目标硬件 receipt |
| GPU | 旧利用率、显存和温度字段只作兼容 | 利用率、显存、时钟、温度、功耗、风扇、idle | 多卡、权限、驱动恢复 receipt |
| Memory | 旧数值与旧 Option 只作兼容 | total/used/swap、组成、模块、压缩 | 跨平台硬件 receipt |
| Disk | 旧容量、速率与 I/O 字段只作兼容 | 容量、挂载空间、速率、IOPS、延迟、SMART 分层 | 多文件系统、计数器复位、热插拔 |
| Network | 旧 totals/rates/link/SSID 只作兼容 | counter、rate、carrier、link、wireless optional | 重命名、无线、跨平台恢复 |
| Sensors | 旧 kind/value 只作兼容 | unit、source scale、reading 与生命周期分离 | cooling、IPMI/Redfish、热插拔 |
| Power | 旧 BatteryInfo/batteries 只作兼容 | capacity、voltage、power、cycle、Battery/UPS kind | 外围电源和目标机恢复 |

## 当前能力项

### CPU (`CORE-AVAIL-01`)

首样本是 gap；测得零保留为 current；counter rollback、权限和 provider failure 形成 typed
状态，逐核数组不能用空集伪造未知。

### GPU (`CORE-AVAIL-02`)

各字段独立合并；一个 enrich­er 失败不能抹掉同设备其他字段。显存、idle 和 engine 数据
区分 confirmed zero、unsupported 与 stale。

### Memory (`CORE-AVAIL-03`)

组成、模块和压缩是独立 optional facts；DMI 单字段失败不扩散到整台机器。

### Disk (`CORE-AVAIL-04`)

容量/挂载空间/计数器/SMART 分层，稳定设备 generation 变化时清理旧速率基线。

### Network (`CORE-AVAIL-05`)

首个 counter 只建 baseline；不变 counter 是真实零；carrier down 保留状态但关闭 rate；
重新连接重新建 baseline。`NetworkScalarObservations` 是 live 数字唯一 authority。

### Process (`CORE-AVAIL-06`)

CPU、内存、I/O、线程、fd、nice 和资源限制均要求稳定身份；没有 exact start token 不得
授权控制或把兼容默认值当成当前事实。pidfd 只能缩小 residual race，不能替代前后身份复核。

### Sensors (`CORE-AVAIL-07`)

发现生命周期与每次读数独立；单位、量纲、异常范围和来源都进入 typed observation。

### Power (`CORE-AVAIL-08`)

Battery、UPS 和外围电源拥有独立 kind 与 generation；capacity、voltage、power、cycle
各自失败，不以聚合状态覆盖字段级事实。

### Optional facts (`CORE-AVAIL-09`)

`Absent` 只由 discovery authority 确认；`NotApplicable` 表示该设备类别不拥有该字段，
两者都不能写成 `Unsupported` 或空字符串。

### Recovery (`CORE-AVAIL-10`)

失败→恢复必须保留 sequence、generation、last-success 和 gap；前端只能消费 current
projection。旧 JSON 读取可兼容，但新写出必须使用 typed 语义。

## 下一层

实现细节在 `crates/taskmanager-core/README.md`、平台 provider README 和测试；本文件只
定义跨 crate 的语义，不记录迁移批次、测试总数或历史回执。
