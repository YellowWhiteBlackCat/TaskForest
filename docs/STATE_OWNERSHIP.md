# 状态所有权与时序总纲

本文是跨 crate 状态、生命周期和事件顺序的唯一第二层权威。`ARCH.md` 只定义分层，
`UI_COMPONENT_ARCHITECTURE.md` 只定义 renderer 组件合同；crate README 只能展开本表实现。

## 所有权台账

| 事实或生命周期 | 唯一 owner | 唯一写入口 | 消费边界 | 代码锚点 |
|---|---|---|---|---|
| Canonical domain fact | `taskmanager-core` 私有 typed model | named constructor / transition / apply | current/last-known 只读 accessor | `taskmanager-core/src/core.rs`（owner module 索引） |
| 已发布外部 payload ingress | 私有 read DTO / serializer | ingress canonicalization；canonical serializer | 不进入 provider/frontend state，不形成兼容 API | `taskmanager-core/src/core/metrics/*/wire.rs` |
| 全局配置 | app-host 的单一 `ConfigCoordinator` | base→local typed submission | revisioned immutable snapshot | `taskmanager-application/src/config_runtime.rs` |
| 危险确认 | application `InteractionState` | arm/replace/confirm/dismiss reducer | 四前端只投影或提交 intent | `taskmanager-application/src/interaction.rs` |
| 相关异步请求 | application typed request session；每条 shell track 一个实例 | begin/accept/reject/terminal/close | frontend 只读状态；terminal payload 仅接受一次 | `taskmanager-application/src/request_session.rs`；track 实例在 `taskmanager-shell/src/app/request_sessions.rs` |
| quit 与用户反馈 | shell `ShellLifecycleState` | typed lifecycle event | `should_quit` / immutable feedback | `taskmanager-shell/src/app/lifecycle.rs` |
| 当前系统投影 | 每条前端轨私有的 `SystemProjectionStore` | platform batch 与命名 reducer | immutable `projection()` | `taskmanager-shell/src/app.rs` |
| live graph 历史 | `taskmanager-telemetry-store` | composition-owned ingestor | revision-keyed immutable series；per-device 曲线是双侧纪律——写侧 ingest 事务换环、读时 generation 过滤（`requested != 0 && ring == requested`），任何 per-device 读边必须携带 generation，单侧防护不成立 | `taskmanager-telemetry-store/src/live_graph.rs`、`system_history/ingest.rs` |
| 前端历史采集 generation | active frontend typed lifecycle | config Enable/Disable + bounded start/stop | app-host read-only replay client | `taskmanager-app-host/src/history_frontend.rs` |
| 持久历史 writer/lock | active frontend history session | correlated system/process/sensor/power ingestion | history-store JSONL/query；其他 frontend 不可写 | `taskmanager-app-host/src/history_persistence_runtime.rs` |
| runtime work | ECS `WorkState` | admission/renew/terminal/recovery system | scheduler snapshot 与 typed verdict | `taskmanager-platform-runtime/src/ecs.rs` |
| runtime delivery | `FairEventPort` | bounded primary/mailbox publication | typed partition turn + sequence merge | `taskmanager-platform-runtime/src/delivery/event_port.rs`、`delivery/event_queue.rs`；`EventSequence` 在 `taskmanager-platform-contract/src/envelope.rs` |
| 本地时区规则 | app-host `StartupLocalTimeCache` | 宿主启动时一次捕获 | cloned host 共享同一 `Arc` observation | `taskmanager-app-host/src/lib.rs` |
| 窗口事件时间 | frontend window-time component | tick/event 注入 | renderer filter/format，只读 | 对应 frontend crate 的 window-time 模块 |
| renderer projection cache | 对应 frontend component | revision/fingerprint miss | owned `Rc`/value，不返回可变 guard | 如 `taskmanager-iced/src/app/projection_caches.rs` |
| 外部 saved-view 输入 | config/preset 私有 read DTO | ingress canonicalization | canonical 只有一套 category/tree projection | `taskmanager-core/src/core/config.rs` |

GPU engine、网络提权及 command/reveal/open-url correlation 同样走该 request session owner；前端
不得另存 request map、pending bool、error 或 accepted payload。一个事实不得同时出现在 domain
public field、shell writable mirror 和 renderer local copy。
展示偏好可以有 renderer-ready immutable projection，但必须与其 canonical config snapshot 一次
提交，不能提供分别替换 draft、语言、主题或偏好字段的写入口。

## 批处理与投递顺序

| 边界 | 顺序合同 | 禁止推论 |
|---|---|---|
| runtime class 间 | `EventClass` typed turn；成功交付后 control/observation 交替 | bool 优先门或全局因果序列 |
| 同一 class | primary 与 terminal mailbox 按 `EventSequence` 合并 | queue 字段排列即先后 |
| application domain | 同域稳定升序归一化；跨域默认可交换 | 跨域最大 sequence 胜出 |
| shell batch fold | failure seed → 独立 domain systems → revision → alert watermark → feedback | frontend `if` 顺序充当事务 |
| renderer frame | update/drain 先生成 immutable projection，render 只消费 | render 中轮询 provider 或推进 session |

