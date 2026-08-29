# taskmanager-application

## Role

Toolkit-neutral commands, reducers, ports, capability requests and correlated
outcomes for the TaskForest application layer.

## Boundary

It owns intent, lifecycle, revision, identity and feedback state. It does not
select a platform adapter, import toolkit types, or render. Injected-path
configuration persistence is its one bounded filesystem primitive; the native
composition root owns the corresponding coordinator lifetime.
`PlatformHandle` may retain an opaque native lifetime owner, ensuring cloned
handles and their worker runtime share one last-handle boundary without exposing
platform or runtime types to this layer.
Removed device-owner authority is retained as a bounded newest tombstone tail,
preserving recent cross-partition conflict protection without an ever-growing
identity map.

## Key modules

- `src/command.rs` defines user intent and typed effects.
- `src/control.rs` and `src/refresh.rs` own live application-facing flows.
- `src/persistent_app_history.rs` projects each accepted process snapshot into
  three durable metrics for a deterministic verified-first set of at most 256
  identities; admission is recomputed per snapshot, while the history store
  alone owns cross-window series retention.
- `src/application_history_projection.rs` joins persistent replay rows into the
  one application-history read model consumed by GPUI, Iced, TUI, and Bevy, including
  explicit capability states and timestamp-aware chart gaps.
- `src/config_store.rs` owns base-aware configuration transactions. Each
  writer merges only top-level fields changed from its last locally observed
  snapshot into the lock-protected current disk value; an unchanged periodic
  snapshot cannot revert another writer. A background reservation freezes that
  snapshot's merge base before executor handoff; successful save advances the
  writer base to the local snapshot. An OS file lock serializes independent
  instances and processes with a bounded typed timeout.
- `src/config_runtime.rs` owns the toolkit-neutral bounded configuration worker,
  client-local base→local patch submissions, monotonic `ConfigRevision`, immutable
  full-snapshot publications, typed recovery/failure, slow-client resync and
  independent last-handle shutdown. A broken refresh retains the last-good
  snapshot; exact and lock-level no-ops do not publish.
  Its monotonic immutable publication is the single process-wide settings
  authority; clients retain only a revisioned snapshot and submit base→local
  changes back to that coordinator.
- `src/history_replay.rs` owns the persistent-replay request generation and
  `Closed | Loading | Ready | Failed` reducer. Only the current request may
  resolve; late/duplicate completions are inert, failed refreshes retain and
  label their last-good window, and a closed controller rejects refresh/window
  transitions. Published rows are immutable and curve cardinality is bounded.
- `src/boot_baseline.rs` owns boot-evidence request identity, duplicate
  suppression, late-completion rejection and last-good retention. A baseline
  is projected only when the successful evidence matches the current boot;
  failure of a newer boot cannot relabel an older comparison.
- `src/service_lifecycle.rs` owns service dependency and log-stream sessions.
  Admission gets a typed attempt identity before platform submission, accepted
  attempts atomically upgrade to `RequestId`, and only matching target/query
  generations resolve. Close, retry, filter and cursor races retain last-good
  data only within the same target/filter generation.
- `src/request_session.rs` owns affinity-read, process-batch, SMART-self-test,
  GPU-engine acquisition, process-network escalation and shell UI-action
  admission/terminal lifecycles. Attempts upgrade atomically to platform request
  ids; terminals additionally match their frozen domain identity, capability and
  payload kind. Replace, close, retry, late and duplicate behavior is defined by
  these states rather than frontend `Option + bool + error` or request maps.
- `src/snapshot_export.rs` owns immutable export requests and the correlated
  `Closed | Queued | Running | Ready | Failed` lifecycle. Its non-blocking port
  couples admission failure, late/duplicate terminal rejection and close; it
  contains no serializer, current-directory discovery or filesystem adapter.
- `src/diagnostics.rs` prepares already-redacted diagnostic plans and owns the
  request-correlated publication port. The app-host alone owns its worker and
  file transaction; closing a frontend session makes a late completion inert.
- `src/interaction.rs` owns the single shared dangerous-confirmation machine: one frozen
  EndTask/ProcessBatch/Service/Startup/Session/SMART-self-test payload, explicit arm/replace/confirm/dismiss
  transitions, and the sole conversion from confirmed intent to platform effect.
- `src/alert_suggestion_window.rs` retains only the bounded evidence windows used
  by alert and SMART suggestions; general live chart history belongs to
  `taskmanager-telemetry-store`.
- `src/managed_alert_rules.rs` owns the complete `ManagedAlertRule` list and
  typed toggle/add/update/remove/import reducer. `AlertCenter` derives its
  evaluator from enabled entries only; disabled entries remain canonical and
  visible, while missing stable identities and invalid atomic imports cannot mutate state.
  Its bounded alert-transition history is the single event-center source for
  every renderer; clear/export operate on that typed source.
- `src/platform/client/scheduler.rs` owns the closed automatic cadence/dispatcher registry;
  runtime route construction consumes that same authority.
- `src/platform/event_batch.rs` owns the frontend batch ordering seam. Runtime
  `EventSequence` is canonical only inside each correlated domain after fair
  control/observation delivery; application projections use typed revision.
  The batch defines no cross-domain last-writer order.
- `src/lib.rs` exposes only application-owned commands, reducers, ports,
  lifecycles and projections. Domain facts and platform contracts are imported
  from `taskmanager-core` and `taskmanager-platform-contract` at their actual
  owners; this crate has no model-forwarding facade.
  Cohesive algorithm namespaces (`history_decimation`,
  `process_category_projection`, `process_details_vm`, `process_sort` and
  `snapshot_export`) remain public only through their named module. Adapter
  selection remains outside this crate.

## Contract and verification

Cross-crate ownership and transition matrices are authoritative only in
[`docs/STATE_OWNERSHIP.md`](../../docs/STATE_OWNERSHIP.md); this README expands
the application implementations without redefining their legal states.

Ports are typed seams: failures, permission requests, stale results and
partial outcomes remain observable. Test reducers and ports with pure behavior
cases and keep provider selection at `taskmanager-app-host`. Directory scan
roots and SMART physical identity plus generation define target lifecycle;
native SMART locators do not become identity. Single-process work uses PID plus
the provider-issued start token, including every independently scheduled
process-insight facet; service, startup, and session work uses their opaque
provider-issued IDs. Process batches remain one capability-scoped transaction
rather than unrelated target jobs. Target identities are preserved exactly
within the platform-contract byte bound; missing or oversized authority fails
before runtime admission instead of being truncated or hashed. UI-facing
per-core history is projected by `taskmanager-telemetry-store`, which applies
core's shared outer-cardinality ceiling without truncating the authoritative
`CpuMetrics` fact itself.

Scheduled system telemetry submits only the domains actually due from the
runtime. Those domains share one application revision for an atomic projection,
while each retains independent request correlation and failure rollback. Pending
correlation is replaced per domain rather than globally cleared or accumulated.
Every drained batch is stable-sorted inside each domain before it leaves the
application, and the shell repeats that normalization defensively for alternate
typed producers. Cross-domain folds must commute unless the shell declares an
explicit precedence phase; struct field order is never execution order.
