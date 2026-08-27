# ADR-033: Bevy ECS runtime scheduling kernel

- 状态：已接受
- 相关：`adr/016-typed-observation-wire-invariants.md`、
  `adr/027-renderer-independent-shell-state.md`、`docs/ARCH.md`、
  `crates/taskmanager-platform-runtime/README.md`

## 背景

平台采集运行时具有实体、组件、系统和调度器的形状，但仍受 TaskForest
的单向依赖、typed event、bounded lane、revision 和前端契约约束。把整个
应用或 OS 采集层改成游戏引擎式 ECS 会混合事实权威、请求生命周期和渲染
投影，也不能仅凭架构相似性证明性能收益。

## 决策

1. Bevy ECS 只存在于 `taskmanager-platform-runtime` 的调度内部，负责
   capability work lifecycle：`waiting → ready → in-flight → completed`，以及
   typed failure policy 允许的 `requeue`。一个 capability route 对应一个
   runtime entity；provider、delivery class、due time 和 work state 是内部数据。
2. 现有 typed `RequestPort`、bounded worker lane、fair event delivery、
   `PlatformEventBatch` 和 correlation 继续是执行与发布契约。ECS 只产生
   typed `CapabilityId` 调度意图；application 通过窄的
   `CapabilityScheduler` port 消费它，并映射到现有 request method。
3. 入队前由 ECS 以 `RequestId` 占用 route；入队失败回滚占用。ECS 不读 OS、
   不构造事实、不修改 application revision，也不把 ECS API 暴露给 frontend。
4. `taskmanager-core` 继续拥有事实和纯规则，`taskmanager-application` 继续
   拥有 command/reducer/port/revision，平台 crate 继续拥有 OS I/O。进程行、
   历史 ring 和 UI projection 不实体化。
5. runtime 构造先验证 route、global/per-capability/per-domain target、scope
   bytes 和 pending-delivery budgets。每个 admitted lifecycle 持有 delivery
   permit 直到 frontend drain terminal；满 primary queue 的 terminal 进入同
   permit 上限的 mailbox，terminal 不因消费者变慢而静默丢失。
6. observation 不消耗 control reserve；同类 primary/mailbox 按
   `EventSequence` 合并。旧 mailbox 终态不能被新 primary 终态超越，publisher
   在同一短临界区分配 sequence 并提交。
7. headless `bevy_app::App` 只用于组装插件与 `Update` schedule；配置好的
   scheduler 保持 `Send + Sync`。不为 ECS 增加 `unsafe` 或跨线程共享 Bevy
   runner。

## 约束与后果

- ECS 模块保持 crate-private，不通过 re-export 扩大公共 API。
- runtime 只发布 typed scheduler verdict；catalog health 仍由现有 capability
  catalog 和 application projection 负责。
- due order、in-flight 去重、lease、bounded retry 和 stale completion rejection
  都在 runtime 内部保持确定性；慢 provider 不得阻塞无关 facet。
- 性能结论必须有对应行为、失败和目标平台证据；架构选择本身不构成性能承诺。

## 验证

headless runtime tests 覆盖 admission、due ordering、request correlation、
rollback、lease、retry、stall、mailbox/backpressure、domain partition 和
stale completion。依赖防火墙确保 ECS 不越过 application port、读取 OS 或进入
frontend；跨平台测试只证明共享契约，不替代原生环境验证。
