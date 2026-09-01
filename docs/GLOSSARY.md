# 术语表

全仓行话在此集中定义；各文档只链接本页，不重复定义。定义只描述当前语义，
历史沿革查对应 ADR。

## 分层与边界

- **领域事实（domain fact）**：core 拥有的 typed 状态。一事实一 owner；跨 crate 只能从
  owner module 导入（[ARCH](ARCH.md)、ADR-047）。
- **owner module**：某事实/类型的唯一所属模块（如 `core.rs` 是 core 的 owner 索引）。
  禁止转发 facade、re-export 第二地址（ADR-047）。
- **硬切换（hard cutover）**：core 演进必须在新 typed contract 上线的同一变更内删除旧
  API、alias、wrapper、fixture、demo/capture 与 caller（[AGENTS](../AGENTS.md)、
  [WIRE_DOMAIN_BOUNDARIES](WIRE_DOMAIN_BOUNDARIES.md)）。
- **ingress canonicalization**：已发布外部旧格式只能在私有 read DTO 解析后一次性转为
  canonical truth；它不是兼容 API，不得被 provider/frontend 导入
  （[WIRE_DOMAIN_BOUNDARIES](WIRE_DOMAIN_BOUNDARIES.md)）。
- **audited boundary crate**：唯一允许 `unsafe` 的四个 crate（perf-ioctl、afpacket、
  fd-bridge、windows-api），只向外返回 typed 值（[PERMISSION_MODEL](PERMISSION_MODEL.md)）。
- **helper**：固定参数、单次、有界的特权可执行文件；不是 shell、daemon 或通用 root API
  （[PERMISSION_MODEL](PERMISSION_MODEL.md)）。
- **typed outcome / typed Unsupported**：拒绝、未完成、缺失、不支持是互斥的 typed 值；
  绝不折叠成同一个错误、零值或空集合。

## 数据与可用性

- **ScalarObservation / OptionalObservation**：值、可用性、最近成功时间的唯一契约；
  `0` 只表示实测零（[SCALAR_AVAILABILITY](SCALAR_AVAILABILITY.md)）。
- **稳定身份与 generation**：设备/进程身份重用后必须断开旧基线；per-device 历史读边必须
  携带 generation（[STATE_OWNERSHIP](STATE_OWNERSHIP.md)）。
- **capability facet / provider**：一个能力一个 facet，一个 facet 一个 provider；运行时
  注册发现，缺失不伪造（ADR-007、[TELEMETRY_MANIFEST](TELEMETRY_MANIFEST.md)）。

## 运行时与状态

- **channel / lane**：每能力一条有界通道；lane 是通道内的投递分区（platform-runtime）。
- **EventClass / EventSequence / FairEventPort**：runtime 公平投递词汇；成功交付后
  control/observation 交替（[STATE_OWNERSHIP](STATE_OWNERSHIP.md)）。
- **WorkState**：ECS work 的所有权状态机（admission、lease、stall、terminal；ADR-033）。
- **request session**：相关异步请求的 typed 会话；terminal payload 只接受一次；每条
  shell track 一个实例（[STATE_OWNERSHIP](STATE_OWNERSHIP.md)）。
- **shell track（轨）**：每个前端在 shell 中的独立状态轨，各持一个
  `SystemProjectionStore` 实例。
- **batch fold（批处理折叠）**：shell 事件归并的固定顺序：failure seed → domain
  systems → revision → alert watermark → feedback（[STATE_OWNERSHIP](STATE_OWNERSHIP.md)）。
- **投影（projection）/ projection cache**：shell 生成的不可变只读视图；渲染层缓存
  miss 时重建，只返回 owned snapshot，不返回可变 guard。

## 前端与 UI

- **四端 / 对等前端（peer frontends）**：GPUI（当前发布形态）、Iced、TUI、Bevy 共享
  同一应用投影，互不拥有业务事实（[ARCH](ARCH.md)）。
- **同异律**：五条跨前端定律——语义完备、映射穷尽、同一、语义平价、折叠
  （[ARCH §8](ARCH.md)）。
- **折叠律**：渲染入口只回放数据层折叠，不重算（"一次折叠，四端渲染"）。
- **触碰迁移律**：存量平台词汇或内联折叠在触碰时中性化、下沉到共享层
  （[ARCH §8.3](ARCH.md)）。
- **CORE-04 注册表**：`taskmanager-ui-contract::functional` 中产品意图 × 前端的显式
  surface decision 清单（shared/local/accepted-difference/typed-unsupported）。
- **surface role**：app-host 发布的窗口角色（Standalone / LayerShell）；per-surface 而
  非全局模式（[HOST_ARCHITECTURE](HOST_ARCHITECTURE.md)）。
- **FrameBudget / ContentBudget**：GPUI 每帧不可变布局预算（ADR-038）。
- **page family**：共享 PageScaffold 的数据页族（ADR-042）；图表页组合根在 ADR-039。

## 验证

- **quick / standard / extended 门禁**：本地分层门禁，入口
  `scripts/quality/local-gates.sh`（[QUALITY_GATES](QUALITY_GATES.md)）。
- **scope 门禁**：并行前端线用 `--scope <line>` 把 cargo 阶段限制在
  "core + 依赖闭包 + 本 crate"（[QUALITY_GATES](QUALITY_GATES.md)）。
- **capture receipt**：真实像素证据回执（PNG + source manifest + marker）；fixture、
  旧图与构造 Scene 不能替代（[QUALITY_GATES](QUALITY_GATES.md)）。
- **test-intent / source-inspection**：测试文件头声明；默认禁止读生产源码证明行为
  （[TEST_LAYOUT](TEST_LAYOUT.md)）。
