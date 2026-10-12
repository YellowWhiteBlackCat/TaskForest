# Ingestion rules

CPU and memory ingestion reads only canonical current accessors. Failed,
partial, stale, or unavailable groups append gaps; legacy wire projections
are never an alternate history input, and confirmed-empty per-core groups
remain a real empty generation rather than a provider failure.

GPU ingestion follows the same rule: every scalar series reads `current_*`,
throttle availability stays outside scalar history, and device generation
keys prevent NVML/DXGI or hot-plug data from crossing identities.

Read APIs expose the named utilization/engine windows plus one
generation-scoped typed-point window (`gpu_metric_point_series_for`) that
folds each retained `GpuMetricPoint` with the caller's projection; the
per-family scalar chart windows were retired when the shell's chart-metric
dispatch became their only consumer, so one fold (the shell's
`GpuChartMetric::value`, the same fold its availability gate derives from)
serves every frontend. The retired generic graph-series selector vocabulary
and availability reconciliation do not live in this crate.

Every per-device system-domain read edge — the series resolver's device legs
and the `*_for` accessors they back (disk summed and split rates, activity,
SMART temperature; network summed and split rates; GPU utilization, engine
and typed-point windows) — carries the caller's device generation and serves
a ring only at the generation it was reset for (`0` never serves), so a
row/ring disagreement across a batch boundary is an honest empty window,
never the previous instance's curve.

Dynamic battery/sensor and storage/network/GPU ingestion enter through named
transaction inputs that bind history, lifecycle, correlation, generation,
commit gate and persistence fan-out atomically; positional ingest paths do
not coexist.

Storage and network also keep split live rings
(`storage_read/write_rate`, `network_rx/tx_rate`) derived from the same
accepted observation as the summed ring — one fact, two projections, no
recomputed sums. Split rings are live-graph only
(`PersistFanout::disabled()`): the persisted metric vocabulary and file
format are unchanged, and missing per-direction samples append explicit
gaps, never zeros.
