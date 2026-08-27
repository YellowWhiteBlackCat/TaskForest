# ADR-013: Independent Process Insight Facets

Status: accepted

## Context

Process Insights previously crossed every boundary as one aggregate request and one
blocking provider call. Linux then collected socket/eBPF accounting, vendor GPU
telemetry, rlimits/cgroup resource facts, and isolation markers sequentially. A slow
or permission-gated source therefore delayed unrelated facts, and a PID reuse during
one of the slow calls could combine observations from different process generations.

The application-facing `FrozenProcessIdentity::start_time_secs` and Linux
`ProcessIdentity::start_token` are intentionally different identity schemes. The
former freezes a selected process row; the latter is the raw `/proc/<pid>/stat`
start-time tick token. Their numeric values and units are not comparable.

## Decision

Process Insights has four standard capabilities:

- `process.insights.network`;
- `process.insights.gpu`;
- `process.insights.resources`;
- `process.insights.isolation`.

Each capability has its own application request/event contract, provider SPI trait,
bounded runtime queue, executor thread, provider identity, health state, and Linux
collector state. Network owns eBPF accounting and traffic rates; GPU owns vendor API
enrichment and engine rates; resources owns rlimit/cgroup observation; isolation
reads its own cgroup/environment markers. There is no aggregate Linux collector:
each domain owns its own state, identity sandwich, root-injectable fixture seam, and
typed failure.

`PlatformClient::submit_process_insights` is the production orchestration boundary.
It allocates one global monotonically increasing application revision, begins a
`ProcessInsightsProjection`, submits
all four facets independently, records one request ID and facet per accepted request,
and returns the four individual submission results plus the immediate typed partial
projection. A queue failure marks only that facet unavailable; accepted siblings are
not rolled back or orphaned. The client stores only this scalar, not a history keyed
by viewed process generations. Revision exhaustion is typed and never silently reuses
the maximum value. Frontends neither fan out requests nor allocate revisions.

Every facet event carries the frozen target, application revision, and a
`ProcessInsightSnapshot<T>` with the provider-native raw identity. The application
projection accepts data only for its currently selected frozen target and exact
revision. The first accepted raw identity becomes the cross-domain identity; a
different raw identity rejects that facet. Late, duplicate, stale-revision, or
different-frozen-generation events never replace current state. Every terminal
success, failure, or ignored event removes its corresponding pending request.
Provider facet events are reducer-internal: frontends receive only the application
projection. A stale revision, target mismatch, or conflicting raw identity produces
a typed diagnostic rather than leaking the raw event as a successful batch item.

Each Linux collector reads the raw procfs identity immediately before its domain I/O
and again immediately afterward. The post-read `(pid, start_token)` must exactly equal
the identity embedded in the facet snapshot or the provider returns
`IdentityChanged`. Providers additionally validate the frozen application target at
their boundary. Neither validation substitutes for the other, and code never compares
frozen seconds with procfs ticks.

There is no aggregate `process.insights` capability, request port, event queue,
provider SPI, runtime binding, or frontend raw stream. `ProcessTelemetrySnapshot`
remains the complete UI read model materialized by `complete_snapshot` only after all
four facet states are terminal. Current domains keep their values; failed domains
receive default values plus a typed unavailable `DeviceState` with no fabricated
last-success timestamp. Any usable current domain keeps the complete snapshot
renderable, while an all-unavailable terminal projection remains an error.

## Consequences

Slow eBPF or GPU vendor work cannot block resource or isolation completion. Permission
and availability truth is domain-specific in both the capability catalog and the
application projection. Partial process insight state becomes observable immediately,
while the frontend never schedules or receives an aggregate provider operation.

The system uses more queues and worker threads, but their ownership and failure
domains now match the actual native I/O. Linux tests must prove raw identity reuse is
rejected, runtime tests must prove slow domains do not serialize siblings, and
architecture tests must reject aggregate provider/runtime/GPUI regressions.
