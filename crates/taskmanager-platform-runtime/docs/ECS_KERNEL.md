# ECS scheduling kernel

`src/ecs.rs` is the runtime's mandatory Bevy ECS lifecycle kernel (ADR-033).
One `WorkState` component owns request identity and lease together; no
independent owner/deadline fields may form an invalid combination. Legal
transitions are:

| From | Input | To |
|---|---|---|
| `Waiting` | monotonic deadline | `Ready` |
| `Waiting` / `Ready` | admitted typed request | `InFlight { request, lease }` |
| `InFlight` | lease expiry | `Stalled { request, abandon deadline }` |
| `InFlight` / `Stalled` | matching terminal | `Waiting` or typed `Blocked(reason)` |
| `Stalled` | abandonment deadline | `Waiting` (retry backoff, delivery recycled) |
| transient `Waiting` | `ExplicitRetry` | `Waiting` due now |
| `Blocked(AwaitingCapabilityChange)` | `CapabilityChanged` | `Waiting` due now |

`Blocked(Permanent)` rejects every recovery trigger. A blind retry cannot
leave `Blocked(AwaitingCapabilityChange)`. Both recovery triggers return a
typed outcome and return `ActiveOwner` for `InFlight` and `Stalled`. The kernel
is fed by existing capability routes and the health catalog; it does not own OS
I/O, provider facts, application revisions, `PlatformEventBatch`, or frontend
projections. Existing typed request ports, bounded lanes, and fair event
delivery remain the execution contract.

## Capability scheduler

The application obtains a narrow `CapabilityScheduler` port from the runtime.
Each due plan is consumed by `PlatformClient::run_scheduled_refresh`, which maps
capabilities to the normal typed application request methods. Typed ECS
admission claims the entity before a bounded lane enqueue, so a second in-flight
request is rejected even when queue capacity remains; a failed enqueue rolls
that exact RequestId claim back. A failed scheduled intent may requeue only an
unowned `Ready` route, so an explicit request winning between planning and
submission cannot be cleared by the scheduled failure callback. The correlated
publisher completion carries the same `RequestId` back to ECS and receives a
typed owner/verdict. Only an accepted verdict may update the catalog; stale or
mismatched completion changes neither lifecycle nor health. Terminal delivery
claim validates the same ECS owner, and the publisher checks the completion
verdict instead of treating enqueue as completion. ECS retirement happens
before the best-effort catalog write, so a poisoned catalog lock cannot retain
completed work.

## Stalls, leases, and recovery

In-flight work has a bounded observation lease: expiry reports one stalled
transition but keeps the original worker authoritative until a matching
terminal publication arrives, or until the scheduler's own stall-abandonment
deadline (`RuntimeBudgets::max_stalled_lifetime_ms`, five leases by default)
retires the owner and requeues the route — an executor that stopped without
publishing must not strand a capability forever. Recovery triggers never
perform that retirement. Current stalled-owner counts, recovered-stall
counters, and abandoned-stall counters are exposed in the bounded scheduling
snapshot. A very late completion of a retired owner fails its delivery claim
and is tolerated by the publishing lane as a counted stale publication: only a
gone event transport stops a lane, and lane thread exits are counted in the
same snapshot. Target progress may renew the same request and recover
`Stalled` to `InFlight`; it cannot change request identity. Accepted completion
and renewal transitions carry an exhaustive `OwnedWorkPhase::{InFlight,
Stalled}` result. Recovered-stall accounting is derived only from that phase;
no boolean can silently merge an on-time result with a late recovery. A
rejected recovery trigger or mismatched late completion mutates neither
lifecycle, due time, delivery permit nor diagnostics.

A provider that never returns is not recoverable inside the same runtime
owner: its entity and one delivery permit remain quarantined, and retries are
rejected. Runtime replacement drops ECS ownership; the worker keeps its permit
until exit. Native providers own I/O timeout or cancellation.

## Clock authority

Runtime time has two authorities. The injected wall clock timestamps external
facts and events only; a process-local monotonic clock exclusively drives ECS
cadence, retry, and in-flight leases. `RuntimeConfig::with_monotonic_clock`
allows deterministic tests or an embedding host to replace that scheduling
clock. Wall-clock correction, rollback, and forward jumps therefore cannot
expire work or move a due deadline.

## Request scopes

Typed requests select one lifecycle scope: whole capability, stable target, or
sideband control of an existing job. Directory scans and generation-bound SMART
operations use target entities, so distinct targets may coexist while the same
target cannot overlap. Target entities are retired on terminal publication or
failed enqueue and are capped at 64 active jobs per capability; a worker that
consumes requests without completing them therefore cannot grow the ECS world
without bound. Fallible target-scope validation runs before both ECS
reservation and bounded-lane enqueue; invalid scope input therefore leaves
neither an entity nor a queued request. A successfully published directory
progress event renews the exact target lease from the monotonic clock.
Sideband admission is declared by the typed request contract and copied into
route metadata; denied is the default, while audited idempotent cancellation
opts in without a capability-ID special case.

## Plugins and snapshot

The kernel has one lifecycle plugin and one data-driven domain-diagnostics
plugin. The latter partitions cumulative planned work across the eight fixed
runtime domains without pretending that each domain already owns distinct ECS
policy. Each route has exactly one typed domain authority; duplicate capability
routes are coalesced deterministically before catalog/world construction. The
implementation-neutral scheduling snapshot is fixed-cardinality except for a
64-entry recent-stall ring; exact target scope is retained only in that bounded
diagnostic tail and active target registry.

ECS adoption is architectural and is not conditional on a performance win.
The kernel is assembled through a headless `bevy_app::App`; because Bevy's App
runner is not `Send`, the configured `World` and `Update` schedule are then
transferred into the worker-safe runtime scheduler. This preserves the existing
worker/thread contract without `unsafe`.

The headless suite under `tests/headless/` exercises typed tick, admission,
completion, retry, stale requests, domain partitioning, and fixed-seed
lifecycle models. It provides behavior/performance baselines and does not move
facts, revisions, process rows, or UI state into ECS.

There is no pre-ECS compatibility path: default and `--no-default-features`
builds use the same lifecycle authority. Feature selection therefore cannot
silently remove admission, leases, target bounds, or scheduler diagnostics.
