# taskmanager-platform-runtime

## Role

OS-neutral bounded execution runtime: provider catalog, worker lanes, fair delivery,
correlation, lifecycle and telemetry integration.

## Boundary

Runtime schedules and transports work but does not define OS facts, page layout,
widget state or product copy; every lane has bounded work, ownership and failure delivery.

Provider registration may carry one typed composition-time capability status.
The catalog publishes it before the first request; correlated terminal health
then becomes authoritative. A provider must not hide failure inside a nominal
success payload, because catalog health and terminal state are one transaction.

The Bevy ECS scheduling kernel owns lifecycle admission, leases, target jobs
and typed recovery; worker lanes are bounded and fair; budgets and event
delivery are validated at construction.

## Contract and verification

The cross-crate ledger is [`STATE_OWNERSHIP.md`](../../docs/STATE_OWNERSHIP.md). Preserve sequence,
generation, partial/stale/unavailable outcomes and recovery; verify queue fairness, cancellation,
timeout, delivery ownership and live drain without host UI.

## Module map

```text
src/assembly.rs  composition.rs        channel assembly and composition
src/registration.rs                    provider catalog wiring
src/channel/                           one bounded channel per capability
│   ├── lanes.rs  port.rs  facet_attach.rs
│   └── construction/  budget.rs  context.rs  system.rs  process.rs  facets.rs
src/delivery/                          FairEventPort + EventClass fair delivery
│   └── event_port.rs  event_queue.rs  publisher.rs  worker/  catalog.rs
src/ecs/                               Bevy ECS scheduling kernel (ADR-033)
│   └── domain.rs  lifecycle/  scheduling_systems.rs  target_jobs.rs
src/process/  (+ spawn.rs)             process worker jobs and startup
src/system/  (+ snapshot_lanes.rs)     system telemetry jobs
src/storage/  (+ directory_scan.rs  smart_state.rs  smart_tracking.rs)
src/environment.rs  integration.rs     environment and integration jobs
src/power.rs  sensor.rs  service.rs    power, sensor, service jobs
src/absent.rs                          absent-capability typed handle
src/health.rs  lifecycle.rs  config.rs runtime health, lifecycle and configuration
```

## Docs

- [`docs/ECS_KERNEL.md`](docs/ECS_KERNEL.md) — ECS lifecycle transitions, scheduler port,
  stall/lease recovery, clock authority, request scopes and plugins.
- [`docs/WORKER_LANES.md`](docs/WORKER_LANES.md) — key module ownership, worker lifetime,
  lane ceilings and named-thread admission.
- [`docs/EVENT_DELIVERY.md`](docs/EVENT_DELIVERY.md) — runtime budgets, delivery permits,
  terminal mailbox, fairness and the publication barrier.
- [`STATE_OWNERSHIP`](../../docs/STATE_OWNERSHIP.md) — cross-crate state ledger;
  [ADR-008](../../adr/008-platform-runtime-execution-mechanics.md) and
  [ADR-033](../../adr/033-bevy-ecs-runtime-scheduling-kernel.md) — runtime mechanics.
