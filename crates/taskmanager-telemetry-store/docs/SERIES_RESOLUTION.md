# Series resolution

Chart series reads go through one scope-aware model. Every `MetricSeries`
variant declares a `SeriesScope` — `Host`, `Device(DeviceDomain)`, or
`HostAndDevice` — and `LiveGraphHistory::resolve_series` routes one
`ChartSeriesQuery` accordingly: host-domain series (CPU/memory scalars) to
their host ring, disk/network/GPU families to the per-device ring or the host
aggregate (`*_total` / `*_mean`) of the same accepted observation.

A wrong-domain query (host series with a device identity, device series
without one) returns a typed `ChartSeriesError` and is never redirected to
the other domain.

The legacy `series`/`*_for` accessors remain as thin wrappers over the same
dispatch; the infallible legacy `series` reports a device-only series as an
empty host window, and `MetricSeries::slot` derives storage ordinals from
`MetricSeries::ALL` so slot-keyed caches stop hand-maintaining numeric
tables.

Disk active time is a device-only series: the persisted
`storage-activity-pct` per-device ring is the authority, and no host mean is
fabricated (the interim host mean ring was removed when the scope model made
the per-device resolution the single path).

## Timestamped reads

`LiveGraphHistory::resolve_series_samples` is the read edge that answers a
query with the accepted samples, each carrying the instant the observation was
MEASURED (`core::metrics::MetricSample`). The value-only window is the
projection of the same accepted list and stays index-aligned with it.

A sample whose observation carries no measurement time is a GAP: it has no
value and no instant, and the surface shows the shared dash rather than an
offset. The acceptance time is a separate field and is never relabelled as
the sampling time. A query for the wrong domain is rejected by the
timestamped read exactly as it is by the value read.
