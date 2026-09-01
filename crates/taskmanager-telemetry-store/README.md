# taskmanager-telemetry-store

## Role

Platform-neutral bounded in-memory telemetry history and correlated read model.
`LiveGraphHistory` is the single renderer-neutral live-chart authority consumed
by Shell, Iced and TUI; GPUI's direct track reads the same store semantics.

## Boundary

The store accepts validated facts with identity, generation, revision and gap
metadata. It does not read OS sources, schedule providers, persist implicitly,
or format toolkit-specific charts. The write handle is separate from the cloned
read model so frontends cannot create a competing append path.

## Contract and verification

History preserves gaps, stale transitions, revision and device lifecycle while
enforcing memory bounds. A healthy authoritative battery or sensor enumeration
retires histories for identities no longer present; unavailable/partial source
states never masquerade as removal. Verify append/query/revision behavior,
retirement and generation isolation without relying on live hardware. The
three correlated per-core rings share core's hard outer-cardinality limit, so
one oversized provider vector cannot multiply retained histories. Live graph
rings have a physical 600-sample maximum and evict the oldest sample; a user
history preference only narrows the visible tail and cannot make storage grow
without bound. Dynamic battery/sensor histories retain old identities across
partial discovery, append explicit gaps when a retained identity is not sampled,
and share a 256-identity domain ceiling; new identities above that ceiling are
rejected with an observable ingestion report until authoritative discovery
retires old entries.

Persistence callbacks execute inside the owning domain commit so persisted
order cannot diverge from live rings and receipts. Custom sinks must not
synchronously read that same telemetry domain; the production store only
appends to its own bounded pending state and satisfies this non-reentrant
contract.

CPU and memory ingestion reads only canonical current accessors. Failed,
partial, stale, or unavailable groups append gaps; legacy wire projections are
never an alternate history input, and confirmed-empty per-core groups remain a
real empty generation rather than a provider failure.
GPU ingestion follows the same rule: every scalar series reads `current_*`,
throttle availability stays outside scalar history, and device generation keys
prevent NVML/DXGI or hot-plug data from crossing identities. Read APIs expose
the named utilization/engine windows plus one generation-scoped typed-point
window (`gpu_metric_point_series_for`) that folds each retained
`GpuMetricPoint` with the caller's projection; the per-family scalar chart
windows were retired when the shell's chart-metric dispatch became their only
consumer, so one fold (the shell's `GpuChartMetric::value`, the same fold its
availability gate derives from) serves every frontend. The retired generic
graph-series selector vocabulary and availability reconciliation do not live
in this crate. Every per-device system-domain read edge — the series
resolver's device legs and the `*_for` accessors they back (disk summed and
split rates, activity, SMART temperature; network summed and split rates; GPU
utilization, engine and typed-point windows) — carries the caller's device
generation and serves a ring only at the generation it was reset for (`0`
never serves), so a row/ring disagreement across a batch boundary is an
honest empty window, never the previous instance's curve.
Dynamic battery/sensor and storage/network/GPU ingestion enter through named
transaction inputs that bind history, lifecycle, correlation, generation,
commit gate and persistence fan-out atomically; positional ingest paths do not
coexist.
Storage and network also keep split live rings (`storage_read/write_rate`,
`network_rx/tx_rate`) derived from the same accepted observation as the summed
ring — one fact, two projections, no recomputed sums. Split rings are
live-graph only (`PersistFanout::disabled()`): the persisted metric vocabulary
and file format are unchanged, and missing per-direction samples append
explicit gaps, never zeros.

Chart series reads go through one scope-aware model. Every `MetricSeries`
variant declares a `SeriesScope` — `Host`, `Device(DeviceDomain)`, or
`HostAndDevice` — and `LiveGraphHistory::resolve_series` routes one
`ChartSeriesQuery` accordingly: host-domain series (CPU/memory scalars) to
their host ring, disk/network/GPU families to the per-device ring or the host
aggregate (`*_total` / `*_mean`) of the same accepted observation. A
wrong-domain query (host series with a device identity, device series without
one) returns a typed `ChartSeriesError` and is never redirected to the other
domain. The legacy `series`/`*_for` accessors remain as thin wrappers over the
same dispatch; the infallible legacy `series` reports a device-only series as
an empty host window, and `MetricSeries::slot` derives storage ordinals from
`MetricSeries::ALL` so slot-keyed caches stop hand-maintaining numeric tables.
Disk active time is a device-only series: the persisted `storage-activity-pct`
per-device ring is the authority, and no host mean is fabricated (the interim
host mean ring was removed when the scope model made the per-device resolution
the single path).

## Module map

```text
src/live_graph.rs                revision-keyed immutable series; per-device reads must
│                                carry generation (double-sided discipline)
src/system_history.rs
├── ingest.rs                    CorrelatedSystemTelemetryIngestor: the only write entry
│   └── ingest/{dynamic,storage_network}.rs
├── device.rs  dynamic_history.rs  gpu.rs   per-device curves and dynamic history
```

Writers: composition-owned ingestor only. Readers: frontend replay clients (read-only).
