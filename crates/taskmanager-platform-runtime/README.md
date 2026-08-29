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

## Key modules

- `src/channel/` and `src/delivery/` own bounded lanes, fairness and publication.
- `src/registration.rs` and `src/composition.rs` own provider catalog wiring.
- `src/config.rs` accepts required system/process bindings as named typed input
  transactions; lane promotion likewise consumes named observation/control
  groups. Positional multi-capability constructors are not a compatibility API.
- `src/assembly.rs` owns the one shared native provider/executor assembly seam;
  OS crates still own their registries, provider adaptation, and IDs.
- `src/ecs/lifecycle.rs` owns capability transitions; `src/ecs/target_jobs.rs`
  owns independently addressed job admission, leases, and entity retirement.
- `src/process.rs`, `src/system.rs`, `src/storage.rs` and `src/power.rs` own runtime jobs;
  `src/process/spawn.rs` isolates process worker startup and event routing from lane contracts.
  Directory scanning drives one typed lane context plus one immutable scan job.

## Worker lifetime boundary

Every provider lane is created through a named `std::thread::Builder` and is
owned by one `WorkerRuntime`; startup failure is typed and aborts native
composition. Seven dashboard lanes are resident; the remaining typed lanes
retain their bounded channel and provider state, start on first request, and
retire after 15 seconds idle. Detail loading is therefore on demand without
changing request, health, correlation, or failure semantics. A partial
composition drops the owner and disconnects idle lanes; the completed
`PlatformHandle` retains it opaquely through its last clone.

Worker admission has a 64-lane runtime ceiling and a 128-thread process ceiling.
Permits live on worker threads, so detached stuck providers remain bounded.
Normal exit returns the permit; drop wakes idle workers and joins only finished
threads. Native providers still own their I/O timeout/cancellation boundary,
and results returning after shutdown are not published.

Native bounded-command readers follow the same fallible named-thread rule:
partial startup reaps the child; sampler startup cleans up when its reader fails.

## ECS runtime boundary

`src/ecs.rs` is the runtime's mandatory Bevy ECS lifecycle kernel. One
`WorkState` component owns request identity and lease together; no independent
owner/deadline fields may form an invalid combination. Legal transitions are:

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

The application obtains a narrow `CapabilityScheduler` port from the runtime.
Each due plan is consumed by `PlatformClient::run_scheduled_refresh`, which maps
capabilities to the normal typed application request methods. Typed ECS
admission claims the entity before a bounded lane enqueue, so a second in-flight
request is rejected even when queue capacity remains; a failed enqueue rolls
that exact RequestId claim back. A failed scheduled intent may requeue only an
unowned `Ready` route, so an explicit request winning between planning and
submission cannot be cleared by the scheduled failure callback. The correlated
publisher completion carries the same `RequestId` back to ECS and receives a
typed owner/verdict. Only an accepted
verdict may update the catalog; stale or mismatched completion changes neither
lifecycle nor health. Terminal delivery claim validates the same ECS owner,
and the publisher checks the completion verdict instead of treating enqueue as
completion. ECS retirement happens before the best-effort catalog
write, so a poisoned catalog lock cannot retain completed work. In-flight work
has a bounded observation lease: expiry reports one stalled transition but
keeps the original worker authoritative until a matching terminal publication
arrives, or until the scheduler's own stall-abandonment deadline
(`RuntimeBudgets::max_stalled_lifetime_ms`, five leases by default) retires the
owner and requeues the route — an executor that stopped without publishing must
not strand a capability forever. Recovery triggers never perform that
retirement. Current stalled-owner counts, recovered-stall counters, and
abandoned-stall counters are exposed in the bounded scheduling snapshot. A
very late completion of a retired owner fails its delivery claim and is
tolerated by the publishing lane as a counted stale publication: only a gone
event transport stops a lane, and lane thread exits are counted in the same
snapshot. Target progress may renew the same request
and recover `Stalled` to `InFlight`; it cannot change request identity. Accepted
completion and renewal transitions carry an exhaustive
`OwnedWorkPhase::{InFlight, Stalled}` result. Recovered-stall accounting is
derived only from that phase; no boolean can silently merge an on-time result
with a late recovery. A rejected recovery trigger or mismatched late completion
mutates neither lifecycle, due time, delivery permit nor diagnostics.

A provider that never returns is not recoverable inside the same runtime owner:
its entity and one delivery permit remain quarantined, and retries are rejected.
Runtime replacement drops ECS ownership; the worker keeps its permit until exit. Native providers own I/O timeout or cancellation.

