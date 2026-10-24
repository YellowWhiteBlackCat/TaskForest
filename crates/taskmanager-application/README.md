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
Domain facts and platform contracts are imported directly from `taskmanager-core` and
`taskmanager-platform-contract` at their actual owner modules; this crate maintains no
model-forwarding facade.

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

## Module map

```text
src/command.rs (+ command/spec.rs)         shared command vocabulary, one entry per intent
src/first_run.rs                           first-run onboarding session state
src/interaction.rs                         danger-confirmation state machine
src/request_session.rs (+ request_session/*)  typed request sessions per target
src/config_runtime.rs (+ publication.rs)   bounded configuration worker
src/config_store.rs                        base-aware configuration transactions
src/control.rs  action.rs  reducer.rs      control orchestration, action vocabulary, reducer
src/refresh.rs  router.rs                  refresh scheduling and intent routing
src/ports.rs                               typed application port seams
src/alert_center.rs  alert_dispatch.rs     alert evaluation and dispatch
src/alert_suggestion_window.rs             bounded evidence windows for alert/SMART
src/managed_alert_rules.rs                 full managed-rule list and typed reducer
src/history_replay.rs  history_decimation.rs  application history projections
src/system_timeline.rs (+ system_timeline/*) system timeline event history and projection
src/application_history_projection.rs      joined application read model
src/performance_history_projection.rs      performance read model for all frontends
src/history_replay_samples.rs              shared timestamp-aware gap kernel
src/persistent_app_history.rs              durable per-snapshot metrics
src/process_category_projection.rs         category aggregates and bucket order
src/process_details_vm.rs  process_sort.rs process detail view-model and sort axes
src/process_resource_projection.rs         per-process resource projection
src/service_lifecycle.rs                   service dependency and log-stream sessions
src/snapshot_export.rs  window_capture.rs  immutable export and capture requests
src/diagnostic_bundle/ (models.rs report.rs)  diagnostic bundle assembly
src/diagnostics.rs                         redacted diagnostic plans and publication port
src/boot_baseline.rs  device_lifecycle.rs  boot evidence and device lifecycle
src/telemetry_refresh_policy.rs            refresh cadence policy
src/source_status.rs  path_contract.rs  i18n.rs
src/platform.rs                            platform integration root
├── client/ (scheduler drain handshake …)  serial reducers over runtime event ports
├── event_batch/ (per-domain batch files)  frontend batch ordering seam
├── facets/ (per-capability request files) capability facet request sessions
├── process_insights_projection/           process insight aggregation
├── smart_projection.rs                    SMART status projection
├── startup_evidence_projection.rs  startup_timeline_projection.rs
├── system_telemetry_projection.rs         system telemetry read-model
└── handle.rs                              opaque native lifetime owner
```

## Docs

- [`docs/HISTORY_PROJECTIONS.md`](docs/HISTORY_PROJECTIONS.md) — persistent, application,
  performance and system-timeline projections plus replay and boot baselines.
- [`docs/REQUEST_SESSIONS.md`](docs/REQUEST_SESSIONS.md) — typed request sessions, service
  lifecycle, export/capture, dangerous-confirmation, first-run and diagnostics.
- [`docs/CONFIG_AND_ALERTS.md`](docs/CONFIG_AND_ALERTS.md) — config transactions, the
  configuration worker and managed alert rules.
- [`docs/BATCH_ORDERING.md`](docs/BATCH_ORDERING.md) — event-batch ordering seam and
  scheduled telemetry drain rules.
- [`STATE_OWNERSHIP`](../../docs/STATE_OWNERSHIP.md) — cross-crate state ledger;
  [ADR-036](../../adr/036-in-process-history-persistence.md) and
  [ADR-040](../../adr/040-optional-setup-discovery.md) — history persistence and setup discovery.
