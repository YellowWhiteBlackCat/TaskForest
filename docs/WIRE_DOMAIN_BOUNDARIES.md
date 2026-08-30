# Wire 输入与领域事实边界

本文定义外部 JSON/快照输入如何一次性进入 core 的 canonical typed truth；它不定义任何
仓库内部兼容 API，也不授权 provider 或 frontend 直接写旧字段镜像。

## 总合同

- Domain model 只允许一个可写事实源；外部旧格式只允许由私有 ingress DTO 解析，并在进入
  domain 的同一瞬间 canonicalize。旧字段不得出现在 public model、shell、frontend、fixture、
  demo/capture 或 export。
- core 演进必须硬切换：新 typed contract 上线的同一变更中，删除旧 API、alias、wrapper、
  fallback、renderer state、测试夹具和调用方。禁止 deprecated 入口、兼容 facade、双栈和
  “以后再清理”的迁移清单。
- ingress DTO 是外部数据格式解析器，不是实现兼容边界：它不能被 provider/frontend 导入，
  不能被 live projection 调用，也不能把旧值留作当前事实。新写出只使用 canonical truth。
- `ScalarObservation`、`OptionalObservation`、`ProcessMetadataObservation`、
  `ScalarObservationGroup` 和 `SensorMeasurementObservation` 的私有 deserialize DTO 会拒绝
  自相矛盾的 payload；Rust domain 字段全部私有，生产代码只能使用 named constructor/transition。
- 通用 observation 只暴露语义明确的 `current_*` 与 `last_known_*` 只读投影；不提供 raw
  `value/state/observations`。Partial group 只接受 `Current/Partial/Unavailable` slot vocabulary，
  由 group 统一写入 current success time，Unknown/Stale slot 与时间不一致的 wire 会被拒绝。
- 外部旧字段只能在对应 typed slot 为 `Unknown` 且输入具有必要身份/时间证明时被一次性
  canonicalize；`Stale`、`Unavailable`、`Partial` 和 confirmed absence 不得回放旧值。
- 任何缺少身份证明的旧字段都直接成为 typed unavailable/unknown，不得通过 PID、索引、
  sentinel 或空值猜测成当前事实。
- Provider 写入顺序必须是“构造 typed observation → 一个 domain apply/constructor”；不得先后
  独立赋值 typed 与外部旧字段。Serializer 只从 canonical truth 单向生成发布格式。
- 跨 crate 行为 fixture 只由 dev-only `taskmanager-test-support` 的 typed builder 组装；产品依赖图
  不导出 fixture 宏，builder 不接受旧字段名或 sentinel hydration。
- 统一退出门：生产 writer/consumer 零旧语义调用，当前 domain/frontend/export 只有 canonical
  类型；外部旧 payload 若仍是已发布格式，只能在私有 ingress 解析并立即进入 canonical truth，
  typed 非当前状态不会在输出中看起来像成功。
- 表中的 writer 规模指平台/provider 路径；编入产品的 demo/capture builder 也是 writer，字段
  私有化前必须同步改成 canonical constructor，不能以“只用于演示”为由保留旁路。

## 逐域现状

