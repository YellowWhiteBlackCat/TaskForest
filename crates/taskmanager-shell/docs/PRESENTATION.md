# Presentation projections

- `src/saved_views.rs` owns saved process-view identity, built-ins, typed
  filter/sort/columns, strict JSON transfer and config projection shared by all
  four frontends. Native controls adapt this model; persisted schema and
  transfer limits belong to core config.
- `src/presentation/about.rs` owns the common build-metadata projection and
  copied details. Product identity, license and repository remain owned by
  neutral assets.
- `src/presentation/system_information.rs` groups cached OS, kernel, desktop
  and hardware facts, omits unavailable rows and preserves complete copied
  values.
- `src/presentation/health_review.rs` projects complete filesystem integrity,
  sensor measurements and generation-matched SMART reports. Unknown counts and
  denied observations remain unavailable. `fixture/health.rs` owns the shared
  deterministic health inputs and seeds the normal storage/SMART fold.
- `src/app/diagnostics.rs` freezes observed snapshot/process inventory,
  services, startup and diagnostic facts into one sanitized application plan.
  Missing required observations fail as unavailable; unobserved optional
  inventories remain null. Shared presentation owns the preview summary and
  privacy-safe typed failure text.
- `src/presentation.rs` and `src/viewmodel.rs` expose renderer-neutral
  projections, including the product-first GPU display identity that keeps
  hardware names separate from driver names.
- `src/presentation/privilege_center.rs` folds helper capability/request states
  once for all four renderers. Authorization is offered only when the catalog
  exposes a usable request seam; GPU actions retain the observed device
  identity.

## GPU chart metric session

GPU history retains typed utilization/scalar/engine query windows. The shared
chart-metric selection model in `presentation/gpu_chart_metric.rs` owns the
telemetry-store series vocabulary, typed availability gating (unavailable
series project explicit, never zero or hidden), default Utilization,
fixed-order cycling, and generation-reset reconcile as one pure per-window
session state. All supported frontends consume this projection, and the
headline window read (`gpu_chart_metric_history`) is one generation-scoped
typed dispatch over the store's `gpu_metric_point_series_for`: the same
`GpuChartMetric::value` fold the gate uses, consumed by Iced/TUI through the
shell history and by GPUI's direct track through its own `LiveGraphHistory`
view of the same store — no frontend keeps a second sampling fold.

The System timeline adapter implements the application read port over the
authoritative telemetry rings. Window, gap and summary rules stay in
application; frontends receive immutable projections and hold only native
rendering caches.

## Timestamped trend samples

`presentation::trend::timestamped_window` is the rendering-neutral read of a
live graph: it returns the accepted samples as `core::metrics::MetricSample`
values, each carrying the instant the observation was MEASURED. The
value-only `trend::window` is the line-drawing projection of the SAME
accepted samples and stays index-aligned with it.

A sample without a measurement instant is a gap: the surface shows the shared
dash, never a `-N` sample offset, and the acceptance time is never relabelled
as the sampling time. `sample_timestamp_text` renders the instant with the
live local-time rules.

## SMART remediation

`presentation::smart_tool::smart_tool_guidance` projects a missing SMART
tool's remediation: the platform-neutral probe command from the contract plus
the platform's install command when the platform actually knows one. A
frontend copies the command; it composes none.

## Partition health

`presentation::storage_health::partition_health_facts` pairs each partition
with its filesystem health fact, using the contract's own link. A partition
with no observation has no fact (not a healthy one), and no frontend matches
mount-point strings.
