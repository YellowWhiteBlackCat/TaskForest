# Collection rules

Shared command lifecycle stays in `taskmanager-platform-portable`; this
crate only chooses fixed argv and parses macOS output.

The current safe process source exposes only a second-resolution
compatibility start time, so target-scoped resources, destructive control,
and process resource reveal fail closed as `Unsupported` until a precise
same-handle identity boundary exists.

Launchd inventory constructs typed `ServiceItem` rows and leaves the
canonical relation graph empty when the bounded provider has no relationship
facts; it never writes compatibility relation strings.

CPU and memory providers build canonical scalar/per-core and optional
groups, then construct each domain snapshot once. Compatibility JSON
projection stays inside core; unsupported memory enrichments remain typed
unavailable.

GPU inventory constructs typed dedicated/aggregate capacity observations
once; unsupported live scalars and throttle capability remain explicit typed
absence. `system_profiler` exposes model/vendor/device/revision facts but no
verified per-adapter stable identity, so non-empty rows keep their brand and
capacity while `device_id` remains empty and generation remains invalid. Such
rows report `Partial(Unsupported)`; duplicate or malformed rows also keep the
discovery refresh fail-closed, so an omitted prior identity is not marked
absent. A complete inventory confirms absence only for identities it can
reconcile (or confirms all prior identities absent when the array is empty);
the current profiler schema supplies no identity-bearing non-empty rows.
Optional ioreg utilization is a separate enrichment source and cannot change
that discovery outcome. The adapter never writes schema-v1 GPU mirrors.

macOS composition owns provider selection, not positional runtime wiring.