| 域 | Wire DTO | Canonical truth 与 fallback | Public mirror / writer 规模 | 下一步与退出条件 |
|---|---|---|---|---|
| Process metadata | 是：`ProcessItemWire` 私有承载旧 `user`/`exe_path` 与 typed metadata/application identity | private `metadata_observations` 与 `application_identity`；仅可信非零 PID、非空进程 identity 且 typed `Unknown` 时导入非空 legacy metadata | 无 public mirror；3 个平台 list writer 与 Linux retention 均走 typed apply | 已退出：frontend/export 只读 current accessors；typed absent/stale/unavailable/partial 不退回 legacy，serializer 只投影 current truth |
| Process scalars | 是：`ProcessItemWire` 私有承载 9 个 schema-v1 行数值与 typed scalar group | private `ProcessScalarObservations`；CPU/RSS/rate 需有效 PID，threads/start 需非零，cpu-time/fds/nice 还需 current nonzero start token；PSS/swap/累计 I/O 无 fallback | 无 public mirror；3 个平台 list writer 均一次 typed apply，frontend/export 只读 `current_*` | 已退出：typed-only 可省略旧键，measured zero 明确投影 0，Partial/Stale/Unavailable 不生成旧成功键 |
| CPU | 是：`CpuMetricsWire` 与 `CpuScalarObservationsWire` 私有承载 outer scalar/vector 与 3 个旧 item vectors | private `CpuScalarObservations` + typed per-core groups；仅可信 identity/topology 且 typed `Unknown` 时导入旧 outer 值，旧 item vector 仅补 Unknown group | 无 live public mirror；3 个平台与 demo writer 均 typed assembly/constructor | 已退出：serializer 仅从 `Available` 投影旧成功键，confirmed-empty 显式保留，Partial/Stale/Unavailable 不伪装 0/空成功 |
| Memory | 是：`MemoryMetricsWire` 私有承载基础标量、composition/module/commit/compression/rate | private `scalar_observations` + `optional_observations`；legacy scalar 需正 total denominator，optional 再需 current typed total identity，且只补 typed `Unknown` | 无 public mirror；3 个平台与 demo writer 均 typed assembly + 单次 constructor/apply | 已退出：Linux scalar+optional retention 原子化；三前端只读 current accessors；零化 percentage 方法已删除，非当前 typed truth 不生成旧成功键 |
| Disk / partition | 是：私有 `DiskMetricsWire` / `DiskPartitionWire` 独占 one-axis transport、removable 与旧数值 vocabulary | private scalar/partition observations、`StorageConnection`、media-removable/hotplug；旧 capacity/rate/iops 需 nonzero，active/response 需 positive finite，mounted free/used 可迁显式 0；旧 `removable=false` 不证明 false | 无 public mirror/旧 accessor；3 平台 inventory 与 Linux SMART/rate/retention 都消费 typed connection/apply | 已退出：typed measured zero 可往返，失败不投影成功键，one-axis transport 类型不离开私有 DTO |
| Network | 是：私有 `NetworkMetricsWire` 用 `Option<NetworkAdapterType>` 区分缺失与显式 `Other` | private scalar/wireless observations + adapter class；旧数值需可信 device/interface identity，旧无线值还需 class 缺失且 `is_wireless=true`；false 不证明 class，link-up 无 fallback | 无 public mirror；3 平台 writer 单次 typed apply；Windows 无 SSID/signal 双写 | 已退出：typed class/absence/failure 胜出，旧零按 identity 迁移，Unknown/Stale/Unavailable 不伪装 false/空/0 |
| GPU | 是：私有 `GpuMetricsWire` 承载利用率、温度、三套 memory、频率、风扇、功率、RC6 与 throttle text/vector | private `GpuScalarObservations` 是 live scalar truth，private availability-bearing throttle observation 与 engine failure/provenance 独立；旧 payload 仅凭非空 device identity、逐字段旧 sentinel 导入 Unknown，上一版 gen=0/Unsupported/empty-provenance envelope 仍可读 | 无 live public mirror；Linux AMD/Intel/NVML、macOS inventory、Windows NVML/DXGI 与 demo writer 均 typed assembly + 单次 apply | 已退出：typed 冲突胜；confirmed-empty throttle 有明确 availability；Partial/Stale/Unavailable 不投影旧成功键；多 provider precedence 与 device generation 保持不变 |
| Battery / power supply | 是：`BatteryInfoWire` 私有，domain 不持有 legacy scalar | `BatteryInfo` 私有持有 `BatteryScalarObservations`；反序列化 fallback 仅限非空 id、Healthy、存在 last-success 且 typed `Unknown` | 无 public mirror；Linux 与 portable collector 均为 typed assembly + 单次 apply | 已退出：生命周期只 retention typed truth；serializer 仅把 `Available` 投影到旧 Option，Partial/Stale/Unavailable 均为 null |
| Sensor / thermal | 是：`SensorReadingWire` 与 `ThermalThrottleSnapshotWire` 私有；legacy kind/value/state/counters 不在 domain | `SensorMeasurementObservation` 与 typed throttle observations；仅 typed `Unknown` 时按 legacy current state 迁移，非当前 typed truth 不回退 | 无 public mirror；Linux/macOS/Windows 均 typed assembly，lifecycle 仅 retain observation | 已退出：typed-only/legacy-only 可读、冲突时 typed 胜出，serializer 只从 canonical current truth 投影旧字段 |
| Process resources | 是：`ProcessResourceSnapshotWire` 私有，旧 limits/groups/6 个 Option 只在 serde 边界 | `ProcessResourceObservations`；旧值只在 typed `Unknown`、存在 success time 且非空/`Some` 时导入，retention 的 group-derived 值还要求同一 resource-group identity | 无 public mirror；Linux/Windows 均组装 typed observation，macOS 明确无能力 | 已退出：typed 冲突胜出，confirmed empty 由 observation variant 表达，只有 typed `Current` 投影旧成功字段 |
| Service relations | 是：`ServiceDepsWire` 与 `ServiceItemWire` 私有，共用同一 legacy relation helper | `ServiceRelationGraph`；反序列化时仅当 graph 缺少某 kind 才导入旧 string，未知 kind 保真 | 无 public mirror；dependency detail 与 inventory 仅暴露 typed/read-only projection | 已退出：两种 payload 共用 typed-wins merge/projection，provider 只组装 typed edges |

## 持续审计

当前表内没有已知生产 writer 旁路。新增字段或 provider 必须先进入 canonical constructor/apply；
外部输入解析只能在私有 ingress 完成，随后由 serializer 单向生成发布格式。修改现有 schema 时
证明输入 canonicalization、typed 冲突胜出、非当前状态不生成成功值；不得恢复 domain mirror、
旧调用入口或 renderer fallback。GPU 多 provider 扩展还必须让 provenance、failure、scalar 与
throttle 属于同一 generation。