Runtime time has two authorities. The injected wall clock timestamps external
facts and events only; a process-local monotonic clock exclusively drives ECS
cadence, retry, and in-flight leases. `RuntimeConfig::with_monotonic_clock`
allows deterministic tests or an embedding host to replace that scheduling
clock. Wall-clock correction, rollback, and forward jumps therefore cannot
expire work or move a due deadline.

Typed requests select one lifecycle scope: whole capability, stable target, or
sideband control of an existing job. Directory scans and generation-bound SMART
operations use target entities, so distinct targets may coexist while the same
target cannot overlap. Target entities are retired on terminal publication or
failed enqueue and are capped at 64 active jobs per capability; a worker that
consumes requests without completing them therefore cannot grow the ECS world
without bound. Fallible target-scope validation runs before both ECS reservation
and bounded-lane enqueue; invalid scope input therefore leaves neither an entity
nor a queued request. A successfully published directory progress event renews
the exact target lease from the monotonic clock. Sideband admission is declared
by the typed request contract and copied into route metadata; denied is the
default, while audited idempotent cancellation opts in without a capability-ID
special case.

The kernel has one lifecycle plugin and one data-driven domain-diagnostics
plugin. The latter partitions cumulative planned work across the eight fixed
runtime domains without pretending that each domain already owns distinct ECS
policy. Each route has exactly one typed domain authority; duplicate capability
routes are coalesced deterministically before catalog/world construction. The
implementation-neutral scheduling snapshot is fixed-cardinality except for a
64-entry recent-stall ring; exact target scope is retained only in that bounded
diagnostic tail and active target registry.

`RuntimeBudgets` is validated by the only production constructor,
`ChannelRuntime::try_new`, before channels or ECS entities are allocated. The
default route ceiling is 64: one power-of-two step above the current 45 typed
routes and equal to the worker-lane ceiling. Per-capability targets remain 64;
the explicit global/domain ceilings are 256/128, so at most four full target
partitions exist globally and two in one domain. Since every scope is already
capped at 4 KiB by the contract, retained target identity is capped at 1 MiB.
Zero, crossed, undersized-delivery, and route-overflow budgets fail as typed
construction errors.

Every admitted lifecycle also reserves one class-attributed delivery permit. A
terminal must first claim that exact `(CapabilityId, RequestId)` permit before
it can retire lifecycle state; wrong, missing, and duplicate claims change
nothing. Completion retires the ECS owner but keeps that permit until the
application drains its terminal event. Primary event queues remain bounded by
`QueueCapacities`; if one is full,
the terminal moves to a mutex-backed mailbox bounded by the 352 default permits
(64 capability owners + 256 target owners + 32 control-reserved slots).
Observation admission cannot consume that control reserve. No pump thread or
provider blocks on frontend progress. Control and observation stay fair; within
each class, primary and mailbox fronts merge by `EventSequence`, preserving FIFO
even when draining a primary slot permits a newer publication to refill it.
Sequence allocation and insertion into primary storage or the retained mailbox
share one short commit lock, so concurrent publishers cannot expose sequence N+1
while sequence N is still between allocation and enqueue. Intermediate progress
uses non-blocking coalescing, while terminal outcomes are never intentionally
dropped. Queue depth/high-water and budget pressure are visible in the scheduling snapshot.
Terminal enqueue and capability-health commit additionally share one publication
barrier with catalog snapshots: once a consumer can observe a terminal event, a
subsequent capability snapshot cannot expose the pre-terminal health state.

Event draining uses a typed two-partition turn. Control owns the initial turn;
after a delivery the other partition owns the next turn, so control cannot
starve observations. Empty partitions fall through. Within a partition,
primary and retained-terminal queues merge by `EventSequence`; cross-partition
fairness is not causality. `EventFinality::{Progress, Terminal}` selects
coalescing or the bounded terminal mailbox.
ECS adoption is architectural and is not conditional on a performance win.
The kernel is assembled through a headless `bevy_app::App`; because Bevy's App
runner is not `Send`, the configured `World` and `Update` schedule are then
transferred into the worker-safe runtime scheduler. This preserves the existing
worker/thread contract without `unsafe`.

The headless suite under `tests/headless/` exercises typed tick, admission,
completion, retry, stale requests, domain partitioning, and fixed-seed lifecycle
models. It provides behavior/performance baselines and does not move facts,
revisions, process rows, or UI state into ECS.

There is no pre-ECS compatibility path: default and `--no-default-features`
builds use the same lifecycle authority. Feature selection therefore cannot
silently remove admission, leases, target bounds, or scheduler diagnostics.

## Contract and verification
The cross-crate ledger is [`STATE_OWNERSHIP.md`](../../docs/STATE_OWNERSHIP.md). Preserve sequence,
generation, partial/stale/unavailable outcomes and recovery; verify queue fairness, cancellation,
timeout, delivery ownership and live drain without host UI.