真实跨域因果必须先在 application 归约为一个 typed projection；不能让 runtime 公平投递、
struct 字段顺序或 toolkit 消息排列隐式决定业务结果。

## 生命周期矩阵

| Authority | From | Event | To / verdict | 不匹配、迟到或重复 |
|---|---|---|---|---|
| `InteractionState` | None/Properties/Confirmation | arm/replace/dismiss | 唯一 surface 或 None | stale dismiss/confirm 不产生 effect |
| `InteractionState` | matching Confirmation | confirm | None + 恰好一个冻结 effect | 只有 matching kind 可提交 |
| config client | observed revision | submit base→local | queued/no-op/rejected | failure 保留 canonical snapshot |
| settings form | Clean/Dirty/Conflict | edit/publication/cancel | 穷尽 typed transition | 外部 revision 不覆盖 dirty draft |
| request session | Closed/Idle | begin attempt | Loading(Attempt) | 旧 terminal 丢弃 |
| request session | Loading(Attempt) | accepted/rejected | Loading(Request)/Failed | 仅同 attempt 生效 |
| request session | Loading(Request) | terminal/close/replace | Ready/Failed/Closed/new Loading | request + frozen target/generation 全匹配 |
| history frontend | Disabled/Unavailable | enable | Connecting(request) | writer/replay bootstrap 在有界 worker；renderer 不阻塞 |
| history frontend | Connecting(request) | matching completion | Active(replay + writer)/Unavailable | 非当前 request 或 disable 后的 completion 丢弃 |
| history frontend | any | disable | Disabled + drop replay/writer | 不提交 connector work；frontend 退出即停止写入 |
| history persistence | Disabled | config Enable | Starting | 只在此后创建当前 frontend 的 writer/ingestor generation |
| history persistence | Starting/Running/Degraded | Disable/Terminate | Stopping | 同一 generation 有界 drain、flush、release |
| history persistence | Stopping | writer Released/Detached | Disabled 或 clean exit / Degraded + failure exit | 未释放时绝不发布 Disabled；frontend 进程退出释放残余所有权 |
| ECS work | Waiting/Ready | admitted request | `InFlight { request, lease }` | 第二 owner 被拒绝 |
| ECS work | InFlight | lease expiry | `Stalled { request }` | 不释放 owner、permit 或配额 |
| ECS work | InFlight/Stalled | matching terminal | Waiting/Blocked | terminal verdict 携带原 `OwnedWorkPhase` |
| ECS work | Stalled | matching progress | InFlight | 只续同 request；恢复计数一次 |
| ECS work | any owned phase | wrong/late completion or renew | 原状态 | due、permit、预算、诊断均不变 |
| quit | Running | typed quit(reason) | Requested(first reason) | 重复请求无第二 effect |
| feedback | activity/notice | typed report/batch/replace | 单一 `FeedbackState` | inventory/control 不各存字符串 |

`Stalled` 表示原 worker 仍有权返回，不表示可以并发重试。永不返回的 provider 在当前 runtime
内保持隔离；只有 runtime owner 替换才能清除 ECS ownership，detached worker 仍占进程级 permit。

## Renderer 状态边界

- GPUI `RootView`、Iced `IcedApp` 只组合有命名职责的 component state；共享语义 payload 不进入
  local surface。Iced 配置 draft、语言、presentation preferences 与 Theme 原子替换。
- `RefCell` 只允许封装 toolkit `draw(&self)` 或一次性交接所需的短借用；projection cache 在执行
  builder 前释放借用，并只返回 owned snapshot。跨 callback、render subtree 或外部 API 暴露 guard
  均禁止。
- 窗口时间缓存按窗口隔离，由 tick/event 注入。app-host 时区缓存策略固定为
  `HostRestartOnly`；当前版本不伪装支持运行期时区 watcher。

## Core 硬切换与外部输入

硬切换不变量的权威出处是 [AGENTS.md](../AGENTS.md)，外部 payload 的 ingress 规则权威出处是
[`WIRE_DOMAIN_BOUNDARIES.md`](WIRE_DOMAIN_BOUNDARIES.md)。状态侧只补充一条：被替换的
renderer state、fixture 与 demo/capture 路径属于"旧 API"，必须与 typed contract 在同一
变更内删除。

## 验证

状态机以穷尽类型和行为测试证明合法迁移、拒绝、取消、迟到、重复与恢复；批处理测试同时断言
数据、owner、permit、预算、revision 和 feedback 副作用。可见状态还需按
[`QUALITY_GATES.md`](QUALITY_GATES.md) 与 [`screenshots/README.md`](screenshots/README.md)
取得当前宿主证据；测试数量或源码名字零命中不能单独证明完成。
